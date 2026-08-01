//! Domain service: extract 2D slices from Tier-2 voxel fields.
//! t2f6-field-inspector.
//!
//! `FieldSlicer` is a stateless domain service (ADR-0001: services may
//! depend on values) built EXCLUSIVELY on the five fields' existing
//! *public* per-voxel accessors — `dose_at` / `concentration_at` /
//! `strain_at` / `stress_at` / `temperature_at` plus `dimensions()`.
//! No `pub(crate)` widening, no `as_array_view()` added to the
//! part-bbox fields.
//!
//! # `PhotoinitiatorField` coordinate gap
//!
//! Unlike the other four fields, `PhotoinitiatorField` carries no
//! `voxel_size_mm()` / `bbox_min_mm()` of its own — it is dimension-
//! locked to its companion `CureField` (both are always installed
//! together via `PrintSimulation::set_voxel_fields`) but does not
//! store the coordinate metadata itself. Rather than widen
//! `PhotoinitiatorField`'s persisted shape to add fields it has never
//! carried (a real persistence-format change, out of scope here),
//! [`FieldSlicer::slice`] takes `voxel_size_mm` / `bbox_min_mm` as
//! explicit caller-supplied parameters for ALL five field kinds. For
//! Cure/Strain/Stress/Thermal the caller reads these directly off the
//! field; for Photoinitiator the caller reads them off the paired
//! `CureField`.

#![cfg(feature = "field-sim")]

use thiserror::Error;

use crate::values::{
    CureField,
    LayerHeightSeq,
    PhotoinitiatorField,
    StrainField,
    StressField,
    ThermalField,
    field_slice::{
        FieldSlice,
        FieldSliceError,
        SliceAxis,
        SlicePlane,
    },
};

