//! 3D voxel-resolved cure dose field — `CureField` value object.
//!
//! ADR-0017, t2f1. A dense `ndarray::Array3<f32>` indexed by `[ix, iy, iz]`
//! holding cumulative absorbed dose (mJ/cm²) at each voxel of a printed
//! part's bounding box. Bbox-anchored: the field carries `bbox_min_mm` so
//! consumers can map world coordinates ↔ voxel indices.
//!
//! # Coordinates
//!
//! - `voxel_size_mm`: physical edge length of one voxel, identical on all
//!   three axes. Validated finite > 0.
//! - `bbox_min_mm`: world-space corner `(x_min, y_min, z_min)` of voxel
//!   `[0, 0, 0]`. The world-space corner of voxel `[ix, iy, iz]` is
//!   `bbox_min_mm + (ix, iy, iz) * voxel_size_mm`. Voxel CENTRES sit at
//!   `bbox_min_mm + (ix + 0.5, iy + 0.5, iz + 0.5) * voxel_size_mm` —
//!   `world_at_voxel_center` provides this.
//!
//! # NaN policy
//!
//! Two-layer defence (`docs/patterns/nan-two-layer-defence.md`):
//! constructor + every mutating accessor checks `is_finite` AND value-range
//! before touching state. NaN dose values cannot enter the field via the
//! public API.
//!
//! # Memory
//!
//! Dense f32. For Mars 5 Ultra envelope (153×78×165 mm) at 0.2 mm voxels,
//! a typical 50×50×100 mm part allocates ≈ 62 MB (see ADR-0017 §"Dense vs
//! sparse"). The `total_bytes()` helper exposes the cost so callers can
//! pre-flight memory budgets before allocation.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::CureDepth;

/// Errors from `CureField` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CureFieldError {
    #[error("CureField dimensions must all be positive, got {nx}×{ny}×{nz}")]
    InvalidDimensions { nx: u32, ny: u32, nz: u32 },
    #[error("CureField voxel_size_mm must be finite and > 0, got {0}")]
    InvalidVoxelSize(f32),
    #[error("CureField bbox_min_mm must be finite on all axes, got ({x}, {y}, {z})")]
    InvalidBboxMin { x: f32, y: f32, z: f32 },
    #[error("CureField index ({ix}, {iy}, {iz}) out of bounds for {nx}×{ny}×{nz}")]
    OutOfBounds {
        ix: u32,
        iy: u32,
        iz: u32,
        nx: u32,
        ny: u32,
        nz: u32,
    },
    #[error("CureField dose values must be finite and >= 0, got {0}")]
    InvalidDose(f32),
}

/// Per-layer summary of a `CureField` slab. KB-160 / ADR-0017.
///
/// `mean` is the cure-depth-at-mean-dose for the layer — replaces the
/// legacy `LayerResult.cure_depth_um` scalar. `min` is the
/// cure-depth-at-minimum-dose (most-undercured voxel) — replaces the
/// legacy `LayerResult.worst_cure_depth_um`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayerSummary {
    /// Mean cure depth (µm) across all voxels in the layer's Z-slab.
    pub mean: f32,
    /// Minimum cure depth (µm) across all voxels in the layer's Z-slab —
    /// the worst-case undercure for that layer.
    pub min: f32,
}

/// Dense 3D voxel field of cumulative absorbed dose (mJ/cm²).
///
/// Dimensions and voxel size are fixed at construction. Dose values are
/// mutated via [`Self::add_dose`] (cumulative addition) and read via
/// [`Self::dose_at`] (bounds-checked).
///
/// # Serialization
///
/// Serde derives via `ndarray`'s `serde` feature. The on-disk JSON shape
/// emits the dense f32 array as a nested list (`[[[...]]]`); for the
/// 50×50×100 mm-at-0.2 mm typical case (62 MB raw) this is large but
/// round-trippable. Future iterations may compress; v1 prioritises
/// correctness over wire size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CureField {
    nx: u32,
    ny: u32,
    nz: u32,
    voxel_size_mm: f32,
    bbox_min_mm: [f32; 3],
    /// Cumulative dose at each voxel, indexed `[ix, iy, iz]`. Always
    /// finite and non-negative; the public API enforces both.
    data: Array3<f32>,
}

