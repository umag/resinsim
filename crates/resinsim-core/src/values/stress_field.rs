//! 3D voxel-resolved residual-stress field — `StressField` value object.
//!
//! ADR-0018, t2f3. Dense `ndarray::Array3<StressTensor>` indexed by
//! `[ix, iy, iz]`. Same coordinate convention + dimension-lock as
//! `StrainField`. The stress accumulator
//! (`SimulationRunner::accumulate_layer_stress`) writes each voxel
//! exactly once during its cure-layer pass via `accumulate_at`.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::field_budget::{enforce_field_budget, FieldAllocationError};
use crate::values::StressTensor;

/// Errors from `StressField` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum StressFieldError {
    #[error("StressField dimensions must all be positive, got {nx}×{ny}×{nz}")]
    InvalidDimensions { nx: u32, ny: u32, nz: u32 },
    #[error("StressField voxel_size_mm must be finite and > 0, got {0}")]
    InvalidVoxelSize(f32),
    #[error("StressField bbox_min_mm must be finite on all axes, got ({x}, {y}, {z})")]
    InvalidBboxMin { x: f32, y: f32, z: f32 },
    #[error("StressField index ({ix}, {iy}, {iz}) out of bounds for {nx}×{ny}×{nz}")]
    OutOfBounds {
        ix: u32,
        iy: u32,
        iz: u32,
        nx: u32,
        ny: u32,
        nz: u32,
    },
    /// Computing von Mises on the voxel's stress tensor produced a
    /// non-finite intermediate — bubbled from `StressTensor`.
    #[error("StressField von_mises_layer_max derivation failed at ({ix}, {iy}, {iz})")]
    VonMisesDerivationFailed { ix: u32, iy: u32, iz: u32 },
    /// ADR-0018 budget guard.
    #[error("StressField allocation exceeds budget: {0}")]
    ExceedsBudget(FieldAllocationError),
}

/// Dense 3D voxel field of residual-stress tensors (MPa).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressField {
    nx: u32,
    ny: u32,
    nz: u32,
    voxel_size_mm: f32,
    bbox_min_mm: [f32; 3],
    data: Array3<StressTensor>,
}