/// Borrow of one of the five Tier-2 voxel fields a simulation may
/// carry. The kind lives only here — no new domain enum duplicates
/// `sidecar::format::FieldKind` (a persistence tag); a CLI-level
/// `ValueEnum` maps user input directly to a variant.
#[derive(Debug, Clone, Copy)]
pub enum FieldRef<'a> {
    Cure(&'a CureField),
    Photoinitiator(&'a PhotoinitiatorField),
    Strain(&'a StrainField),
    Stress(&'a StressField),
    Thermal(&'a ThermalField),
}

impl<'a> FieldRef<'a> {
    /// Voxel-grid dimensions `(nx, ny, nz)`.
    pub fn dimensions(&self) -> (u32, u32, u32) {
        match self {
            Self::Cure(f) => f.dimensions(),
            Self::Photoinitiator(f) => f.dimensions(),
            Self::Strain(f) => f.dimensions(),
            Self::Stress(f) => f.dimensions(),
            Self::Thermal(f) => f.dimensions(),
        }
    }

    /// Physical unit label for this field's scalar (post-reduction)
    /// values.
    pub fn unit_label(&self) -> &'static str {
        match self {
            Self::Cure(_) => "mJ/cm²",
            Self::Photoinitiator(_) => "",
            Self::Strain(_) => "",
            Self::Stress(_) => "MPa",
            Self::Thermal(_) => "°C",
        }
    }

    /// `true` for the four part-bbox-anchored, layer-stacked fields
    /// (cure / photoinitiator / strain / stress) whose Z axis is the
    /// print's layer index; `false` for `Thermal`, which is
    /// vat-envelope-anchored with a spatial Z axis
    /// (`docs/patterns/thermal-field-z-dim-is-spatial.md`). Used by
    /// `FieldSlicer::resolve_index` to select the Z-resolution branch.
    fn is_layer_stacked(&self) -> bool {
        !matches!(self, Self::Thermal(_))
    }

    /// Scalar value at voxel `(ix, iy, iz)`. Strain and stress reduce
    /// through the PRODUCTION `StrainTensor::magnitude` /
    /// `StressTensor::von_mises_mpa` helpers — never re-derived here
    /// (`docs/patterns/anti/test-mirrors-production-formula.md`).
    fn value_at(&self, ix: u32, iy: u32, iz: u32) -> Result<f32, FieldSlicerError> {
        match self {
            Self::Cure(f) => f
                .dose_at(ix, iy, iz)
                .map_err(|e| FieldSlicerError::VoxelAccess(e.to_string())),
            Self::Photoinitiator(f) => f
                .concentration_at(ix, iy, iz)
                .map_err(|e| FieldSlicerError::VoxelAccess(e.to_string())),
            Self::Strain(f) => f
                .strain_at(ix, iy, iz)
                .map(|t| t.magnitude())
                .map_err(|e| FieldSlicerError::VoxelAccess(e.to_string())),
            Self::Stress(f) => {
                let tensor = f
                    .stress_at(ix, iy, iz)
                    .map_err(|e| FieldSlicerError::VoxelAccess(e.to_string()))?;
                tensor
                    .von_mises_mpa()
                    .map_err(|e| FieldSlicerError::TensorReduction(e.to_string()))
            }
            Self::Thermal(f) => f
                .temperature_at(ix, iy, iz)
                .map_err(|e| FieldSlicerError::VoxelAccess(e.to_string())),
        }
    }
}

/// Errors from [`FieldSlicer::slice`] / [`FieldSlicer::resolve_index`].
#[derive(Debug, Clone, PartialEq, Error)]
pub enum FieldSlicerError {
    /// The fixed-axis index passed to `slice()` (or resolved by
    /// `resolve_index()`) is past the field's extent on that axis.
    #[error(
        "FieldSlicer: {axis:?} index {index} is out of range for this field — valid range is 0..{valid_exclusive_max}"
    )]
    IndexOutOfRange {
        axis: SliceAxis,
        index: u32,
        valid_exclusive_max: u32,
    },
    /// A `resolve_index()` world-mm query fell outside the field's
    /// physical extent on `axis`.
    #[error(
        "FieldSlicer: {axis:?}={value_mm} mm is outside this field's extent [{min_mm}, {max_mm}] mm"
    )]
    WorldCoordOutOfRange {
        axis: SliceAxis,
        value_mm: f32,
        min_mm: f32,
        max_mm: f32,
    },
    /// `resolve_index()` was asked to address a layer-stacked field's
    /// Z axis by millimetres, but the simulation carries no
    /// `layer_height_provenance` (STL / area-only run — no CTB-derived
    /// per-layer heights exist). Falling back to `iz * voxel_size_mm`
    /// would be `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`;
    /// the caller must address this field by voxel index instead.
    #[error(
        "FieldSlicer: addressing {axis:?} by millimetres requires per-layer heights, but this \
         simulation has no layer_height_provenance (STL / area-only run) — address this field \
         by voxel index instead"
    )]
    MissingLayerHeightProvenance { axis: SliceAxis },
    /// A per-voxel accessor returned an error despite `FieldSlicer`
    /// having pre-validated the index against `dimensions()` —
    /// defensive; should not occur in practice.
    #[error("FieldSlicer: voxel access failed — {0}")]
    VoxelAccess(String),
    /// Tensor-to-scalar reduction (von Mises / magnitude) failed —
    /// bubbled from `StressTensor`/`StrainTensor`'s own defensive
    /// catastrophic-cancellation guard.
    #[error("FieldSlicer: tensor reduction failed — {0}")]
    TensorReduction(String),
    #[error(transparent)]
    SliceConstruction(#[from] FieldSliceError),
}

/// Stateless domain service. Extracts 2D [`FieldSlice`]s from any of
/// the five Tier-2 voxel fields and resolves world-mm addressing to
/// voxel indices — the single place the two Z-index semantics
/// (layer-stacked cumulative height vs. spatial thermal) diverge, so
/// the divergence is testable in isolation.
pub struct FieldSlicer;

