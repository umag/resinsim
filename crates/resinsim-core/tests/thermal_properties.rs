use proptest::prelude::*;
use resinsim_core::services::ThermalCalculator;
use resinsim_core::values::ThermalTimeConstant;

proptest! {
    /// KB-150: Vat temperature at t=0 equals ambient.
    #[test]
    fn vat_temp_at_t0_is_ambient(
        ambient in -10.0f32..40.0,
        delta_t in 1.0f32..30.0,
        tau in 100.0f32..5000.0,
    ) {
        let t = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).unwrap(), 0.0);
        prop_assert!((t.value() - ambient).abs() < 1e-4,
            "at t=0 should be ambient: {} vs {}", t.value(), ambient);
    }

    /// Temperature is monotonically increasing with time (for positive ΔT).
    #[test]
    fn vat_temp_monotonic_with_time(
        ambient in 15.0f32..30.0,
        delta_t in 1.0f32..20.0,
        tau in 100.0f32..3000.0,
        t1 in 0.0f32..10000.0,
        t2 in 0.0f32..10000.0,
    ) {
        let temp1 = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).unwrap(), t1);
        let temp2 = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).unwrap(), t2);
        if t1 <= t2 {
            prop_assert!(temp1.value() <= temp2.value() + 1e-4,
                "temperature should rise with time: t1={t1} T1={}, t2={t2} T2={}", temp1.value(), temp2.value());
        }
    }

    /// Temperature always stays between ambient and ambient + ΔT.
    #[test]
    fn vat_temp_bounded(
        ambient in 10.0f32..35.0,
        delta_t in 1.0f32..20.0,
        tau in 100.0f32..3000.0,
        time in 0.0f32..100000.0,
    ) {
        let t = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).unwrap(), time);
        prop_assert!(t.value() >= ambient - 1e-4, "temp below ambient: {}", t.value());
        prop_assert!(t.value() <= ambient + delta_t + 1e-4,
            "temp above steady state: {} > {}", t.value(), ambient + delta_t);
    }

    /// KB-150: Viscosity at reference temperature equals reference viscosity.
    #[test]
    fn viscosity_at_ref_temp_is_ref(
        mu_ref in 10.0f32..2000.0,
        ref_temp in 15.0f32..35.0,
        ea in 20.0f32..80.0,
    ) {
        let mu = ThermalCalculator::viscosity_at_temperature(mu_ref, ref_temp, ref_temp, ea);
        prop_assert!((mu - mu_ref).abs() < 0.1,
            "at ref temp should be ref viscosity: {} vs {}", mu, mu_ref);
    }

    /// Viscosity decreases with temperature (for positive Ea).
    #[test]
    fn viscosity_decreases_with_temp(
        mu_ref in 50.0f32..1000.0,
        ref_temp in 20.0f32..30.0,
        ea in 20.0f32..80.0,
    ) {
        let mu_warm = ThermalCalculator::viscosity_at_temperature(mu_ref, ref_temp, ref_temp + 10.0, ea);
        prop_assert!(mu_warm < mu_ref,
            "warmer should be less viscous: {} vs {}", mu_warm, mu_ref);
    }

    /// Viscosity increases when cooled (Arrhenius).
    #[test]
    fn viscosity_increases_when_cooled(
        mu_ref in 50.0f32..1000.0,
        ref_temp in 25.0f32..35.0,
        ea in 20.0f32..80.0,
    ) {
        let mu_cold = ThermalCalculator::viscosity_at_temperature(mu_ref, ref_temp, ref_temp - 5.0, ea);
        prop_assert!(mu_cold > mu_ref,
            "cooler should be more viscous: {} vs {}", mu_cold, mu_ref);
    }

    /// Viscosity ratio is symmetric: ratio(T1→T2) × ratio(T2→T1) ≈ 1.
    #[test]
    fn viscosity_ratio_inverse_symmetric(
        ref_temp in 20.0f32..30.0,
        target_temp in 25.0f32..45.0,
        ea in 20.0f32..80.0,
    ) {
        let r1 = ThermalCalculator::viscosity_ratio(ref_temp, target_temp, ea);
        let r2 = ThermalCalculator::viscosity_ratio(target_temp, ref_temp, ea);
        prop_assert!((r1 * r2 - 1.0).abs() < 1e-3,
            "inverse ratios should multiply to 1: {} × {} = {}", r1, r2, r1 * r2);
    }

    /// Viscosity is always positive.
    #[test]
    fn viscosity_always_positive(
        mu_ref in 1.0f32..2000.0,
        ref_temp in 15.0f32..35.0,
        target_temp in 5.0f32..60.0,
        ea in 10.0f32..80.0,
    ) {
        let mu = ThermalCalculator::viscosity_at_temperature(mu_ref, ref_temp, target_temp, ea);
        prop_assert!(mu > 0.0, "viscosity must be positive: {}", mu);
        prop_assert!(mu.is_finite(), "viscosity must be finite: {}", mu);
    }
}