impl StressField {
    pub fn new(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
    ) -> Result<Self, StressFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(StressFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(StressFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite() || !bbox_min_mm[1].is_finite() || !bbox_min_mm[2].is_finite()
        {
            return Err(StressFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        enforce_field_budget(
            "StressField",
            nx,
            ny,
            nz,
            std::mem::size_of::<StressTensor>() as u64,
            voxel_size_mm,
        )
        .map_err(StressFieldError::ExceedsBudget)?;
        let data = Array3::<StressTensor>::from_elem(
            (nx as usize, ny as usize, nz as usize),
            StressTensor::zero(),
        );
        Ok(Self {
            nx,
            ny,
            nz,
            voxel_size_mm,
            bbox_min_mm,
            data,
        })
    }

    pub fn dimensions(&self) -> (u32, u32, u32) {
        (self.nx, self.ny, self.nz)
    }

    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }

    pub fn bbox_min_mm(&self) -> [f32; 3] {
        self.bbox_min_mm
    }

    /// Read access to the underlying dense voxel buffer. Used by the
    /// sidecar persistence layer (ADR-0019); not part of the public
    /// domain API.
    pub(crate) fn data(&self) -> &Array3<StressTensor> {
        &self.data
    }

    /// Reconstitute a `StressField` from raw persistence inputs
    /// (ADR-0019 sidecar decoder).
    pub(crate) fn from_persistence_parts(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
        data: Array3<StressTensor>,
    ) -> Result<Self, StressFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(StressFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(StressFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite() || !bbox_min_mm[1].is_finite() || !bbox_min_mm[2].is_finite()
        {
            return Err(StressFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        enforce_field_budget(
            "StressField",
            nx,
            ny,
            nz,
            std::mem::size_of::<StressTensor>() as u64,
            voxel_size_mm,
        )
        .map_err(StressFieldError::ExceedsBudget)?;
        if data.shape() != [nx as usize, ny as usize, nz as usize] {
            return Err(StressFieldError::InvalidDimensions { nx, ny, nz });
        }
        Ok(Self {
            nx,
            ny,
            nz,
            voxel_size_mm,
            bbox_min_mm,
            data,
        })
    }

    pub fn voxel_count(&self) -> u64 {
        u64::from(self.nx) * u64::from(self.ny) * u64::from(self.nz)
    }

    pub fn total_bytes(&self) -> u64 {
        self.voxel_count() * (std::mem::size_of::<StressTensor>() as u64)
    }

    /// Stress tensor at voxel `[ix, iy, iz]` (MPa). Returns zero tensor
    /// for unwritten voxels.
    pub fn stress_at(&self, ix: u32, iy: u32, iz: u32) -> Result<StressTensor, StressFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok(self.data[(ix as usize, iy as usize, iz as usize)])
    }

    /// Write `tensor` into voxel `[ix, iy, iz]`. The orchestrator
    /// (`SimulationRunner::accumulate_layer_stress`) calls this exactly
    /// once per voxel per cure pass; the field-level invariant is "one
    /// write per voxel" but unlike StrainField's lock_strain_at, we do
    /// NOT type-enforce single-write here — the per-voxel stress can
    /// in principle accumulate across multiple sources (e.g. cure-driven
    /// residual + future thermal contraction in t2f4). The accumulating
    /// API is a `set` for v1; an explicit cumulative-add operator can
    /// be added later if t2f4 needs it.
    pub fn accumulate_at(
        &mut self,
        ix: u32,
        iy: u32,
        iz: u32,
        tensor: StressTensor,
    ) -> Result<(), StressFieldError> {
        self.check_bounds(ix, iy, iz)?;
        self.data[(ix as usize, iy as usize, iz as usize)] = tensor;
        Ok(())
    }

    /// Maximum von Mises stress across the Z-slab at layer index `iz`
    /// (MPa). Returns 0 for an unwritten slab. Used by SimulationRunner
    /// to cache `LayerResult.stress_von_mises_max_mpa`.
    pub fn von_mises_layer_max(&self, iz: u32) -> Result<f32, StressFieldError> {
        if iz >= self.nz {
            return Err(StressFieldError::OutOfBounds {
                ix: 0,
                iy: 0,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        let mut max_vm: f32 = 0.0;
        for ix in 0..self.nx {
            for iy in 0..self.ny {
                let s = self.data[(ix as usize, iy as usize, iz as usize)];
                let vm = s
                    .von_mises_mpa()
                    .map_err(|_| StressFieldError::VonMisesDerivationFailed { ix, iy, iz })?;
                if vm > max_vm {
                    max_vm = vm;
                }
            }
        }
        Ok(max_vm)
    }

    /// Per-voxel yield fraction for the Z-slab at layer index `iz` —
    /// the share of cured voxels in the slab whose von Mises stress
    /// exceeds `tensile_strength_mpa`. Result in `[0, 1]`.
    ///
    /// Physical interpretation: tensile_strength_mpa is the
    /// uniaxial yield stress of the cured resin (KB-140); von Mises is
    /// the scalar yield criterion that generalises uniaxial yield to
    /// multi-axial stress states. A voxel with σ_vm > σ_tensile has
    /// crossed the yield surface and is no longer in elastic
    /// equilibrium — it has deformed plastically (or, for brittle
    /// photopolymer, cracked).
    ///
    /// "Cured" voxel = stress tensor != zero (the upstream `set_strain_
    /// stress_fields` path only writes non-zero stress for cured
    /// voxels because the strain field uses zero as the "uncured
    /// liquid" sentinel). Voxels with zero stress are uncured liquid
    /// or outside the part bbox and excluded from the denominator —
    /// a layer that is 99% empty with one yielded voxel should read
    /// 100% yielded, not 1%.
    ///
    /// Returns 0.0 for slabs with no cured voxels.
    ///
    /// **Model-gap caveat (ADR-0018 §9):** the per-voxel σ_vm value
    /// reflects free-shrinkage stress only — it does NOT include
    /// cumulative residual stress that builds up as later layers cure
    /// against already-cured layers below. Real MSLA prints warp
    /// because of the latter, not the former. With the v1 model,
    /// `yield_fraction` reads 0 on most prints even with the right
    /// physical threshold (tensile_strength_mpa). The fraction will
    /// become a useful early-warning signal once the Tier-3
    /// compatibility-violation residual stress model lands.
    pub fn yield_fraction(
        &self,
        iz: u32,
        tensile_strength_mpa: f32,
    ) -> Result<f32, StressFieldError> {
        if iz >= self.nz {
            return Err(StressFieldError::OutOfBounds {
                ix: 0,
                iy: 0,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        if !tensile_strength_mpa.is_finite() || tensile_strength_mpa <= 0.0 {
            // Defensive — ResinProfile.validate() rejects this; return
            // zero rather than panic if a hostile test fixture bypasses.
            return Ok(0.0);
        }
        let zero = StressTensor::zero();
        let mut cured_count: u64 = 0;
        let mut yielded_count: u64 = 0;
        for ix in 0..self.nx {
            for iy in 0..self.ny {
                let s = self.data[(ix as usize, iy as usize, iz as usize)];
                if s == zero {
                    continue;
                }
                cured_count += 1;
                let vm = s
                    .von_mises_mpa()
                    .map_err(|_| StressFieldError::VonMisesDerivationFailed { ix, iy, iz })?;
                if vm > tensile_strength_mpa {
                    yielded_count += 1;
                }
            }
        }
        if cured_count == 0 {
            return Ok(0.0);
        }
        Ok((yielded_count as f32) / (cured_count as f32))
    }

    fn check_bounds(&self, ix: u32, iy: u32, iz: u32) -> Result<(), StressFieldError> {
        if ix >= self.nx || iy >= self.ny || iz >= self.nz {
            return Err(StressFieldError::OutOfBounds {
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

    #[test]
    fn new_constructs_zero_filled_field() {
        let f = StressField::new(2, 3, 4, 0.5, [0.0, 0.0, 0.0]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert_eq!(f.dimensions(), (2, 3, 4));
        for ix in 0..2 {
            for iy in 0..3 {
                for iz in 0..4 {
                    assert_eq!(f.stress_at(ix, iy, iz).expect("test fixture: in-bounds index and finite stress tensor satisfy field preconditions"), StressTensor::zero());
                }
            }
        }
    }

    #[test]
    fn new_rejects_zero_dims() {
        assert!(StressField::new(0, 1, 1, 0.5, [0.0; 3]).is_err());
    }

    #[test]
    fn new_rejects_invalid_voxel_size() {
        assert!(StressField::new(1, 1, 1, 0.0, [0.0; 3]).is_err());
        assert!(StressField::new(1, 1, 1, f32::NAN, [0.0; 3]).is_err());
    }

    #[test]
    fn new_rejects_nan_bbox() {
        assert!(StressField::new(1, 1, 1, 0.5, [f32::NAN, 0.0, 0.0]).is_err());
    }

    #[test]
    fn accumulate_at_writes_tensor() {
        let mut f = StressField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        let s = StressTensor::new(10.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        f.accumulate_at(0, 0, 0, s).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert_eq!(f.stress_at(0, 0, 0).expect("test fixture: in-bounds index and finite stress tensor satisfy field preconditions"), s);
    }

    #[test]
    fn accumulate_at_oob_returns_err() {
        let mut f = StressField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert!(matches!(
            f.accumulate_at(2, 0, 0, StressTensor::zero()),
            Err(StressFieldError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn von_mises_layer_max_zero_for_unwritten_layer() {
        let f = StressField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert_eq!(f.von_mises_layer_max(0).expect("test fixture: in-bounds index and finite stress tensor satisfy field preconditions"), 0.0);
    }

    #[test]
    fn von_mises_layer_max_picks_largest_in_slab() {
        let mut f = StressField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        // Uniaxial 100 → vm = 100; uniaxial 50 → vm = 50.
        let s100 = StressTensor::new(100.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        let s50 = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        f.accumulate_at(0, 0, 1, s50).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        f.accumulate_at(1, 1, 1, s100).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        let m = f.von_mises_layer_max(1).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert!((m - 100.0).abs() < 1e-3);
    }

    #[test]
    fn von_mises_layer_max_oob_returns_err() {
        let f = StressField::new(1, 1, 1, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        assert!(matches!(
            f.von_mises_layer_max(1),
            Err(StressFieldError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn total_bytes_uses_stress_tensor_size() {
        let f = StressField::new(2, 3, 4, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite stress tensor satisfy field preconditions",
        );
        let expected = 24_u64 * (std::mem::size_of::<StressTensor>() as u64);
        assert_eq!(f.total_bytes(), expected);
    }

    #[test]
    fn budget_exceeded_returns_error() {
        use crate::values::field_budget::FIELD_BUDGET_ENV_VAR;
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "1000000") };
        let r = StressField::new(1000, 1000, 1, 0.1, [0.0; 3]);
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
        assert!(matches!(r, Err(StressFieldError::ExceedsBudget(_))));
    }

    // --- t2f3.1 B1: direct unit tests for yield_fraction ---
    //
    // yield_fraction was previously exercised only indirectly via
    // predict_strain_failures. These six tests lock its query surface
    // directly so future regressions surface without round-tripping
    // through the predictor.
    //
    // Math anchor: von Mises of a uniaxial stress tensor (σ_xx, 0, 0,
    // 0, 0, 0) equals |σ_xx|. Hydrostatic p = σ_xx/3; deviatoric
    // s_xx = 2σ/3, s_yy = s_zz = -σ/3; vm = √(3/2 · (4/9 + 1/9 + 1/9)
    // · σ²) = √(σ²) = |σ_xx|. Tests (c) and (d) rely on this.

    #[test]
    fn yield_fraction_zero_for_unwritten_slab() {
        // Fresh field, no accumulate_at calls — exercises the
        // cured_count == 0 early-return at the end of yield_fraction.
        let f = StressField::new(2, 2, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        let frac = f
            .yield_fraction(0, 50.0)
            .expect("test fixture: in-bounds layer with finite positive tensile");
        assert_eq!(frac, 0.0);
    }

    #[test]
    fn yield_fraction_zero_when_cured_below_threshold() {
        // 1×1×1 field with a single cured voxel whose von Mises is
        // sub-threshold. Distinct from the unwritten-slab path: here
        // cured_count == 1, yielded_count == 0 → 0.0.
        use crate::entities::ResinProfile;
        let resin = ResinProfile::elegoo_ceramic_grey_v2();
        let tensile = resin.tensile_strength_mpa;
        let mut f = StressField::new(1, 1, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        // σ_xx = 5 MPa → vm = 5 < tensile = 38 → does not yield.
        let sub = StressTensor::new(5.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            .expect("test fixture: literal stress components satisfy validation");
        f.accumulate_at(0, 0, 0, sub)
            .expect("test fixture: literal stress components satisfy validation");
        let frac = f
            .yield_fraction(0, tensile)
            .expect("test fixture: in-bounds layer with finite positive tensile");
        assert_eq!(frac, 0.0);
    }

    #[test]
    fn yield_fraction_one_when_all_cured_voxels_yield() {
        // 1×1×1 field; sole voxel uniaxially loaded above tensile.
        // vm(uniaxial σ_xx=50) = 50 > tensile=38 → yielded_count == 1,
        // cured_count == 1 → 1.0.
        use crate::entities::ResinProfile;
        let resin = ResinProfile::elegoo_ceramic_grey_v2();
        let tensile = resin.tensile_strength_mpa;
        let mut f = StressField::new(1, 1, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            .expect("test fixture: literal stress components satisfy validation");
        f.accumulate_at(0, 0, 0, yielded)
            .expect("test fixture: literal stress components satisfy validation");
        let frac = f
            .yield_fraction(0, tensile)
            .expect("test fixture: in-bounds layer with finite positive tensile");
        assert_eq!(frac, 1.0);
    }

    #[test]
    fn yield_fraction_denominator_excludes_uncured_voxels() {
        // 2×2×1 field; exactly ONE voxel cured + yielded; three left
        // at StressTensor::zero() (uncured). The s == zero filter in
        // yield_fraction must exclude the three uncureds from the
        // denominator: yielded_count = 1, cured_count = 1 → 1.0.
        // If the denominator wrongly counted all 4 voxels, fraction
        // would be 0.25 ≠ 1.0.
        use crate::entities::ResinProfile;
        let resin = ResinProfile::elegoo_ceramic_grey_v2();
        let tensile = resin.tensile_strength_mpa;
        let mut f = StressField::new(2, 2, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            .expect("test fixture: literal stress components satisfy validation");
        f.accumulate_at(0, 0, 0, yielded)
            .expect("test fixture: literal stress components satisfy validation");
        // Three other voxels deliberately left at StressTensor::zero().
        let frac = f
            .yield_fraction(0, tensile)
            .expect("test fixture: in-bounds layer with finite positive tensile");
        assert_eq!(
            frac, 1.0,
            "denominator must exclude uncured (zero-tensor) voxels"
        );
    }

    #[test]
    fn yield_fraction_defensive_on_invalid_tensile() {
        // The defensive guard at the top of yield_fraction returns 0.0
        // when tensile is not finite or not strictly positive. Even
        // with a real yielding stress accumulated, NaN / 0 / negative
        // tensile must produce Ok(0.0).
        let mut f = StressField::new(1, 1, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        let s = StressTensor::new(100.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            .expect("test fixture: literal stress components satisfy validation");
        f.accumulate_at(0, 0, 0, s)
            .expect("test fixture: literal stress components satisfy validation");
        assert_eq!(
            f.yield_fraction(0, f32::NAN)
                .expect("defensive guard returns Ok(0.0) for NaN tensile"),
            0.0
        );
        assert_eq!(
            f.yield_fraction(0, 0.0)
                .expect("defensive guard returns Ok(0.0) for zero tensile"),
            0.0
        );
        assert_eq!(
            f.yield_fraction(0, -1.0)
                .expect("defensive guard returns Ok(0.0) for negative tensile"),
            0.0
        );
    }

    #[test]
    fn yield_fraction_layer_oob_returns_err() {
        let f = StressField::new(2, 2, 1, 0.5, [0.0; 3])
            .expect("test fixture: literal stress components satisfy validation");
        let err = f.yield_fraction(99, 50.0);
        // Destructure on iz to lock the witness — bare matches! is a
        // silent-green hazard per
        // docs/patterns/anti/bare-matches-as-test-assertion.md.
        assert!(
            matches!(err, Err(StressFieldError::OutOfBounds { iz: 99, .. })),
            "expected OutOfBounds {{ iz: 99, .. }}, got {err:?}"
        );
    }
}