impl FieldSlicer {
    /// Extract the 2D slice through `field` in `plane`, at `index`
    /// along the plane's fixed axis (`plane.fixed_axis()`).
    ///
    /// `voxel_size_mm` / `bbox_min_mm` are caller-supplied (see module
    /// docs — required uniformly across all five variants because
    /// `PhotoinitiatorField` alone has no such accessors of its own).
    pub fn slice(
        field: &FieldRef<'_>,
        plane: SlicePlane,
        index: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
    ) -> Result<FieldSlice, FieldSlicerError> {
        let (nx, ny, nz) = field.dimensions();
        let fixed_axis = plane.fixed_axis();
        let fixed_dim = match fixed_axis {
            SliceAxis::X => nx,
            SliceAxis::Y => ny,
            SliceAxis::Z => nz,
        };
        if index >= fixed_dim {
            return Err(FieldSlicerError::IndexOutOfRange {
                axis: fixed_axis,
                index,
                valid_exclusive_max: fixed_dim,
            });
        }
        let (nu, nv) = match plane {
            SlicePlane::Xy => (nx, ny),
            SlicePlane::Xz => (nx, nz),
            SlicePlane::Yz => (ny, nz),
        };
        let mut values = Vec::with_capacity(nu as usize * nv as usize);
        // Row-major: v (slow) outer, u (fast) inner — matches
        // `FieldSlice::value_at`'s `v * nu + u` indexing.
        for v in 0..nv {
            for u in 0..nu {
                let (ix, iy, iz) = match plane {
                    SlicePlane::Xy => (u, v, index),
                    SlicePlane::Xz => (u, index, v),
                    SlicePlane::Yz => (index, u, v),
                };
                values.push(field.value_at(ix, iy, iz)?);
            }
        }
        let (origin_u, origin_v) = match plane {
            SlicePlane::Xy => (bbox_min_mm[0], bbox_min_mm[1]),
            SlicePlane::Xz => (bbox_min_mm[0], bbox_min_mm[2]),
            SlicePlane::Yz => (bbox_min_mm[1], bbox_min_mm[2]),
        };
        let slice = FieldSlice::new(
            plane,
            index,
            nu,
            nv,
            values,
            field.unit_label(),
            [origin_u, origin_v],
            [voxel_size_mm, voxel_size_mm],
        )?;
        Ok(slice)
    }

    /// Resolve a world-mm address on `axis` to a voxel index. THE
    /// load-bearing correctness boundary of this service — TWO
    /// branches for Z, selected by `field.is_layer_stacked()`:
    ///
    /// - **Layer-stacked** (cure / photoinitiator / strain / stress),
    ///   Z axis: resolved through CUMULATIVE per-layer heights from
    ///   `layer_heights` (a simulation's
    ///   `PrintSimulation::layer_height_provenance()`'s
    ///   `LayerHeightSeq`), never `iz * voxel_size_mm`
    ///   (`docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`).
    ///   `layer_heights = None` (STL / area-only runs — no CTB-derived
    ///   per-layer heights exist) returns
    ///   `MissingLayerHeightProvenance`; the caller must address the
    ///   field by voxel index instead.
    /// - **Thermal**, Z axis: resolved as
    ///   `(z_mm - bbox_min_mm[2]) / voxel_size_mm` over the vat
    ///   envelope — deliberately NOT layer-stacked; NEVER calls
    ///   `world_at_voxel_center()`, which is documented as
    ///   intentionally wrong for this purpose.
    /// - **X and Y**, all five field kinds: resolved through
    ///   `bbox_min_mm[axis] + i * voxel_size_mm`.
    ///
    /// Coordinates at the exact upper boundary of the field's extent
    /// snap to the nearest (last) voxel rather than erroring — mirrors
    /// `ThermalField::temperature_at_world`'s extreme-face convention.
    pub fn resolve_index(
        field: &FieldRef<'_>,
        axis: SliceAxis,
        value_mm: f32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
        layer_heights: Option<&LayerHeightSeq>,
    ) -> Result<u32, FieldSlicerError> {
        let (nx, ny, nz) = field.dimensions();
        match axis {
            SliceAxis::X => {
                resolve_lateral_index(value_mm, bbox_min_mm[0], voxel_size_mm, nx, SliceAxis::X)
            }
            SliceAxis::Y => {
                resolve_lateral_index(value_mm, bbox_min_mm[1], voxel_size_mm, ny, SliceAxis::Y)
            }
            SliceAxis::Z => {
                if field.is_layer_stacked() {
                    resolve_layer_stacked_z_index(value_mm, bbox_min_mm[2], layer_heights, nz)
                } else {
                    resolve_lateral_index(value_mm, bbox_min_mm[2], voxel_size_mm, nz, SliceAxis::Z)
                }
            }
        }
    }
}