impl CureField {
    /// Construct a new all-zero `CureField`.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` if any axis is 0.
    /// Returns `InvalidVoxelSize` if `voxel_size_mm` is not finite > 0.
    /// Returns `InvalidBboxMin` if any axis of `bbox_min_mm` is not finite.
    pub fn new(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
    ) -> Result<Self, CureFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(CureFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(CureFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite() || !bbox_min_mm[1].is_finite() || !bbox_min_mm[2].is_finite()
        {
            return Err(CureFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        let data = Array3::<f32>::zeros((nx as usize, ny as usize, nz as usize));
        Ok(Self {
            nx,
            ny,
            nz,
            voxel_size_mm,
            bbox_min_mm,
            data,
        })
    }

    /// Voxel-grid dimensions `(nx, ny, nz)`.
    pub fn dimensions(&self) -> (u32, u32, u32) {
        (self.nx, self.ny, self.nz)
    }

    /// Physical edge length of one voxel (mm).
    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }

    /// World-space `[x, y, z]` corner of voxel `[0, 0, 0]`.
    pub fn bbox_min_mm(&self) -> [f32; 3] {
        self.bbox_min_mm
    }

    /// Total number of voxels (`nx × ny × nz`).
    pub fn voxel_count(&self) -> u64 {
        u64::from(self.nx) * u64::from(self.ny) * u64::from(self.nz)
    }

    /// Total memory footprint of the dose array (`voxel_count × 4` bytes).
    /// Useful for pre-flight memory budget checks before allocation in
    /// tight environments. Does not include the struct overhead.
    pub fn total_bytes(&self) -> u64 {
        self.voxel_count() * (std::mem::size_of::<f32>() as u64)
    }

    /// World-space coordinate of the centre of voxel `[ix, iy, iz]`.
    /// Returns `Err(OutOfBounds)` if any index is past the field's
    /// dimensions.
    pub fn world_at_voxel_center(
        &self,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<[f32; 3], CureFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok([
            self.bbox_min_mm[0] + (ix as f32 + 0.5) * self.voxel_size_mm,
            self.bbox_min_mm[1] + (iy as f32 + 0.5) * self.voxel_size_mm,
            self.bbox_min_mm[2] + (iz as f32 + 0.5) * self.voxel_size_mm,
        ])
    }

    /// Cumulative dose at voxel `[ix, iy, iz]` (mJ/cm²).
    pub fn dose_at(&self, ix: u32, iy: u32, iz: u32) -> Result<f32, CureFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok(self.data[(ix as usize, iy as usize, iz as usize)])
    }

    /// Add `delta_dose` (mJ/cm²) to voxel `[ix, iy, iz]`'s cumulative
    /// total. `delta_dose` must be finite and `>= 0` — depletion lives on
    /// `PhotoinitiatorField`, not here, and we never want dose to decrease.
    pub fn add_dose(
        &mut self,
        ix: u32,
        iy: u32,
        iz: u32,
        delta_dose: f32,
    ) -> Result<(), CureFieldError> {
        self.check_bounds(ix, iy, iz)?;
        if !delta_dose.is_finite() || delta_dose < 0.0 {
            return Err(CureFieldError::InvalidDose(delta_dose));
        }
        self.data[(ix as usize, iy as usize, iz as usize)] += delta_dose;
        Ok(())
    }

    /// Total dose accumulated across all voxels (mJ/cm² · voxel_count).
    /// Useful for energy-budget sanity checks.
    pub fn total_dose(&self) -> f64 {
        self.data.iter().map(|&v| f64::from(v)).sum()
    }

    /// Maximum dose at any voxel (mJ/cm²).
    pub fn max_dose(&self) -> f32 {
        self.data.iter().copied().fold(0.0f32, f32::max)
    }

    /// Compute the per-layer summary for the Z-slab at layer index `iz`,
    /// mapped from cumulative dose to cure depth via the supplied
    /// `dp_um` (penetration depth, µm) and `ec_mj_cm2` (critical energy,
    /// mJ/cm²). Per-voxel cure depth = `Dp × ln(E / Ec)` clamped at 0 for
    /// undercured voxels (Beer-Lambert KB-103; consistent with the
    /// `CureCalculator::cure_depth` scalar primitive).
    ///
    /// Returns `Err(OutOfBounds)` if `iz >= nz`.
    pub fn layer_summary(
        &self,
        iz: u32,
        dp_um: f32,
        ec_mj_cm2: f32,
    ) -> Result<LayerSummary, CureFieldError> {
        if iz >= self.nz {
            return Err(CureFieldError::OutOfBounds {
                ix: 0,
                iy: 0,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        // Tier-1 inputs already validated by upstream (PenetrationDepth +
        // Energy value-object constructors). This summary is an internal
        // mapping; we trust the inputs are finite > 0 by the time they
        // arrive here.
        let mut sum: f64 = 0.0;
        let mut min_cd = f32::INFINITY;
        let mut count: u64 = 0;
        for ix in 0..self.nx {
            for iy in 0..self.ny {
                let dose = self.data[(ix as usize, iy as usize, iz as usize)];
                // Below or at Ec ⇒ undercured ⇒ cure depth = 0.
                let cd = if dose > ec_mj_cm2 {
                    dp_um * (dose / ec_mj_cm2).ln()
                } else {
                    0.0
                };
                sum += f64::from(cd);
                if cd < min_cd {
                    min_cd = cd;
                }
                count += 1;
            }
        }
        let mean = if count > 0 {
            (sum / count as f64) as f32
        } else {
            0.0
        };
        let min = if min_cd.is_finite() { min_cd } else { 0.0 };
        Ok(LayerSummary { mean, min })
    }

    /// Per-voxel cure depth at `[ix, iy, iz]` — Beer-Lambert mapping from
    /// the stored dose at this voxel. Same formula as the scalar primitive
    /// in `CureCalculator::cure_depth` (KB-103) but localised per voxel.
    /// Returns `Err(OutOfBounds)` if any index is past dimensions.
    pub fn cure_depth_at(
        &self,
        ix: u32,
        iy: u32,
        iz: u32,
        dp_um: f32,
        ec_mj_cm2: f32,
    ) -> Result<CureDepth, CureFieldError> {
        let dose = self.dose_at(ix, iy, iz)?;
        let cd = if dose > ec_mj_cm2 {
            dp_um * (dose / ec_mj_cm2).ln()
        } else {
            0.0
        };
        Ok(CureDepth::new(cd)
            .expect("KB-103 with validated dp/ec/dose and bounded by add_dose is finite"))
    }

    fn check_bounds(&self, ix: u32, iy: u32, iz: u32) -> Result<(), CureFieldError> {
        if ix >= self.nx || iy >= self.ny || iz >= self.nz {
            return Err(CureFieldError::OutOfBounds {
                ix,
                iy,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_2x2x2() -> CureField {
        CureField::new(2, 2, 2, 0.5, [0.0, 0.0, 0.0]).expect("2×2×2 test fixture is valid")
    }

    #[test]
    fn new_rejects_zero_x() {
        let err = CureField::new(0, 2, 2, 0.5, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidDimensions { nx: 0, .. });
    }

    #[test]
    fn new_rejects_zero_y() {
        let err = CureField::new(2, 0, 2, 0.5, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidDimensions { ny: 0, .. });
    }

    #[test]
    fn new_rejects_zero_z() {
        let err = CureField::new(2, 2, 0, 0.5, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidDimensions { nz: 0, .. });
    }

    #[test]
    fn new_rejects_nan_voxel_size() {
        let err = CureField::new(2, 2, 2, f32::NAN, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidVoxelSize(_));
    }

    #[test]
    fn new_rejects_zero_voxel_size() {
        let err = CureField::new(2, 2, 2, 0.0, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidVoxelSize(_));
    }

    #[test]
    fn new_rejects_negative_voxel_size() {
        let err = CureField::new(2, 2, 2, -0.5, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidVoxelSize(_));
    }

    #[test]
    fn new_rejects_infinite_voxel_size() {
        let err = CureField::new(2, 2, 2, f32::INFINITY, [0.0, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidVoxelSize(_));
    }

    #[test]
    fn new_rejects_nan_bbox_min() {
        let err = CureField::new(2, 2, 2, 0.5, [f32::NAN, 0.0, 0.0]).expect_err("test fixture: input deliberately violates CureField constructor precondition, so Err is the expected outcome");
        matches!(err, CureFieldError::InvalidBboxMin { .. });
    }

    #[test]
    fn new_initialises_all_voxels_to_zero() {
        let f = make_2x2x2();
        for ix in 0..2 {
            for iy in 0..2 {
                for iz in 0..2 {
                    assert_eq!(f.dose_at(ix, iy, iz).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), 0.0);
                }
            }
        }
        assert_eq!(f.total_dose(), 0.0);
        assert_eq!(f.max_dose(), 0.0);
    }

    #[test]
    fn dimensions_and_voxel_size_round_trip() {
        let f = CureField::new(3, 5, 7, 0.1, [1.0, 2.0, 3.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert_eq!(f.dimensions(), (3, 5, 7));
        assert!((f.voxel_size_mm() - 0.1).abs() < 1e-6);
        assert_eq!(f.bbox_min_mm(), [1.0, 2.0, 3.0]);
        assert_eq!(f.voxel_count(), 3 * 5 * 7);
        assert_eq!(f.total_bytes(), 3 * 5 * 7 * 4);
    }

    #[test]
    fn world_at_voxel_center_returns_voxel_centre() {
        let f = CureField::new(2, 2, 2, 0.5, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        // Voxel [0, 0, 0] center = (0.25, 0.25, 0.25)
        assert_eq!(f.world_at_voxel_center(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), [0.25, 0.25, 0.25]);
        // Voxel [1, 1, 1] center = (0.75, 0.75, 0.75)
        assert_eq!(f.world_at_voxel_center(1, 1, 1).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), [0.75, 0.75, 0.75]);
    }

    #[test]
    fn world_at_voxel_center_with_bbox_offset_applies_offset() {
        let f = CureField::new(2, 2, 2, 0.5, [10.0, 20.0, 30.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert_eq!(
            f.world_at_voxel_center(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"),
            [10.25, 20.25, 30.25]
        );
    }

    #[test]
    fn world_at_voxel_center_oob_returns_err() {
        let f = make_2x2x2();
        assert!(f.world_at_voxel_center(2, 0, 0).is_err());
    }

    #[test]
    fn add_dose_then_dose_at_round_trips() {
        let mut f = make_2x2x2();
        f.add_dose(0, 0, 0, 12.5).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert!((f.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)") - 12.5).abs() < 1e-6);
        // Other voxels remain zero.
        assert_eq!(f.dose_at(1, 1, 1).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), 0.0);
    }

    #[test]
    fn add_dose_accumulates() {
        let mut f = make_2x2x2();
        f.add_dose(0, 0, 0, 3.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(0, 0, 0, 2.5).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert!((f.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)") - 5.5).abs() < 1e-6);
    }

    #[test]
    fn add_dose_rejects_nan() {
        let mut f = make_2x2x2();
        assert!(matches!(
            f.add_dose(0, 0, 0, f32::NAN),
            Err(CureFieldError::InvalidDose(_))
        ));
        // Field stays untouched.
        assert_eq!(f.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), 0.0);
    }

    #[test]
    fn add_dose_rejects_negative() {
        let mut f = make_2x2x2();
        assert!(matches!(
            f.add_dose(0, 0, 0, -1.0),
            Err(CureFieldError::InvalidDose(_))
        ));
    }

    #[test]
    fn add_dose_rejects_infinity() {
        let mut f = make_2x2x2();
        assert!(matches!(
            f.add_dose(0, 0, 0, f32::INFINITY),
            Err(CureFieldError::InvalidDose(_))
        ));
    }

    #[test]
    fn add_dose_accepts_zero() {
        let mut f = make_2x2x2();
        assert!(f.add_dose(0, 0, 0, 0.0).is_ok());
        assert_eq!(f.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"), 0.0);
    }

    #[test]
    fn dose_at_oob_returns_err() {
        let f = make_2x2x2();
        assert!(f.dose_at(2, 0, 0).is_err());
        assert!(f.dose_at(0, 2, 0).is_err());
        assert!(f.dose_at(0, 0, 2).is_err());
    }

    #[test]
    fn total_dose_sums_all_voxels() {
        let mut f = make_2x2x2();
        f.add_dose(0, 0, 0, 1.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(1, 0, 0, 2.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(0, 1, 0, 3.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        // Float-mode sum stored as f64.
        assert!((f.total_dose() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn max_dose_returns_largest() {
        let mut f = make_2x2x2();
        f.add_dose(0, 0, 0, 1.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(1, 1, 1, 7.5).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(0, 1, 0, 3.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert!((f.max_dose() - 7.5).abs() < 1e-6);
    }

    #[test]
    fn layer_summary_undercured_returns_zero_for_all() {
        // All voxels at dose = 0 < Ec ⇒ Cd = 0 everywhere.
        let f = make_2x2x2();
        let s = f.layer_summary(0, 100.0, 5.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert_eq!(s.mean, 0.0);
        assert_eq!(s.min, 0.0);
    }

    #[test]
    fn layer_summary_uniform_dose_matches_kb103_scalar() {
        // Fill layer 0 with uniform dose 5×Ec; Cd = Dp × ln(5) ≈ 100 × 1.609 = 160.9.
        let mut f = CureField::new(2, 2, 1, 0.5, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        for ix in 0..2 {
            for iy in 0..2 {
                f.add_dose(ix, iy, 0, 25.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
                // dose = 25, Ec = 5 ⇒ ratio 5
            }
        }
        let s = f.layer_summary(0, 100.0, 5.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        let expected = 100.0 * 5.0f32.ln();
        assert!(
            (s.mean - expected).abs() < 0.01,
            "uniform-dose mean cd: expected {expected:.3}, got {:.3}",
            s.mean
        );
        // Mean equals min when the dose is uniform.
        assert!((s.min - expected).abs() < 0.01);
    }

    #[test]
    fn layer_summary_min_picks_worst_voxel() {
        // One voxel under-exposed, three above ⇒ min < mean.
        let mut f = CureField::new(2, 2, 1, 0.5, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(0, 0, 0, 25.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"); // ratio 5
        f.add_dose(1, 0, 0, 25.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(0, 1, 0, 25.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        f.add_dose(1, 1, 0, 7.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)"); // ratio 1.4 — much shallower
        let s = f.layer_summary(0, 100.0, 5.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        let cd_5 = 100.0 * 5.0f32.ln();
        let cd_1_4 = 100.0 * 1.4f32.ln();
        assert!((s.min - cd_1_4).abs() < 0.01);
        assert!(s.min < s.mean);
        // Mean = (3 × cd_5 + cd_1_4) / 4
        let expected_mean = (3.0 * cd_5 + cd_1_4) / 4.0;
        assert!((s.mean - expected_mean).abs() < 0.01);
    }

    #[test]
    fn layer_summary_oob_returns_err() {
        let f = make_2x2x2();
        assert!(f.layer_summary(2, 100.0, 5.0).is_err());
    }

    #[test]
    fn cure_depth_at_oob_returns_err() {
        let f = make_2x2x2();
        assert!(f.cure_depth_at(0, 0, 2, 100.0, 5.0).is_err());
    }

    #[test]
    fn cure_depth_at_matches_scalar_primitive() {
        // Single voxel exposed to dose 25 mJ/cm², Dp=100, Ec=5
        // ⇒ Cd = 100 × ln(5) ≈ 160.9 µm. Same as CureCalculator::cure_depth
        // (KB-103) when the column resolves to a single voxel.
        let mut f = make_2x2x2();
        f.add_dose(0, 0, 0, 25.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        let cd = f.cure_depth_at(0, 0, 0, 100.0, 5.0).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        let expected = 100.0 * 5.0f32.ln();
        assert!(
            (cd.value() - expected).abs() < 0.01,
            "delegation: expected {expected:.3}, got {:.3}",
            cd.value()
        );
    }

    #[test]
    fn memory_footprint_formula() {
        // 100×100×500 = 5M voxels × 4 bytes = 20 MB.
        let f = CureField::new(100, 100, 500, 0.2, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy CureField constructor preconditions (positive dims + finite voxel_size > 0 + finite bbox_min)");
        assert_eq!(f.voxel_count(), 5_000_000);
        assert_eq!(f.total_bytes(), 20_000_000);
    }
}
