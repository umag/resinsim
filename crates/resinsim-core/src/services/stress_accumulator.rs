//! Per-voxel linear-elastic stress from strain — `StressAccumulator` service.
//!
//! ADR-0018 / t2f3 / KB-162. Pure-function service: applies the
//! closed-form 6×6 isotropic Voigt stiffness `D` to a single
//! `StrainTensor` and returns the resulting `StressTensor`. The
//! orchestrator (`SimulationRunner::accumulate_layer_stress`) owns
//! per-layer iteration.
//!
//! All math lives in `StressTensor::from_strain_linear_elastic` —
//! this service is a thin named indirection so call sites read
//! `StressAccumulator::strain_to_stress(...)` consistent with the
//! orchestrator method's naming.

#![cfg(feature = "field-sim")]

use crate::values::{StrainTensor, StressTensor, StressTensorError};

/// Pure-function helpers wrapping the linear-elastic stress mapping.
pub struct StressAccumulator;

impl StressAccumulator {
    /// Apply isotropic linear-elastic stiffness to convert a strain
    /// tensor to a stress tensor (MPa). Wraps
    /// `StressTensor::from_strain_linear_elastic`.
    pub fn strain_to_stress(
        epsilon: &StrainTensor,
        youngs_modulus_mpa: f32,
        poissons_ratio: f32,
    ) -> Result<StressTensor, StressTensorError> {
        StressTensor::from_strain_linear_elastic(epsilon, youngs_modulus_mpa, poissons_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_strain_produces_zero_stress() {
        let s = StressAccumulator::strain_to_stress(&StrainTensor::zero(), 2000.0, 0.35).expect("test fixture: validated E (=2000 MPa) and nu (=0.35) produce finite stress");
        assert_eq!(s, StressTensor::zero());
    }

    #[test]
    fn isotropic_strain_produces_isotropic_stress() {
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: validated E (=2000 MPa) and nu (=0.35) produce finite stress");
        let s = StressAccumulator::strain_to_stress(&e, 2000.0, 0.35).expect("test fixture: validated E (=2000 MPa) and nu (=0.35) produce finite stress");
        // Hydrostatic component non-zero, shears zero.
        assert!(s.hydrostatic_mpa() < 0.0);
        assert_eq!(s.yz(), 0.0);
        assert_eq!(s.xz(), 0.0);
        assert_eq!(s.xy(), 0.0);
    }

    #[test]
    fn rejects_invalid_moduli() {
        let e = StrainTensor::zero();
        assert!(StressAccumulator::strain_to_stress(&e, 0.0, 0.35).is_err());
        assert!(StressAccumulator::strain_to_stress(&e, 2000.0, 0.5).is_err());
    }
}