/// X/Y (and thermal-Z) resolution: uniform `voxel_size_mm` pitch from
/// `origin_mm`. The exact-upper-boundary coordinate snaps to the last
/// voxel (nearest-voxel fallback), matching
/// `ThermalField::temperature_at_world`'s extreme-face convention.
fn resolve_lateral_index(
    value_mm: f32,
    origin_mm: f32,
    voxel_size_mm: f32,
    dim: u32,
    axis: SliceAxis,
) -> Result<u32, FieldSlicerError> {
    let max_mm = origin_mm + dim as f32 * voxel_size_mm;
    if !value_mm.is_finite() || value_mm < origin_mm || value_mm > max_mm {
        return Err(FieldSlicerError::WorldCoordOutOfRange {
            axis,
            value_mm,
            min_mm: origin_mm,
            max_mm,
        });
    }
    let raw = ((value_mm - origin_mm) / voxel_size_mm).floor();
    let idx = raw.clamp(0.0, (dim - 1) as f32) as u32;
    Ok(idx)
}

/// Layer-stacked Z resolution: walk the CUMULATIVE per-layer heights
/// (µm, converted to mm) from `layer_heights`, never
/// `iz * voxel_size_mm`. `layer_heights = None` returns
/// `MissingLayerHeightProvenance` — the caller must address by index.
/// Absolute tolerance (mm) on the layer-stack Z bounds check. Cumulative
/// µm-per-layer summation over many layers accrues f32 rounding error
/// (e.g. 50+30+20+40 µm sums to 0.139_999_99 mm rather than the exact
/// 0.14 mm) — without this margin, a query at the nominal top-of-stack
/// mm value would spuriously reject as `WorldCoordOutOfRange`. 1e-4 mm
/// (0.1 µm) is far below any physical layer thickness (tens of µm) so
/// it cannot mask a genuinely out-of-range query.
const Z_BOUNDARY_EPSILON_MM: f32 = 1e-4;

