//! 3D voxel-resolved photoinitiator concentration field — KB-160, ADR-0017.
//!
//! Companion to [`CureField`](crate::values::CureField): same dimensions
//! and bbox-anchored convention. Holds dimensionless `[0, 1]` concentration
//! at each voxel. Mutated via [`Self::deplete`] (monotonically
//! non-increasing) and read via [`Self::concentration_at`].
//!
//! # Standard radical kinetics (KB-160)
//!
//! For an exposure that delivers `delta_dose` (mJ/cm²) to a voxel during
//! one integration step, the concentration evolves via the analytical
//! solution of `dC/dt = -k_d × I × C`:
//!
//! ```text
//! C_after = C_before × exp(-k_d × delta_dose)
//! ```
//!
//! `k_d` is the resin-specific decay constant
//! (`ResinProfile::photoinitiator_decay_constant_k_d`). The exponential
//! form ensures `C` stays in `[0, C_initial]` for any finite non-negative
//! input.
//!
//! # NaN policy
//!
//! Two-layer defence: every constructor + mutating accessor checks
//! `is_finite` AND value-range before touching state. NaN concentrations
//! cannot enter the field via the public API.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::field_budget::{enforce_field_budget, FieldAllocationError};

/// Errors from `PhotoinitiatorField` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PhotoinitiatorFieldError {
    #[error("PhotoinitiatorField dimensions must all be positive, got {nx}×{ny}×{nz}")]
    InvalidDimensions { nx: u32, ny: u32, nz: u32 },
    #[error("PhotoinitiatorField initial_concentration must be finite and in [0, 1], got {0}")]
    InvalidInitialConcentration(f32),
    #[error("PhotoinitiatorField index ({ix}, {iy}, {iz}) out of bounds for {nx}×{ny}×{nz}")]
    OutOfBounds {
        ix: u32,
        iy: u32,
        iz: u32,
        nx: u32,
        ny: u32,
        nz: u32,
    },
    #[error(
        "PhotoinitiatorField deplete: k_d × delta_dose must be finite and >= 0 (got k_d={k_d}, delta_dose={delta_dose})"
    )]
    InvalidDepletionInput { k_d: f32, delta_dose: f32 },
    /// ADR-0018 budget guard — requested allocation exceeds the
    /// configured per-field budget. Behaviour change from pre-t2f3.
    #[error("PhotoinitiatorField allocation exceeds budget: {0}")]
    ExceedsBudget(FieldAllocationError),
}

/// Dense 3D voxel field of photoinitiator concentration (dimensionless
/// fraction in `[0, initial_concentration]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoinitiatorField {
    nx: u32,
    ny: u32,
    nz: u32,
    initial_concentration: f32,
    /// Concentration at each voxel, indexed `[ix, iy, iz]`. Always finite
    /// and bounded `[0, initial_concentration]`; the public API enforces
    /// both.
    data: Array3<f32>,
}

