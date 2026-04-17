use crate::values::{CureDepth, Energy, PenetrationDepth};

/// Domain service: Beer-Lambert cure depth calculations.
/// Stateless — all inputs via parameters.
///
/// Core equation (KB-103): Cd = Dp × ln(E / Ec)
pub struct CureCalculator;

impl CureCalculator {
    /// Compute cure depth using the Jacobs working curve.
    /// Returns negative value for undercured (E < Ec).
    ///
    /// KB-103: Cd = Dp × ln(E / Ec)
    ///
    /// Contract: `dp`, `energy`, and `critical_energy` must all be positive and finite.
    /// `Energy::new` and `Energy::scale` enforce this at construction; this guard is the
    /// last-line runtime check that also catches any bypass (e.g. transmute in tests).
    pub fn cure_depth(dp: PenetrationDepth, energy: Energy, critical_energy: Energy) -> CureDepth {
        assert!(
            critical_energy.value() > 0.0 && critical_energy.value().is_finite(),
            "cure_depth: critical_energy must be positive and finite, got {}",
            critical_energy.value()
        );
        assert!(
            energy.value() > 0.0 && energy.value().is_finite(),
            "cure_depth: energy must be positive and finite, got {}",
            energy.value()
        );
        CureDepth::new(dp.value() * (energy.value() / critical_energy.value()).ln())
            .expect("Beer-Lambert with validated dp, energy, critical_energy always yields finite result")
    }

    /// Compute UV intensity at depth z into the resin.
    /// KB-103: I(z) = I₀ × exp(-z / Dp)
    pub fn intensity_at_depth(surface_intensity: f32, depth_um: f32, dp: PenetrationDepth) -> f32 {
        surface_intensity * (-depth_um / dp.value()).exp()
    }

    /// Check if cure depth is sufficient for a given layer height.
    pub fn is_sufficient(dp: PenetrationDepth, energy: Energy, critical_energy: Energy, layer_height_um: f32) -> bool {
        Self::cure_depth(dp, energy, critical_energy).is_sufficient(layer_height_um)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- KB-103 test vectors ---

    #[test]
    fn cure_depth_100um_when_dp_100_and_e_equals_e_times_ec() {
        // KB-103: Dp=100, E=e×Ec → Cd = 100 × ln(e) = 100.0
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(100.0).unwrap(),
            Energy::new(std::f32::consts::E * 10.0).unwrap(),
            Energy::new(10.0).unwrap(),
        );
        assert!((cd.value() - 100.0).abs() < 0.1);
    }

    #[test]
    fn cure_depth_zero_when_e_equals_ec() {
        // KB-103: E = Ec → ln(1) = 0
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(100.0).unwrap(),
            Energy::new(10.0).unwrap(),
            Energy::new(10.0).unwrap(),
        );
        assert!((cd.value() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cure_depth_negative_when_undercured() {
        // KB-103: E < Ec → negative (undercured)
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(100.0).unwrap(),
            Energy::new(5.0).unwrap(),
            Energy::new(10.0).unwrap(),
        );
        assert!(cd.value() < 0.0);
        assert!((cd.value() - (-69.3)).abs() < 0.1);
    }

    #[test]
    fn cure_depth_liqcreate_premium_black() {
        // KB-100: Dp=170µm, Ec=5.0 mJ/cm² at 405nm
        // KB-103 vector: E=10.0 → Cd = 170 × ln(10/5) = 170 × 0.693 = 117.7
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(170.0).unwrap(),
            Energy::new(10.0).unwrap(),
            Energy::new(5.0).unwrap(),
        );
        assert!((cd.value() - 117.83).abs() < 0.1);
    }