fn resolve_layer_stacked_z_index(
    value_mm: f32,
    bbox_min_z: f32,
    layer_heights: Option<&LayerHeightSeq>,
    nz: u32,
) -> Result<u32, FieldSlicerError> {
    let seq = layer_heights
        .ok_or(FieldSlicerError::MissingLayerHeightProvenance { axis: SliceAxis::Z })?;
    let n = (nz as usize).min(seq.len());
    if n == 0 || !value_mm.is_finite() {
        return Err(FieldSlicerError::WorldCoordOutOfRange {
            axis: SliceAxis::Z,
            value_mm,
            min_mm: bbox_min_z,
            max_mm: bbox_min_z,
        });
    }
    // boundaries[iz] is the world-mm bottom of layer iz; boundaries[n]
    // is the top of the stack. Layer iz spans [boundaries[iz], boundaries[iz+1]).
    let mut boundaries = Vec::with_capacity(n + 1);
    boundaries.push(bbox_min_z);
    let mut cursor_mm = bbox_min_z;
    for iz in 0..n {
        let h_mm = seq.get(iz).unwrap_or(0.0) / 1000.0;
        cursor_mm += h_mm;
        boundaries.push(cursor_mm);
    }
    let top_mm = cursor_mm;
    if value_mm < bbox_min_z - Z_BOUNDARY_EPSILON_MM || value_mm > top_mm + Z_BOUNDARY_EPSILON_MM {
        return Err(FieldSlicerError::WorldCoordOutOfRange {
            axis: SliceAxis::Z,
            value_mm,
            min_mm: bbox_min_z,
            max_mm: top_mm,
        });
    }
    let mut resolved = None;
    for iz in 0..n {
        // The `|| iz == n - 1` fallback snaps the exact-top-boundary
        // coordinate to the last layer (nearest-voxel convention).
        if value_mm < boundaries[iz + 1] || iz == n - 1 {
            resolved = Some(iz as u32);
            break;
        }
    }
    Ok(resolved.expect(
        "loop always sets `resolved` via the `iz == n - 1` fallback on its final iteration, given n > 0",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{
        FieldStatsScope,
        StrainTensor,
        StressTensor,
    };

    const FIXTURE_MSG: &str = "test fixture: literal in-range indices and validly-constructed fields satisfy FieldSlicer preconditions";

    /// 4×4×3 field filled with `f(ix,iy,iz) = ix*100 + iy*10 + iz` — a
    /// transposition bug (swapped axes anywhere in the index math)
    /// changes the pulled values immediately, since all three weights
    /// are distinct.
    fn fill_cure_field_with_index_pattern(nx: u32, ny: u32, nz: u32) -> CureField {
        let mut f = CureField::new(nx, ny, nz, 0.5, [0.0, 0.0, 0.0]).expect(FIXTURE_MSG);
        for ix in 0..nx {
            for iy in 0..ny {
                for iz in 0..nz {
                    let value = (ix * 100 + iy * 10 + iz) as f32;
                    f.add_dose(ix, iy, iz, value).expect(FIXTURE_MSG);
                }
            }
        }
        f
    }

    fn fill_thermal_field_with_index_pattern(nx: u32, ny: u32, nz: u32) -> ThermalField {
        let mut f = ThermalField::new(nx, ny, nz, 0.5, [0.0, 0.0, 0.0], 25.0).expect(FIXTURE_MSG);
        for ix in 0..nx {
            for iy in 0..ny {
                for iz in 0..nz {
                    f.as_array_mut()[(ix as usize, iy as usize, iz as usize)] =
                        (ix * 100 + iy * 10 + iz) as f32;
                }
            }
        }
        f
    }

    // ---- transposition-fill correctness, all three planes (Cure) ----

    #[test]
    fn cure_field_xy_plane_pulls_correct_shape_and_transposed_values() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // Fix Z at 2: nu=nx=4, nv=ny=4; value(u,v) = f(u, v, 2).
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 2, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.nu(), 4);
        assert_eq!(slice.nv(), 4);
        assert_eq!(slice.unit_label(), "mJ/cm²");
        for u in 0..4u32 {
            for v in 0..4u32 {
                let expected = (u * 100 + v * 10 + 2) as f32;
                assert_eq!(
                    slice.value_at(u, v),
                    Some(expected),
                    "XY slice mismatch at u={u},v={v}"
                );
            }
        }
    }

    #[test]
    fn cure_field_xz_plane_pulls_correct_shape_and_transposed_values() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // Fix Y at 1: nu=nx=4, nv=nz=3; value(u,v) = f(u, 1, v).
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xz, 1, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.nu(), 4);
        assert_eq!(slice.nv(), 3);
        for u in 0..4u32 {
            for v in 0..3u32 {
                let expected = (u * 100 + 10 + v) as f32;
                assert_eq!(
                    slice.value_at(u, v),
                    Some(expected),
                    "XZ slice mismatch at u={u},v={v}"
                );
            }
        }
    }

    #[test]
    fn cure_field_yz_plane_pulls_correct_shape_and_transposed_values() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // Fix X at 3: nu=ny=4, nv=nz=3; value(u,v) = f(3, u, v).
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Yz, 3, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.nu(), 4);
        assert_eq!(slice.nv(), 3);
        for u in 0..4u32 {
            for v in 0..3u32 {
                let expected = (300 + u * 10 + v) as f32;
                assert_eq!(
                    slice.value_at(u, v),
                    Some(expected),
                    "YZ slice mismatch at u={u},v={v}"
                );
            }
        }
    }

    #[test]
    fn thermal_field_xy_plane_pulls_correct_shape_and_transposed_values() {
        let field = fill_thermal_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Thermal(&field);
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 1, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.unit_label(), "°C");
        assert_eq!(slice.nu(), 4);
        assert_eq!(slice.nv(), 4);
        for u in 0..4u32 {
            for v in 0..4u32 {
                let expected = (u * 100 + v * 10 + 1) as f32;
                assert_eq!(slice.value_at(u, v), Some(expected));
            }
        }
    }

    // ---- strain / stress reduce via PRODUCTION helpers, cross-checked ----

    #[test]
    fn strain_field_slice_reduces_via_production_magnitude_and_cross_checks_layer_max() {
        let mut field = StrainField::new(3, 3, 2, 0.5, [0.0, 0.0, 0.0]).expect(FIXTURE_MSG);
        let small = StrainTensor::from_isotropic(-0.005).expect(FIXTURE_MSG);
        let large = StrainTensor::from_isotropic(-0.02).expect(FIXTURE_MSG);
        field.lock_strain_at(0, 0, 1, small).expect(FIXTURE_MSG);
        field.lock_strain_at(1, 1, 1, large).expect(FIXTURE_MSG);
        let field_ref = FieldRef::Strain(&field);
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 1, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.unit_label(), "");
        let stats = slice.stats(FieldStatsScope::All).expect(FIXTURE_MSG);
        // Cross-check against the PRODUCTION reduction
        // (StrainField::magnitude_layer_max) — never re-derive the
        // Frobenius formula in the slicer or its tests.
        let expected_max = field.magnitude_layer_max(1).expect(FIXTURE_MSG);
        assert!(
            (stats.max - expected_max).abs() < 1e-5,
            "FieldSlicer's strain reduction ({}) must match production \
             StrainField::magnitude_layer_max ({expected_max})",
            stats.max
        );
    }

    #[test]
    fn stress_field_slice_reduces_via_production_von_mises_and_cross_checks_layer_max() {
        let mut field = StressField::new(3, 3, 2, 0.5, [0.0, 0.0, 0.0]).expect(FIXTURE_MSG);
        let s50 = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(FIXTURE_MSG);
        let s100 = StressTensor::new(100.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(FIXTURE_MSG);
        field.accumulate_at(0, 0, 1, s50).expect(FIXTURE_MSG);
        field.accumulate_at(1, 1, 1, s100).expect(FIXTURE_MSG);
        let field_ref = FieldRef::Stress(&field);
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 1, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.unit_label(), "MPa");
        let stats = slice.stats(FieldStatsScope::All).expect(FIXTURE_MSG);
        // Cross-check against the PRODUCTION reduction
        // (StressField::von_mises_layer_max), not a re-derivation.
        let expected_max = field.von_mises_layer_max(1).expect(FIXTURE_MSG);
        assert!(
            (stats.max - expected_max).abs() < 1e-5,
            "FieldSlicer's stress reduction ({}) must match production \
             StressField::von_mises_layer_max ({expected_max})",
            stats.max
        );
        assert!((expected_max - 100.0).abs() < 1e-3);
    }

    // ---- photoinitiator: coordinates supplied externally (see module docs) ----

    #[test]
    fn photoinitiator_field_slice_pulls_correct_shape_and_values() {
        let mut field = PhotoinitiatorField::new(4, 4, 3, 1.0).expect(FIXTURE_MSG);
        field.deplete(2, 1, 0, 0.05, 50.0).expect(FIXTURE_MSG);
        let field_ref = FieldRef::Photoinitiator(&field);
        // voxel_size_mm/bbox_min_mm supplied externally — PhotoinitiatorField
        // has no such accessors of its own (see module docs).
        let slice = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 0, 0.5, [0.0, 0.0, 0.0])
            .expect(FIXTURE_MSG);
        assert_eq!(slice.nu(), 4);
        assert_eq!(slice.nv(), 4);
        assert_eq!(slice.unit_label(), "");
        let depleted = field.concentration_at(2, 1, 0).expect(FIXTURE_MSG);
        assert!((slice.value_at(2, 1).expect(FIXTURE_MSG) - depleted).abs() < 1e-6);
        assert_eq!(slice.value_at(0, 0), Some(1.0));
        assert_eq!(slice.value_at(3, 3), Some(1.0));
    }

    // ---- out-of-range index ----

    #[test]
    fn slice_out_of_range_z_index_returns_typed_error_naming_valid_range() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        let err = FieldSlicer::slice(&field_ref, SlicePlane::Xy, 3, 0.5, [0.0, 0.0, 0.0])
            .expect_err("test fixture: nz=3 so index 3 is deliberately out of range, so Err is the expected outcome");
        assert!(
            matches!(
                err,
                FieldSlicerError::IndexOutOfRange {
                    axis: SliceAxis::Z,
                    index: 3,
                    valid_exclusive_max: 3
                }
            ),
            "expected IndexOutOfRange {{axis: Z, index: 3, valid_exclusive_max: 3}}, got {err:?}"
        );
    }

    #[test]
    fn slice_out_of_range_x_and_y_index_also_typed() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        let err_x = FieldSlicer::slice(&field_ref, SlicePlane::Yz, 4, 0.5, [0.0, 0.0, 0.0])
            .expect_err("test fixture: nx=4 so index 4 is deliberately out of range, so Err is the expected outcome");
        assert!(
            matches!(
                err_x,
                FieldSlicerError::IndexOutOfRange {
                    axis: SliceAxis::X,
                    index: 4,
                    valid_exclusive_max: 4
                }
            ),
            "expected IndexOutOfRange {{axis: X, index: 4, valid_exclusive_max: 4}}, got {err_x:?}"
        );
        let err_y = FieldSlicer::slice(&field_ref, SlicePlane::Xz, 4, 0.5, [0.0, 0.0, 0.0])
            .expect_err("test fixture: ny=4 so index 4 is deliberately out of range, so Err is the expected outcome");
        assert!(
            matches!(
                err_y,
                FieldSlicerError::IndexOutOfRange {
                    axis: SliceAxis::Y,
                    index: 4,
                    valid_exclusive_max: 4
                }
            ),
            "expected IndexOutOfRange {{axis: Y, index: 4, valid_exclusive_max: 4}}, got {err_y:?}"
        );
    }

    // ---- resolve_index: X/Y lateral resolution (all field kinds share this) ----

    #[test]
    fn resolve_index_x_and_y_resolve_through_bbox_plus_i_times_voxel_size() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // voxel_size=0.5mm, bbox_min=[10.0, 20.0, 0.0]: voxel 2 on X spans
        // [11.0, 11.5) mm; voxel 3 on Y spans [21.5, 22.0) mm.
        let ix = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::X,
            11.2,
            0.5,
            [10.0, 20.0, 0.0],
            None,
        )
        .expect(FIXTURE_MSG);
        assert_eq!(ix, 2);
        let iy = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Y,
            21.9,
            0.5,
            [10.0, 20.0, 0.0],
            None,
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iy, 3);
    }

    #[test]
    fn resolve_index_lateral_out_of_range_returns_typed_world_coord_error() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // Field X extent is [0.0, 2.0) mm (4 voxels x 0.5mm); 5.0mm is
        // well past it.
        let err = FieldSlicer::resolve_index(&field_ref, SliceAxis::X, 5.0, 0.5, [0.0; 3], None)
            .expect_err("test fixture: 5.0mm deliberately exceeds the 2.0mm field extent, so Err is the expected outcome");
        assert!(
            matches!(
                err,
                FieldSlicerError::WorldCoordOutOfRange {
                    axis: SliceAxis::X,
                    ..
                }
            ),
            "expected WorldCoordOutOfRange {{axis: X, ..}}, got {err:?}"
        );
    }

    #[test]
    fn resolve_index_lateral_exact_upper_boundary_snaps_to_last_voxel() {
        let field = fill_cure_field_with_index_pattern(4, 4, 3);
        let field_ref = FieldRef::Cure(&field);
        // Field X extent is exactly [0.0, 2.0] mm inclusive of the boundary.
        let ix = FieldSlicer::resolve_index(&field_ref, SliceAxis::X, 2.0, 0.5, [0.0; 3], None)
            .expect(FIXTURE_MSG);
        assert_eq!(
            ix, 3,
            "exact upper boundary must snap to the last voxel (nx-1)"
        );
    }

    // ---- resolve_index: Z branch selection ----

    #[test]
    fn resolve_index_z_thermal_uses_bbox_and_voxel_size_not_layer_heights() {
        let field = fill_thermal_field_with_index_pattern(4, 4, 10);
        let field_ref = FieldRef::Thermal(&field);
        // voxel_size=0.5mm, bbox_min_z=0.0: z=1.2mm -> floor(1.2/0.5)=2.
        let iz = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            1.2,
            0.5,
            [0.0, 0.0, 0.0],
            None, // no layer_heights supplied — Thermal must not need it
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iz, 2);
    }

    #[test]
    fn resolve_index_z_layer_stacked_uses_cumulative_layer_heights() {
        // 4 layers, non-uniform heights (µm): 50, 30, 20, 40.
        // Cumulative boundaries (mm): 0, 0.05, 0.08, 0.10, 0.14.
        let seq = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 40.0]).expect(FIXTURE_MSG);
        let field = fill_cure_field_with_index_pattern(2, 2, 4);
        let field_ref = FieldRef::Cure(&field);
        // z=0.09mm falls in [0.08, 0.10) -> layer index 2.
        let iz = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            0.09,
            0.5, // voxel_size_mm — irrelevant to Z on a layer-stacked field
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iz, 2);
        // z=0.0mm (bottom) -> layer 0; z=0.06mm -> layer 1.
        let iz0 = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            0.0,
            0.5,
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iz0, 0);
        let iz1 = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            0.06,
            0.5,
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iz1, 1);
    }

    #[test]
    fn resolve_index_z_layer_stacked_exact_top_boundary_snaps_to_last_layer() {
        let seq = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 40.0]).expect(FIXTURE_MSG);
        let field = fill_cure_field_with_index_pattern(2, 2, 4);
        let field_ref = FieldRef::Cure(&field);
        // Total stack height = 140 µm = 0.14 mm exactly.
        let iz = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            0.14,
            0.5,
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect(FIXTURE_MSG);
        assert_eq!(iz, 3);
    }

    #[test]
    fn resolve_index_z_layer_stacked_out_of_range_returns_typed_error() {
        let seq = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 40.0]).expect(FIXTURE_MSG);
        let field = fill_cure_field_with_index_pattern(2, 2, 4);
        let field_ref = FieldRef::Cure(&field);
        let err = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            1.0, // well past the 0.14mm stack top
            0.5,
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect_err("test fixture: 1.0mm deliberately exceeds the 0.14mm layer stack, so Err is the expected outcome");
        assert!(
            matches!(
                err,
                FieldSlicerError::WorldCoordOutOfRange {
                    axis: SliceAxis::Z,
                    ..
                }
            ),
            "expected WorldCoordOutOfRange {{axis: Z, ..}}, got {err:?}"
        );
    }

    #[test]
    fn resolve_index_z_layer_stacked_absent_provenance_returns_typed_error() {
        // STL / area-only run: no LayerHeightSeq available. Must NOT
        // fall back to iz * voxel_size_mm
        // (docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md).
        let field = fill_cure_field_with_index_pattern(2, 2, 4);
        let field_ref = FieldRef::Cure(&field);
        let err = FieldSlicer::resolve_index(
            &field_ref,
            SliceAxis::Z,
            0.09,
            0.5,
            [0.0, 0.0, 0.0],
            None,
        )
        .expect_err(
            "test fixture: layer_heights deliberately omitted, so Err is the expected outcome",
        );
        assert!(
            matches!(
                err,
                FieldSlicerError::MissingLayerHeightProvenance { axis: SliceAxis::Z }
            ),
            "expected MissingLayerHeightProvenance {{axis: Z}}, got {err:?}"
        );
    }

    /// THE MANDATORY REGRESSION TEST (binding review condition,
    /// findings-issue-t2f6-adversarial-r2.yaml): for the SAME z_mm
    /// input, a cure field (layer-stacked, non-uniform layer heights)
    /// and a thermal field (spatial, `bbox_min_z + iz*voxel_size_mm`)
    /// MUST resolve to DIFFERENT voxel indices. A copy-paste collapse
    /// of the two branches would make this pass trivially with equal
    /// indices — the explicit inequality assertion catches that.
    #[test]
    fn resolve_index_z_resolutions_differ_between_cure_and_thermal_for_the_same_z_mm() {
        // Non-uniform layer heights (µm): 50, 30, 20, 40.
        // Cumulative boundaries (mm): 0, 0.05, 0.08, 0.10, 0.14.
        let seq = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 40.0]).expect(FIXTURE_MSG);
        let cure_field = fill_cure_field_with_index_pattern(2, 2, 4);
        let cure_ref = FieldRef::Cure(&cure_field);
        // Thermal field: coarse voxel_size_mm=0.5mm, bbox_min_z=0.0, plenty of Z voxels.
        let thermal_field = fill_thermal_field_with_index_pattern(2, 2, 10);
        let thermal_ref = FieldRef::Thermal(&thermal_field);

        let z_mm = 0.09_f32;

        // Cure (layer-stacked): 0.09mm falls in the cumulative bracket
        // [0.08, 0.10) -> layer index 2.
        let cure_index = FieldSlicer::resolve_index(
            &cure_ref,
            SliceAxis::Z,
            z_mm,
            0.5,
            [0.0, 0.0, 0.0],
            Some(&seq),
        )
        .expect(FIXTURE_MSG);
        assert_eq!(
            cure_index, 2,
            "cure Z index must come from cumulative layer heights"
        );

        // Thermal (spatial): floor(0.09 / 0.5) = 0.
        let thermal_index = FieldSlicer::resolve_index(
            &thermal_ref,
            SliceAxis::Z,
            z_mm,
            0.5,
            [0.0, 0.0, 0.0],
            None, // Thermal never needs layer_heights
        )
        .expect(FIXTURE_MSG);
        assert_eq!(
            thermal_index, 0,
            "thermal Z index must come from bbox_min_z + iz*voxel_size_mm"
        );

        assert_ne!(
            cure_index, thermal_index,
            "cure (layer-stacked, cumulative heights) and thermal (spatial, voxel_size_mm) \
             MUST resolve the same z_mm={z_mm} to DIFFERENT indices — a copy-paste collapse \
             of the two branches would make this pass trivially with equal indices"
        );
    }
}
