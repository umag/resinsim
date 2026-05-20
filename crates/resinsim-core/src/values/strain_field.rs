//! 3D voxel-resolved shrinkage-strain field — `StrainField` value object.
//!
//! ADR-0018, t2f3. Dense `ndarray::Array3<StrainTensor>` indexed by
//! `[ix, iy, iz]`. Same coordinate convention as `CureField`:
//! bbox-anchored, `voxel_size_mm` is the LCD pixel pitch on X-Y, Z is
//! the layer-count axis (per
//! `docs/patterns/voxel-field-z-dimension-is-layer-count.md`).
//!
//! # Cured-layer-locks-strain
//!
//! Each voxel is written exactly once — during its cure-layer pass.
//! `lock_strain_at` enforces the invariant at the type level: a
//! `StrainTensor` already-set voxel cannot be overwritten without
//! explicit `unlock_strain_at`. The orchestrator
//! (`SimulationRunner::apply_voxel_shrinkage_for_layer`) walks
//! `iz=current_layer` voxels, writes each once, and never returns to
//! an earlier slab. Late-layer light penetration into already-cured
//! voxels (which the CureField DOES record cumulatively) is correctly
//! ignored for strain because cured polymer no longer deforms freely
//! (KB-161 §"cured-layer-locks-strain").
//!
//! # NaN policy
//!
//! Two-layer defence per `docs/patterns/nan-two-layer-defence.md`:
//! constructor + every mutating accessor checks `is_finite` on the
//! StrainTensor before touching state. NaN cannot enter via the public
//! API (StrainTensor's own constructor rejects NaN, and
//! `lock_strain_at` takes a pre-validated StrainTensor by value).
//!
//! # Memory
//!
//! Dense f32 × 6 = 24 bytes per voxel. The MAX_FIELD_ALLOCATION_BYTES
//! budget guard runs at construction.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::field_budget::{enforce_field_budget, FieldAllocationError};
use crate::values::StrainTensor;

/// Errors from `StrainField` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum StrainFieldError {
    #[error("StrainField dimensions must all be positive, got {nx}×{ny}×{nz}")]
    InvalidDimensions { nx: u32, ny: u32, nz: u32 },
    #[error("StrainField voxel_size_mm must be finite and > 0, got {0}")]
    InvalidVoxelSize(f32),
    #[error("StrainField bbox_min_mm must be finite on all axes, got ({x}, {y}, {z})")]
    InvalidBboxMin { x: f32, y: f32, z: f32 },
    #[error("StrainField index ({ix}, {iy}, {iz}) out of bounds for {nx}×{ny}×{nz}")]
    OutOfBounds {
        ix: u32,
        iy: u32,
        iz: u32,
        nx: u32,
        ny: u32,
        nz: u32,
    },
    /// `lock_strain_at` was called on a voxel whose strain was already
    /// set non-zero. Cured-layer-locks-strain invariant violation.
    #[error("StrainField voxel ({ix}, {iy}, {iz}) is already locked; cured-layer-locks-strain forbids overwrite")]
    AlreadyLocked { ix: u32, iy: u32, iz: u32 },
    /// ADR-0018 budget guard.
    #[error("StrainField allocation exceeds budget: {0}")]
    ExceedsBudget(FieldAllocationError),
}

/// Dense 3D voxel field of shrinkage strain tensors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrainField {
    nx: u32,
    ny: u32,
    nz: u32,
    voxel_size_mm: f32,
    bbox_min_mm: [f32; 3],
    data: Array3<StrainTensor>,
}