    #[test]
    fn cure_depth_premium_white() {
        // KB-100: Dp=350µm, Ec=6.87 mJ/cm² at 405nm
        // KB-103 vector: E=20.0 → Cd = 350 × ln(20/6.87) = 350 × 1.068 = 373.7
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(350.0).unwrap(),
            Energy::new(20.0).unwrap(),
            Energy::new(6.87).unwrap(),
        );
        assert!((cd.value() - 373.7).abs() < 0.5);
    }

    #[test]
    fn cure_depth_pr48_academic() {
        // KB-101: PR48 at 365nm, Dp=42µm, Ec=18.3 mJ/cm²
        // KB-103 vector: E=50.0 → Cd = 42 × ln(50/18.3) = 42 × 1.005 = 42.2
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(42.0).unwrap(),
            Energy::new(50.0).unwrap(),
            Energy::new(18.3).unwrap(),
        );
        assert!((cd.value() - 42.2).abs() < 0.2);
    }

    #[test]
    fn cure_depth_veroclear_deep_penetration() {
        // KB-101: VeroClear at 405nm, Dp=568µm, Ec=6.9 mJ/cm²
        // KB-103 vector: E=50.0 → Cd = 568 × ln(50/6.9) = 568 × 1.981 = 1125.2
        let cd = CureCalculator::cure_depth(
            PenetrationDepth::new(568.0).unwrap(),
            Energy::new(50.0).unwrap(),
            Energy::new(6.9).unwrap(),
        );
        assert!((cd.value() - 1125.0).abs() < 5.0);
    }

    // --- Guard contract ---

    #[test]
    #[should_panic(expected = "cure_depth: critical_energy")]
    fn cure_depth_panics_on_zero_critical_energy_bypass() {
        // unsafe: only way to construct Energy(0.0) for testing the runtime guard directly
        let zero_ec: Energy = unsafe { std::mem::transmute(0.0f32) };
        let _ = CureCalculator::cure_depth(
            PenetrationDepth::new(100.0).unwrap(),
            Energy::new(10.0).unwrap(),
            zero_ec,
        );
    }

    // --- KB-103 intensity test vectors ---

    #[test]
    fn intensity_at_surface_equals_i0() {
        let i = CureCalculator::intensity_at_depth(5.0, 0.0, PenetrationDepth::new(170.0).unwrap());
        assert!((i - 5.0).abs() < 1e-6);
    }

    #[test]
    fn intensity_at_dp_equals_i0_over_e() {
        // At z = Dp, I = I₀/e = I₀ × 0.368
        let i = CureCalculator::intensity_at_depth(5.0, 170.0, PenetrationDepth::new(170.0).unwrap());
        assert!((i - 1.839).abs() < 0.001);
    }

    #[test]
    fn intensity_at_2dp_equals_i0_over_e2() {
        let i = CureCalculator::intensity_at_depth(5.0, 340.0, PenetrationDepth::new(170.0).unwrap());
        assert!((i - 0.677).abs() < 0.001);
    }

    #[test]
    fn intensity_at_50um() {
        // KB-103 vector: I₀=5.0, z=50, Dp=170 → 5.0 × exp(-50/170) = 3.722
        let i = CureCalculator::intensity_at_depth(5.0, 50.0, PenetrationDepth::new(170.0).unwrap());
        assert!((i - 3.726).abs() < 0.001);
    }

    // --- Sufficiency checks ---

    #[test]
    fn sufficient_when_cd_exceeds_layer() {
        // KB-100: Premium Black, E=10 → Cd=117.7µm, layer=50µm → sufficient
        assert!(CureCalculator::is_sufficient(
            PenetrationDepth::new(170.0).unwrap(),
            Energy::new(10.0).unwrap(),
            Energy::new(5.0).unwrap(),
            50.0,
        ));
    }

    #[test]
    fn insufficient_when_cd_below_layer() {
        // KB-171: E=6.0, Dp=170, Ec=5.0 → Cd=31.0µm, layer=50µm → insufficient
        assert!(!CureCalculator::is_sufficient(
            PenetrationDepth::new(170.0).unwrap(),
            Energy::new(6.0).unwrap(),
            Energy::new(5.0).unwrap(),
            50.0,
        ));
    }
}
