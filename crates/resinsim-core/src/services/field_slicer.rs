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

use crate::values::field_slice::{FieldSlice, FieldSliceError, SliceAxis, SlicePlane};
use crate::values::{CureField, PhotoinitiatorField, StrainField, StressField, ThermalField};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{FieldStatsScope, StrainTensor, StressTensor};

    const FIXTURE_MSG: &str =
        "test fixture: literal in-range indices and validly-constructed fields satisfy FieldSlicer preconditions";

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
        let s50 =
            StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(FIXTURE_MSG);
        let s100 =
            StressTensor::new(100.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(FIXTURE_MSG);
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
}