impl StrainField {
    /// Construct a new all-zero `StrainField`. Same coordinate-system
    /// validation as `CureField::new`. Applies the
    /// `MAX_FIELD_ALLOCATION_BYTES` budget guard before allocation.
    pub fn new(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
    ) -> Result<Self, StrainFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(StrainFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(StrainFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite() || !bbox_min_mm[1].is_finite() || !bbox_min_mm[2].is_finite()
        {
            return Err(StrainFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        enforce_field_budget(
            "StrainField",
            nx,
            ny,
            nz,
            std::mem::size_of::<StrainTensor>() as u64,
            voxel_size_mm,
        )
        .map_err(StrainFieldError::ExceedsBudget)?;
        let data = Array3::<StrainTensor>::from_elem(
            (nx as usize, ny as usize, nz as usize),
            StrainTensor::zero(),
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
    pub(crate) fn data(&self) -> &Array3<StrainTensor> {
        &self.data
    }

    /// Reconstitute a `StrainField` from raw persistence inputs
    /// (ADR-0019 sidecar decoder). Validates dim/voxel_size/bbox/budget
    /// then takes the caller-supplied data array. The sidecar decoder
    /// guarantees per-tensor finiteness before invoking — see
    /// `docs/patterns/anti/rust-nan-positive-validation-gap.md`.
    #[doc(hidden)]
    pub fn from_persistence_parts(
        nx: u32,
        ny: u32,
        nz: u32,
        voxel_size_mm: f32,
        bbox_min_mm: [f32; 3],
        data: Array3<StrainTensor>,
    ) -> Result<Self, StrainFieldError> {
        if nx == 0 || ny == 0 || nz == 0 {
            return Err(StrainFieldError::InvalidDimensions { nx, ny, nz });
        }
        if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
            return Err(StrainFieldError::InvalidVoxelSize(voxel_size_mm));
        }
        if !bbox_min_mm[0].is_finite() || !bbox_min_mm[1].is_finite() || !bbox_min_mm[2].is_finite()
        {
            return Err(StrainFieldError::InvalidBboxMin {
                x: bbox_min_mm[0],
                y: bbox_min_mm[1],
                z: bbox_min_mm[2],
            });
        }
        enforce_field_budget(
            "StrainField",
            nx,
            ny,
            nz,
            std::mem::size_of::<StrainTensor>() as u64,
            voxel_size_mm,
        )
        .map_err(StrainFieldError::ExceedsBudget)?;
        if data.shape() != [nx as usize, ny as usize, nz as usize] {
            return Err(StrainFieldError::InvalidDimensions { nx, ny, nz });
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

    /// Total memory footprint (`voxel_count × size_of::<StrainTensor>()` bytes).
    pub fn total_bytes(&self) -> u64 {
        self.voxel_count() * (std::mem::size_of::<StrainTensor>() as u64)
    }

    /// World-space coordinate of voxel `[ix, iy, iz]` centre. Uses
    /// `voxel_size_mm` for ALL three axes — same caveat as
    /// `CureField::world_at_voxel_center` (Z is physically the layer
    /// height, not voxel_size_mm).
    pub fn world_at_voxel_center(
        &self,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<[f32; 3], StrainFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok([
            self.bbox_min_mm[0] + (ix as f32 + 0.5) * self.voxel_size_mm,
            self.bbox_min_mm[1] + (iy as f32 + 0.5) * self.voxel_size_mm,
            self.bbox_min_mm[2] + (iz as f32 + 0.5) * self.voxel_size_mm,
        ])
    }

    /// Strain tensor at voxel `[ix, iy, iz]`. Returns `StrainTensor::zero()`
    /// for unlocked voxels (constructor initial state).
    pub fn strain_at(&self, ix: u32, iy: u32, iz: u32) -> Result<StrainTensor, StrainFieldError> {
        self.check_bounds(ix, iy, iz)?;
        Ok(self.data[(ix as usize, iy as usize, iz as usize)])
    }

    /// Lock the strain at voxel `[ix, iy, iz]` to `tensor`. Cured-layer-
    /// locks-strain invariant: errors if the voxel was previously set to
    /// a non-zero tensor. Zero tensors are still "unlocked" for the
    /// purpose of this check — they're the constructor's initial state
    /// and a re-write to a non-zero tensor is the intended cure-layer
    /// pass for that voxel.
    pub fn lock_strain_at(
        &mut self,
        ix: u32,
        iy: u32,
        iz: u32,
        tensor: StrainTensor,
    ) -> Result<(), StrainFieldError> {
        self.check_bounds(ix, iy, iz)?;
        let existing = self.data[(ix as usize, iy as usize, iz as usize)];
        if existing != StrainTensor::zero() {
            return Err(StrainFieldError::AlreadyLocked { ix, iy, iz });
        }
        self.data[(ix as usize, iy as usize, iz as usize)] = tensor;
        Ok(())
    }

    /// Maximum Frobenius-norm strain magnitude across the Z-slab at
    /// layer index `iz`. Used by `SimulationRunner` to cache the
    /// per-layer aggregate on `LayerResult.strain_magnitude_max`.
    pub fn magnitude_layer_max(&self, iz: u32) -> Result<f32, StrainFieldError> {
        if iz >= self.nz {
            return Err(StrainFieldError::OutOfBounds {
                ix: 0,
                iy: 0,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        let mut max_mag: f32 = 0.0;
        for ix in 0..self.nx {
            for iy in 0..self.ny {
                let m = self.data[(ix as usize, iy as usize, iz as usize)].magnitude();
                if m > max_mag {
                    max_mag = m;
                }
            }
        }
        Ok(max_mag)
    }

    /// Maximum |∇ε| over adjacent X/Y/Z voxel pairs in the Z-slab at
    /// `iz`, measured as the Frobenius difference between adjacent
    /// strain tensors. Pairs where EITHER voxel has zero strain are
    /// SKIPPED — these are part-surface (cured-vs-empty) transitions
    /// that trivially produce a Frobenius diff of `L·√3` regardless of
    /// the resin or geometry, and would dominate the layer-max with
    /// false-positive noise that the lilith torso run surfaced. The
    /// post-t2f3 calibration finding is documented in KB-161. Pure-
    /// interior gradients (both neighbours cured) ARE measured — those
    /// reflect real strain discontinuities (e.g. thick-thin step
    /// transitions) and are the intended CohesiveFailure signal.
    ///
    /// Boundary voxels (no neighbour in some direction) are also
    /// SKIPPED. Returns 0 if the layer has < 2 voxels in any direction
    /// or if no two cured voxels are adjacent in the slab.
    pub fn gradient_layer_max(&self, iz: u32) -> Result<f32, StrainFieldError> {
        if iz >= self.nz {
            return Err(StrainFieldError::OutOfBounds {
                ix: 0,
                iy: 0,
                iz,
                nx: self.nx,
                ny: self.ny,
                nz: self.nz,
            });
        }
        let mut max_grad: f32 = 0.0;
        let zero = StrainTensor::zero();

        // Frobenius difference helper. The Frobenius norm of the
        // (e1 - e2) tensor uses the same √(Σ + 2·Σ_off) convention as
        // StrainTensor::magnitude — so the symmetric off-diagonal
        // contributions are weighted by 2 to match the full 3×3
        // matrix-difference Frobenius norm.
        let frob_diff = |a: StrainTensor, b: StrainTensor| -> f32 {
            let [axx, ayy, azz, ayz, axz, axy] = a.components();
            let [bxx, byy, bzz, byz, bxz, bxy] = b.components();
            let dxx = axx - bxx;
            let dyy = ayy - byy;
            let dzz = azz - bzz;
            let dyz = ayz - byz;
            let dxz = axz - bxz;
            let dxy = axy - bxy;
            (dxx * dxx + dyy * dyy + dzz * dzz + 2.0 * (dyz * dyz + dxz * dxz + dxy * dxy)).sqrt()
        };

        // Predicate: both neighbours must be cured (non-zero strain)
        // for the pair to contribute. cured-vs-empty pairs are the
        // part-surface noise we're filtering out.
        let both_cured = |a: StrainTensor, b: StrainTensor| -> bool { a != zero && b != zero };

        // X-direction gradients within the slab
        if self.nx >= 2 {
            for ix in 0..(self.nx - 1) {
                for iy in 0..self.ny {
                    let a = self.data[(ix as usize, iy as usize, iz as usize)];
                    let b = self.data[((ix + 1) as usize, iy as usize, iz as usize)];
                    if !both_cured(a, b) {
                        continue;
                    }
                    let g = frob_diff(a, b);
                    if g > max_grad {
                        max_grad = g;
                    }
                }
            }
        }
        // Y-direction gradients within the slab
        if self.ny >= 2 {
            for ix in 0..self.nx {
                for iy in 0..(self.ny - 1) {
                    let a = self.data[(ix as usize, iy as usize, iz as usize)];
                    let b = self.data[(ix as usize, (iy + 1) as usize, iz as usize)];
                    if !both_cured(a, b) {
                        continue;
                    }
                    let g = frob_diff(a, b);
                    if g > max_grad {
                        max_grad = g;
                    }
                }
            }
        }
        // Z-direction gradient: between this slab and the one below
        if iz > 0 {
            for ix in 0..self.nx {
                for iy in 0..self.ny {
                    let a = self.data[(ix as usize, iy as usize, iz as usize)];
                    let b = self.data[(ix as usize, iy as usize, (iz - 1) as usize)];
                    if !both_cured(a, b) {
                        continue;
                    }
                    let g = frob_diff(a, b);
                    if g > max_grad {
                        max_grad = g;
                    }
                }
            }
        }
        Ok(max_grad)
    }

    fn check_bounds(&self, ix: u32, iy: u32, iz: u32) -> Result<(), StrainFieldError> {
        if ix >= self.nx || iy >= self.ny || iz >= self.nz {
            return Err(StrainFieldError::OutOfBounds {
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
        let f = StrainField::new(2, 3, 4, 0.5, [0.0, 0.0, 0.0]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert_eq!(f.dimensions(), (2, 3, 4));
        assert_eq!(f.voxel_count(), 24);
        // Every voxel starts at zero.
        for ix in 0..2 {
            for iy in 0..3 {
                for iz in 0..4 {
                    let t = f.strain_at(ix, iy, iz).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions");
                    assert_eq!(t, StrainTensor::zero());
                }
            }
        }
    }

    #[test]
    fn new_rejects_zero_dims() {
        assert!(matches!(
            StrainField::new(0, 1, 1, 0.5, [0.0; 3]),
            Err(StrainFieldError::InvalidDimensions { .. })
        ));
        assert!(matches!(
            StrainField::new(1, 0, 1, 0.5, [0.0; 3]),
            Err(StrainFieldError::InvalidDimensions { .. })
        ));
        assert!(matches!(
            StrainField::new(1, 1, 0, 0.5, [0.0; 3]),
            Err(StrainFieldError::InvalidDimensions { .. })
        ));
    }

    #[test]
    fn new_rejects_nan_voxel_size() {
        assert!(StrainField::new(1, 1, 1, f32::NAN, [0.0; 3]).is_err());
    }

    #[test]
    fn new_rejects_zero_voxel_size() {
        assert!(StrainField::new(1, 1, 1, 0.0, [0.0; 3]).is_err());
    }

    #[test]
    fn new_rejects_nan_bbox() {
        assert!(StrainField::new(1, 1, 1, 0.5, [f32::NAN, 0.0, 0.0]).is_err());
        assert!(StrainField::new(1, 1, 1, 0.5, [0.0, f32::NAN, 0.0]).is_err());
        assert!(StrainField::new(1, 1, 1, 0.5, [0.0, 0.0, f32::NAN]).is_err());
    }

    #[test]
    fn strain_at_oob_returns_err() {
        let f = StrainField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert!(matches!(
            f.strain_at(2, 0, 0),
            Err(StrainFieldError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn lock_strain_at_writes_zero_voxel() {
        let mut f = StrainField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let t = StrainTensor::from_isotropic(-0.01).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(0, 0, 0, t).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert_eq!(f.strain_at(0, 0, 0).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions"), t);
    }

    #[test]
    fn lock_strain_at_rejects_overwrite() {
        let mut f = StrainField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let t = StrainTensor::from_isotropic(-0.01).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(0, 0, 0, t).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let result = f.lock_strain_at(0, 0, 0, t);
        assert!(matches!(
            result,
            Err(StrainFieldError::AlreadyLocked { .. })
        ));
    }

    #[test]
    fn lock_strain_at_oob_returns_err() {
        let mut f = StrainField::new(2, 2, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let t = StrainTensor::zero();
        assert!(matches!(
            f.lock_strain_at(2, 0, 0, t),
            Err(StrainFieldError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn magnitude_layer_max_zero_for_unlocked_layer() {
        let f = StrainField::new(3, 3, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert_eq!(f.magnitude_layer_max(0).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions"), 0.0);
        assert_eq!(f.magnitude_layer_max(1).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions"), 0.0);
    }

    #[test]
    fn magnitude_layer_max_picks_largest_in_slab() {
        let mut f = StrainField::new(3, 3, 2, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let small = StrainTensor::from_isotropic(-0.005).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let large = StrainTensor::from_isotropic(-0.02).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(0, 0, 1, small).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(1, 1, 1, large).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let m = f.magnitude_layer_max(1).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        // Frobenius of isotropic ε = -0.02 is |ε|·√3 ≈ 0.0346.
        let expected = 0.02 * 3.0_f32.sqrt();
        assert!((m - expected).abs() < 1e-5);
    }

    #[test]
    fn magnitude_layer_max_oob_returns_err() {
        let f = StrainField::new(1, 1, 1, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert!(matches!(
            f.magnitude_layer_max(1),
            Err(StrainFieldError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn gradient_layer_max_zero_for_uniform_layer() {
        let mut f = StrainField::new(2, 2, 1, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let t = StrainTensor::from_isotropic(-0.01).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        for ix in 0..2 {
            for iy in 0..2 {
                f.lock_strain_at(ix, iy, 0, t).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions");
            }
        }
        // All voxels identical → gradient = 0.
        assert_eq!(f.gradient_layer_max(0).expect("test fixture: in-bounds index and finite tensor satisfy field accessor preconditions"), 0.0);
    }

    #[test]
    fn gradient_layer_max_skips_cured_vs_empty_pair() {
        // Post-t2f3 calibration finding (KB-161): cured-vs-empty pairs
        // are the part-surface false-positive that pollutes the layer
        // max. The new contract is: both neighbours must be non-zero
        // (cured) for the pair to contribute to the max.
        let mut f = StrainField::new(2, 1, 1, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let nonzero = StrainTensor::from_isotropic(-0.02).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        // Only voxel (1, 0, 0) is locked; (0, 0, 0) stays zero.
        // This is a part-surface transition — must NOT trigger the
        // gradient max.
        f.lock_strain_at(1, 0, 0, nonzero).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let g = f.gradient_layer_max(0).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert_eq!(
            g, 0.0,
            "cured-vs-empty pair must NOT contribute to gradient_layer_max"
        );
    }

    #[test]
    fn gradient_layer_max_detects_interior_step() {
        // Two cured voxels with different magnitudes are the intended
        // CohesiveFailure signal — measure that gradient.
        let mut f = StrainField::new(2, 1, 1, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let small = StrainTensor::from_isotropic(-0.005).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let large = StrainTensor::from_isotropic(-0.02).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(0, 0, 0, small).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        f.lock_strain_at(1, 0, 0, large).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let g = f.gradient_layer_max(0).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        // Frobenius diff of two isotropic tensors with magnitudes
        // 0.005 and 0.02: components differ by 0.015 on the diagonal,
        // 0 on shears → √(3·0.015²) = 0.015·√3.
        let expected = 0.015_f32 * 3.0_f32.sqrt();
        assert!((g - expected).abs() < 1e-5, "expected {expected}, got {g}");
    }

    #[test]
    fn total_bytes_matches_voxel_count_times_tensor_size() {
        let f = StrainField::new(2, 3, 4, 0.5, [0.0; 3]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let expected = 24_u64 * (std::mem::size_of::<StrainTensor>() as u64);
        assert_eq!(f.total_bytes(), expected);
    }

    #[test]
    fn world_at_voxel_center_with_bbox_offset() {
        let f = StrainField::new(2, 2, 2, 0.5, [10.0, 20.0, 30.0]).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        let p = f.world_at_voxel_center(0, 0, 0).expect(
            "test fixture: in-bounds index and finite tensor satisfy field accessor preconditions",
        );
        assert!((p[0] - 10.25).abs() < 1e-6);
        assert!((p[1] - 20.25).abs() < 1e-6);
        assert!((p[2] - 30.25).abs() < 1e-6);
    }

    #[test]
    fn budget_exceeded_constructor_returns_error() {
        use crate::values::field_budget::FIELD_BUDGET_ENV_VAR;
        // Cap budget at 1 MB and request a field that needs ~24 MB
        // (1000 × 1000 voxels × 24 bytes/StrainTensor at iz=1 ≈ 24 MB).
        unsafe { std::env::set_var(FIELD_BUDGET_ENV_VAR, "1000000") };
        let r = StrainField::new(1000, 1000, 1, 0.1, [0.0; 3]);
        unsafe { std::env::remove_var(FIELD_BUDGET_ENV_VAR) };
        assert!(matches!(r, Err(StrainFieldError::ExceedsBudget(_))));
    }
}
