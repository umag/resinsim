use crate::values::{CureDepth, Energy, PenetrationDepth};

/// Gas constant. Unit: J/(mol·K).
const R: f32 = 8.314;

/// Domain service: Beer-Lambert cure depth calculations.
/// Stateless — all inputs via parameters.
///
/// Core equation (KB-103): Cd = Dp × ln(E / Ec)
///
/// Temperature-dependent variant (KB-153): the critical energy Ec itself is
/// temperature-dependent via Arrhenius kinetics — higher vat temperature
/// lowers Ec (faster radical polymerization ⇒ less energy needed to cross the
/// gel threshold). See [`CureCalculator::cure_depth_at_temp`].
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
        CureDepth::new(dp.value() * (energy.value() / critical_energy.value()).ln()).expect(
            "Beer-Lambert with validated dp, energy, critical_energy always yields finite result",
        )
    }

    /// Compute cure depth with Arrhenius temperature correction on Ec.
    ///
    /// # Formula (KB-153)
    ///
    /// ```text
    /// Ec(T_K) = Ec_ref × exp((Ea_cure_J / R) × (1/T_K - 1/T_ref_K))
    /// Cd      = Dp × ln(E / Ec(T_K))
    /// ```
    ///
    /// where `Ea_cure_J = ea_cure_kj_mol × 1000.0`. Because `T_K > T_ref_K`
    /// implies `(1/T_K - 1/T_ref_K) < 0`, the exponent is negative and
    /// `Ec(T_K) < Ec_ref` — higher vat temperature lowers Ec, yielding a
    /// deeper cure. This matches radical-polymerization Arrhenius rate
    /// physics.
    ///
    /// # Uncertainty
    ///
    /// The `ea_cure_kj_mol` input is typically
    /// [`DEFAULT_CURE_KINETICS_EA_KJ_MOL`](crate::entities::DEFAULT_CURE_KINETICS_EA_KJ_MOL)
    /// (30 kJ/mol, KB-153 literature midpoint) unless the resin profile
    /// carries a measured value. Downstream cure-drift predictions using the
    /// default may be wrong by ±50%. Callers rendering user output SHOULD
    /// warn when the default is used.
    pub fn cure_depth_at_temp(
        dp: PenetrationDepth,
        energy: Energy,
        ec_ref: Energy,
        ref_temp_c: f32,
        vat_temp_c: f32,
        ea_cure_kj_mol: f32,
    ) -> CureDepth {
        let t_ref_k = ref_temp_c + 273.15;
        let t_k = vat_temp_c + 273.15;
        let ea_j = ea_cure_kj_mol * 1000.0;
        let exponent = (ea_j / R) * (1.0 / t_k - 1.0 / t_ref_k);
        let ec_adjusted = ec_ref.value() * exponent.exp();
        let ec = Energy::new(ec_adjusted).expect(
            "Ec(T) = Ec_ref × exp(bounded Arrhenius exponent) is positive and finite for validated inputs",
        );
        Self::cure_depth(dp, energy, ec)
    }

    /// Compute UV intensity at depth z into the resin.
    /// KB-103: I(z) = I₀ × exp(-z / Dp)
    pub fn intensity_at_depth(surface_intensity: f32, depth_um: f32, dp: PenetrationDepth) -> f32 {
        surface_intensity * (-depth_um / dp.value()).exp()
    }

    /// Check if cure depth is sufficient for a given layer height.
    pub fn is_sufficient(
        dp: PenetrationDepth,
        energy: Energy,
        critical_energy: Energy,
        layer_height_um: f32,
    ) -> bool {
        Self::cure_depth(dp, energy, critical_energy).is_sufficient(layer_height_um)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dp(um: f32) -> PenetrationDepth {
        PenetrationDepth::new(um)
            .expect("test fixture: positive finite µm is in PenetrationDepth domain")
    }

    fn energy(mj_cm2: f32) -> Energy {
        Energy::new(mj_cm2).expect("test fixture: positive finite mJ/cm² is in Energy domain")
    }

    // --- KB-103 test vectors ---

    #[test]
    fn cure_depth_100um_when_dp_100_and_e_equals_e_times_ec() {
        // KB-103: Dp=100, E=e×Ec → Cd = 100 × ln(e) = 100.0
        let cd =
            CureCalculator::cure_depth(dp(100.0), energy(std::f32::consts::E * 10.0), energy(10.0));
        assert!((cd.value() - 100.0).abs() < 0.1);
    }

    #[test]
    fn cure_depth_zero_when_e_equals_ec() {
        // KB-103: E = Ec → ln(1) = 0
        let cd = CureCalculator::cure_depth(dp(100.0), energy(10.0), energy(10.0));
        assert!((cd.value() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cure_depth_negative_when_undercured() {
        // KB-103: E < Ec → negative (undercured)
        let cd = CureCalculator::cure_depth(dp(100.0), energy(5.0), energy(10.0));
        assert!(cd.value() < 0.0);
        assert!((cd.value() - (-69.3)).abs() < 0.1);
    }

    #[test]
    fn cure_depth_liqcreate_premium_black() {
        // KB-100: Dp=170µm, Ec=5.0 mJ/cm² at 405nm
        // KB-103 vector: E=10.0 → Cd = 170 × ln(10/5) = 170 × 0.693 = 117.7
        let cd = CureCalculator::cure_depth(dp(170.0), energy(10.0), energy(5.0));
        assert!((cd.value() - 117.83).abs() < 0.1);
    }

    #[test]
    fn cure_depth_premium_white() {
        // KB-100: Dp=350µm, Ec=6.87 mJ/cm² at 405nm
        // KB-103 vector: E=20.0 → Cd = 350 × ln(20/6.87) = 350 × 1.068 = 373.7
        let cd = CureCalculator::cure_depth(dp(350.0), energy(20.0), energy(6.87));
        assert!((cd.value() - 373.7).abs() < 0.5);
    }

    #[test]
    fn cure_depth_pr48_academic() {
        // KB-101: PR48 at 365nm, Dp=42µm, Ec=18.3 mJ/cm²
        // KB-103 vector: E=50.0 → Cd = 42 × ln(50/18.3) = 42 × 1.005 = 42.2
        let cd = CureCalculator::cure_depth(dp(42.0), energy(50.0), energy(18.3));
        assert!((cd.value() - 42.2).abs() < 0.2);
    }

    #[test]
    fn cure_depth_veroclear_deep_penetration() {
        // KB-101: VeroClear at 405nm, Dp=568µm, Ec=6.9 mJ/cm²
        // KB-103 vector: E=50.0 → Cd = 568 × ln(50/6.9) = 568 × 1.981 = 1125.2
        let cd = CureCalculator::cure_depth(dp(568.0), energy(50.0), energy(6.9));
        assert!((cd.value() - 1125.0).abs() < 5.0);
    }

    // --- Guard contract ---

    #[test]
    #[should_panic(expected = "cure_depth: critical_energy")]
    fn cure_depth_panics_on_zero_critical_energy_bypass() {
        // unsafe: only way to construct Energy(0.0) for testing the runtime guard directly
        let zero_ec: Energy = unsafe { std::mem::transmute(0.0f32) };
        let _ = CureCalculator::cure_depth(dp(100.0), energy(10.0), zero_ec);
    }

    // --- KB-103 intensity test vectors ---

    #[test]
    fn intensity_at_surface_equals_i0() {
        let i = CureCalculator::intensity_at_depth(5.0, 0.0, dp(170.0));
        assert!((i - 5.0).abs() < 1e-6);
    }

    #[test]
    fn intensity_at_dp_equals_i0_over_e() {
        // At z = Dp, I = I₀/e = I₀ × 0.368
        let i = CureCalculator::intensity_at_depth(5.0, 170.0, dp(170.0));
        assert!((i - 1.839).abs() < 0.001);
    }

    #[test]
    fn intensity_at_2dp_equals_i0_over_e2() {
        let i = CureCalculator::intensity_at_depth(5.0, 340.0, dp(170.0));
        assert!((i - 0.677).abs() < 0.001);
    }

    #[test]
    fn intensity_at_50um() {
        // KB-103 vector: I₀=5.0, z=50, Dp=170 → 5.0 × exp(-50/170) = 3.722
        let i = CureCalculator::intensity_at_depth(5.0, 50.0, dp(170.0));
        assert!((i - 3.726).abs() < 0.001);
    }

    // --- Sufficiency checks ---

    #[test]
    fn sufficient_when_cd_exceeds_layer() {
        // KB-100: Premium Black, E=10 → Cd=117.7µm, layer=50µm → sufficient
        assert!(CureCalculator::is_sufficient(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            50.0,
        ));
    }

    // --- KB-153: temperature-dependent cure depth ---

    #[test]
    fn cure_depth_at_ref_temp_equals_cure_depth() {
        // When vat_temp_c = ref_temp_c, exponent = 0, exp(0) = 1 → Ec_adjusted = Ec_ref.
        // cure_depth_at_temp must equal cure_depth.
        let baseline = CureCalculator::cure_depth(dp(170.0), energy(10.0), energy(5.0));
        let at_ref = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            25.0,
            30.0,
        );
        assert!((at_ref.value() - baseline.value()).abs() < 1e-4);
    }

    #[test]
    fn cure_depth_at_temp_decreases_ec_when_warmer() {
        // T > T_ref ⇒ Ec(T) < Ec_ref ⇒ deeper cure.
        let cold = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            25.0,
            30.0,
        );
        let warm = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            40.0,
            30.0,
        );
        assert!(warm.value() > cold.value());
    }

    #[test]
    fn cure_depth_at_temp_increases_ec_when_colder() {
        // T < T_ref ⇒ Ec(T) > Ec_ref ⇒ shallower cure.
        let at_ref = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            25.0,
            30.0,
        );
        let chilly = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            15.0,
            30.0,
        );
        assert!(chilly.value() < at_ref.value());
    }

    #[test]
    fn cure_depth_at_temp_stronger_effect_with_higher_ea() {
        // Larger Ea magnifies Arrhenius sensitivity — Cd at fixed (T, E) should
        // move further from the ambient-temp baseline as Ea grows.
        let at_ref = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            40.0,
            30.0,
        );
        let big_ea = CureCalculator::cure_depth_at_temp(
            dp(170.0),
            energy(10.0),
            energy(5.0),
            25.0,
            40.0,
            60.0,
        );
        assert!(big_ea.value() > at_ref.value());
    }

    #[test]
    fn insufficient_when_cd_below_layer() {
        // KB-171: E=6.0, Dp=170, Ec=5.0 → Cd=31.0µm, layer=50µm → insufficient
        assert!(!CureCalculator::is_sufficient(
            dp(170.0),
            energy(6.0),
            energy(5.0),
            50.0,
        ));
    }
}