impl PhotoinitiatorField {
    /// Construct a new uniform field initialised to `initial_concentration`
    /// at every voxel.
    pub fn new(
        nx: u32,
        ny: u32,
        nz: u32,
        initial_concentration: f32,
    ) -> Result<Self, PhotoinitiatorFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(PhotoinitiatorFieldError::InvalidDimensions { nx, ny, nz });
        }
        // NaN-two-layer-defence: explicit is_finite first.
        if !initial_concentration.is_finite() || !(0.0..=1.0).contains(&initial_concentration) {
            return Err(PhotoinitiatorFieldError::InvalidInitialConcentration(
                initial_concentration,
            ));
        }
        // ADR-0018 budget guard. PhotoinitiatorField has no voxel_size_mm
        // of its own (it's dimension-locked to CureField in VoxelState);
        // pass a sentinel 1.0 mm for the suggested-voxel-size inversion —
        // callers that hit this branch should be reading the parallel
        // CureField error to find the real per-mm suggestion.
        enforce_field_budget(
            "PhotoinitiatorField",
            nx,
            ny,
            nz,
            std::mem::size_of::<f32>() as u64,
            1.0,
        )
        .map_err(PhotoinitiatorFieldError::ExceedsBudget)?;
        let data = Array3::<f32>::from_elem(
            (nx as usize, ny as usize, nz as usize),
            initial_concentration,
        );
        Ok(Self {
            nx,
            ny,
            nz,
            initial_concentration,
            data,
        })
    }

    pub fn dimensions(&self) -> (u32, u32, u32) {
        (self.nx, self.ny, self.nz)
    }

    /// Read access to the underlying dense voxel buffer. Used by the
    /// sidecar persistence layer (ADR-0019); not part of the public
    /// domain API.
    pub(crate) fn data(&self) -> &Array3<f32> {
        &self.data
    }

    /// Reconstitute a `PhotoinitiatorField` from raw persistence inputs
    /// (ADR-0019 sidecar decoder). `initial_concentration` is captured
    /// at construction and validated via `new`-equivalent rules; the
    /// caller passes the per-voxel data array directly (already-validated
    /// for finiteness by the sidecar decoder).
    pub(crate) fn from_persistence_parts(
        nx: u32,
        ny: u32,
        nz: u32,
        initial_concentration: f32,
        data: Array3<f32>,
    ) -> Result<Self, PhotoinitiatorFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(PhotoinitiatorFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !initial_concentration.is_finite() || !(0.0..=1.0).contains(&initial_concentration) {
            return Err(PhotoinitiatorFieldError::InvalidInitialConcentration(
                initial_concentration,
            ));
        }
        enforce_field_budget(
            "PhotoinitiatorField",
            nx,
            ny,
            nz,
            std::mem::size_of::<f32>() as u64,
            1.0,
        )
        .map_err(PhotoinitiatorFieldError::ExceedsBudget)?;
        if data.shape() != [nx as usize, ny as usize, nz as usize] {
            return Err(PhotoinitiatorFieldError::InvalidDimensions { nx, ny, nz });
        }
        Ok(Self {
            nx,
            ny,
            nz,
            initial_concentration,
            data,
        })
    }

    pub fn initial_concentration(&self) -> f32 {
        self.initial_concentration
    }

    pub fn voxel_count(&self) -> u64 {
        u64::from(self.nx) * u64::from(self.ny) * u64::from(self.nz)
    }

    pub fn total_bytes(&self) -> u64 {
        self.voxel_count() * (std::mem::size_of::<f32>() as u64)
    }

    /// Concentration at voxel `[ix, iy, iz]`.
    pub fn concentration_at(
        &self,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<f32, PhotoinitiatorFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok(self.data[(ix as usize, iy as usize, iz as usize)])
    }

    /// Concentration column at `(ix, iy)`: returns the Z-axis slice as a
    /// fresh `Vec<f32>` of length `nz`. ADR-0018 / t2f2.
    ///
    /// Read-only snapshot — the returned vector is decoupled from the field's
    /// internal buffer, so callers can pass it as a snapshot for
    /// `VoxelCureCalculator::compute_column_exposure` while the field is
    /// later mutated through `deplete`.
    pub fn column_at(&self, ix: u32, iy: u32) -> Result<Vec<f32>, PhotoinitiatorFieldError> {
        if ix >= self.nx || iy >= self.ny {
            return Err(PhotoinitiatorFieldError::OutOfBounds {
                ix,
                iy,
                iz: 0,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        let (ix_u, iy_u) = (ix as usize, iy as usize);
        Ok((0..self.nz as usize)
            .map(|iz| self.data[(ix_u, iy_u, iz)])
            .collect())
    }

    /// Deplete the voxel at `[ix, iy, iz]` per KB-160:
    /// `C_after = C_before × exp(-k_d × delta_dose)`.
    ///
    /// `k_d` must be finite and `>= 0`; `delta_dose` must be finite and
    /// `>= 0`. A `delta_dose == 0` or `k_d == 0` is a no-op (the
    /// exponential collapses to 1). The result is clamped to `[0, C_before]`
    /// against floating-point under/overshoot — concentration NEVER
    /// increases through this method (no recombination chemistry modelled).
    pub fn deplete(
        &mut self,
        ix: u32,
        iy: u32,
        iz: u32,
        k_d: f32,
        delta_dose: f32,
    ) -> Result<(), PhotoinitiatorFieldError> {
        self.check_bounds(ix, iy, iz)?;
        if !k_d.is_finite() || k_d < 0.0 || !delta_dose.is_finite() || delta_dose < 0.0 {
            return Err(PhotoinitiatorFieldError::InvalidDepletionInput { k_d, delta_dose });
        }
        let idx = (ix as usize, iy as usize, iz as usize);
        let c_before = self.data[idx];
        // exp(-k_d × delta_dose) is in [0, 1] for any non-negative
        // k_d × delta_dose. Bound the multiplier defensively to guard
        // floating-point overshoot above 1.0.
        let multiplier = (-k_d * delta_dose).exp();
        let multiplier = multiplier.clamp(0.0, 1.0);
        let c_after = (c_before * multiplier).clamp(0.0, c_before);
        self.data[idx] = c_after;
        Ok(())
    }

    /// Minimum concentration anywhere in the field. Useful for
    /// "is the resin fully cured?" checks.
    pub fn min_concentration(&self) -> f32 {
        self.data.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// Maximum concentration anywhere in the field. Should never exceed
    /// `initial_concentration`.
    pub fn max_concentration(&self) -> f32 {
        self.data.iter().copied().fold(0.0f32, f32::max)
    }

    fn check_bounds(&self, ix: u32, iy: u32, iz: u32) -> Result<(), PhotoinitiatorFieldError> {
        if ix >= self.nx || iy >= self.ny || iz >= self.nz {
            return Err(PhotoinitiatorFieldError::OutOfBounds {
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

    fn make_2x2x2() -> PhotoinitiatorField {
        PhotoinitiatorField::new(2, 2, 2, 1.0).expect("2×2×2 test fixture is valid")
    }

    #[test]
    fn new_rejects_zero_dims() {
        assert!(PhotoinitiatorField::new(0, 2, 2, 1.0).is_err());
        assert!(PhotoinitiatorField::new(2, 0, 2, 1.0).is_err());
        assert!(PhotoinitiatorField::new(2, 2, 0, 1.0).is_err());
    }

    #[test]
    fn new_rejects_nan_initial_concentration() {
        assert!(matches!(
            PhotoinitiatorField::new(2, 2, 2, f32::NAN),
            Err(PhotoinitiatorFieldError::InvalidInitialConcentration(_))
        ));
    }

    #[test]
    fn new_rejects_negative_initial_concentration() {
        assert!(PhotoinitiatorField::new(2, 2, 2, -0.1).is_err());
    }

    #[test]
    fn new_rejects_initial_concentration_above_one() {
        assert!(PhotoinitiatorField::new(2, 2, 2, 1.5).is_err());
    }

    #[test]
    fn new_accepts_zero_initial_concentration() {
        let f = PhotoinitiatorField::new(2, 2, 2, 0.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert_eq!(f.initial_concentration(), 0.0);
        assert_eq!(f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])"), 0.0);
    }

    #[test]
    fn new_initialises_uniform_concentration() {
        let f = PhotoinitiatorField::new(3, 3, 3, 0.85).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        for ix in 0..3 {
            for iy in 0..3 {
                for iz in 0..3 {
                    assert!((f.concentration_at(ix, iy, iz).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])") - 0.85).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn deplete_reduces_concentration() {
        let mut f = make_2x2x2();
        // KB-160 analytical: C_after = C_before × exp(-k_d × delta_dose)
        //                  = 1.0 × exp(-0.05 × 50) = exp(-2.5) ≈ 0.0821
        f.deplete(0, 0, 0, 0.05, 50.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        let expected = (-2.5f32).exp();
        let actual = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert!(
            (actual - expected).abs() < 1e-5,
            "kb-160 analytical: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn deplete_with_zero_dose_is_noop() {
        let mut f = make_2x2x2();
        f.deplete(0, 0, 0, 0.05, 0.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert_eq!(f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])"), 1.0);
    }

    #[test]
    fn deplete_with_zero_k_d_is_noop() {
        let mut f = make_2x2x2();
        f.deplete(0, 0, 0, 0.0, 100.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert_eq!(f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])"), 1.0);
    }

    #[test]
    fn deplete_monotonically_non_increasing() {
        let mut f = make_2x2x2();
        let before = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        f.deplete(0, 0, 0, 0.05, 10.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        let after_first = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        f.deplete(0, 0, 0, 0.05, 10.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        let after_second = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert!(after_first < before);
        assert!(after_second < after_first);
        assert!(after_second > 0.0);
    }

    #[test]
    fn deplete_never_goes_below_zero_for_huge_dose() {
        let mut f = make_2x2x2();
        // 1000 × 50 = 50000 in the exponent ⇒ exp(-50000) underflows to 0.
        // Result must clamp to >= 0, never NaN or negative.
        f.deplete(0, 0, 0, 1000.0, 50.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        let c = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert!(c >= 0.0);
        assert!(c.is_finite());
    }

    #[test]
    fn deplete_rejects_nan_k_d() {
        let mut f = make_2x2x2();
        assert!(f.deplete(0, 0, 0, f32::NAN, 10.0).is_err());
    }

    #[test]
    fn deplete_rejects_nan_delta_dose() {
        let mut f = make_2x2x2();
        assert!(f.deplete(0, 0, 0, 0.05, f32::NAN).is_err());
    }

    #[test]
    fn deplete_rejects_negative_k_d() {
        let mut f = make_2x2x2();
        assert!(f.deplete(0, 0, 0, -0.05, 10.0).is_err());
    }

    #[test]
    fn deplete_rejects_negative_delta_dose() {
        let mut f = make_2x2x2();
        assert!(f.deplete(0, 0, 0, 0.05, -10.0).is_err());
    }

    #[test]
    fn deplete_oob_returns_err() {
        let mut f = make_2x2x2();
        assert!(f.deplete(2, 0, 0, 0.05, 10.0).is_err());
    }

    #[test]
    fn min_concentration_returns_smallest() {
        let mut f = make_2x2x2();
        f.deplete(0, 0, 0, 0.05, 50.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])"); // ≈ 0.082
        f.deplete(1, 1, 1, 0.05, 10.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])"); // ≈ 0.606
        let min = f.min_concentration();
        assert!((min - (-2.5f32).exp()).abs() < 1e-5);
    }

    #[test]
    fn max_concentration_never_exceeds_initial() {
        let mut f = make_2x2x2();
        f.deplete(0, 0, 0, 0.05, 50.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        f.deplete(1, 1, 1, 0.05, 10.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert!(f.max_concentration() <= f.initial_concentration());
    }

    #[test]
    fn voxel_count_and_bytes() {
        let f = PhotoinitiatorField::new(10, 20, 30, 1.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
        assert_eq!(f.voxel_count(), 6000);
        assert_eq!(f.total_bytes(), 24_000);
    }

    /// Property-style spot check: repeated depletion of the same voxel
    /// monotonically decreases concentration toward zero, never below.
    #[test]
    fn repeated_depletion_drives_to_zero_floor() {
        let mut f = make_2x2x2();
        let mut prev = 1.0;
        for _ in 0..50 {
            f.deplete(0, 0, 0, 0.05, 1.0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
            let curr = f.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy PhotoinitiatorField constructor preconditions (positive dims + finite concentration in [0,1])");
            assert!(
                curr <= prev,
                "monotone non-increasing violated: prev={prev}, curr={curr}"
            );
            assert!(curr >= 0.0);
            prev = curr;
        }
        // After 50 × exp(-0.05) = exp(-2.5) ≈ 0.082, well below the
        // initial 1.0 but still positive.
        assert!(prev < 0.1);
        assert!(prev > 0.0);
    }
}
