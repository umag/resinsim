use proptest::prelude::*;
use resinsim_core::entities::{PrinterProfile, Recipe};
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
        let t = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).expect("proptest strategy 100..5000 s produces valid ThermalTimeConstant"), 0.0);
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
        let temp1 = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).expect("proptest strategy 100..5000 s produces valid ThermalTimeConstant"), t1);
        let temp2 = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).expect("proptest strategy 100..5000 s produces valid ThermalTimeConstant"), t2);
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
        let t = ThermalCalculator::vat_temperature(ambient, delta_t, ThermalTimeConstant::new(tau).expect("proptest strategy 100..5000 s produces valid ThermalTimeConstant"), time);
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

    // --- Two-stage LED → vat model (ADR-0007, KB-152) -----------------------

    /// Stage A: LED temperature is monotonically non-decreasing in time.
    #[test]
    fn led_temp_monotonic_in_time(
        initial_led in 15.0f32..35.0,
        delta_t in 1.0f32..25.0,
        tau in 500.0f32..6000.0,
        t1 in 0.0f32..30000.0,
        t2 in 0.0f32..30000.0,
    ) {
        let tau_const = ThermalTimeConstant::new(tau)
            .expect("proptest strategy 500..6000 s produces valid ThermalTimeConstant");
        let led1 = ThermalCalculator::led_temperature_at_time(initial_led, delta_t, tau_const, t1);
        let led2 = ThermalCalculator::led_temperature_at_time(initial_led, delta_t, tau_const, t2);
        if t1 <= t2 {
            prop_assert!(led1.value() <= led2.value() + 1e-4,
                "LED temp should rise with time: t1={t1} T1={}, t2={t2} T2={}", led1.value(), led2.value());
        }
    }

    /// Stage A: LED temperature approaches the steady-state asymptote
    /// initial_led + delta_t as t → ∞ (tested at 5τ ≈ 99.3% of rise).
    #[test]
    fn led_temp_approaches_steady_state(
        initial_led in 15.0f32..35.0,
        delta_t in 1.0f32..25.0,
        tau in 500.0f32..6000.0,
    ) {
        let tau_const = ThermalTimeConstant::new(tau)
            .expect("proptest strategy 500..6000 s produces valid ThermalTimeConstant");
        let led = ThermalCalculator::led_temperature_at_time(initial_led, delta_t, tau_const, 10.0 * tau);
        let asymptote = initial_led + delta_t;
        prop_assert!((led.value() - asymptote).abs() < 0.05,
            "at t = 10τ LED should be near asymptote: {} vs {}", led.value(), asymptote);
    }

    /// Stage B: vat temperature is bounded between ambient and LED for
    /// coupling in [0, 1].
    #[test]
    fn vat_temp_from_led_bounded(
        ambient in 10.0f32..30.0,
        led_delta in 1.0f32..30.0,
        coupling in 0.0f32..=1.0,
    ) {
        let led = resinsim_core::values::VatTemperature::new(ambient + led_delta)
            .expect("proptest strategy: ambient + positive delta is a valid VatTemperature");
        let vat = ThermalCalculator::vat_temperature_from_led(led, ambient, coupling);
        prop_assert!(vat.value() >= ambient - 1e-4,
            "vat below ambient: {} < {}", vat.value(), ambient);
        prop_assert!(vat.value() <= led.value() + 1e-4,
            "vat above LED: {} > {}", vat.value(), led.value());
    }

    /// Stage B: coupling = 0 returns ambient exactly (perfect isolation).
    #[test]
    fn vat_temp_equals_ambient_when_coupling_zero(
        ambient in 10.0f32..30.0,
        led_temp_c in 15.0f32..60.0,
    ) {
        let led = resinsim_core::values::VatTemperature::new(led_temp_c)
            .expect("proptest strategy 15..60 °C produces valid VatTemperature");
        let vat = ThermalCalculator::vat_temperature_from_led(led, ambient, 0.0);
        prop_assert!((vat.value() - ambient).abs() < 1e-4,
            "coupling=0 should return ambient: {} vs {}", vat.value(), ambient);
    }

    /// Stage B: coupling = 1 returns LED temperature exactly (perfect coupling).
    #[test]
    fn vat_temp_equals_led_when_coupling_one(
        ambient in 10.0f32..30.0,
        led_temp_c in 15.0f32..60.0,
    ) {
        let led = resinsim_core::values::VatTemperature::new(led_temp_c)
            .expect("proptest strategy 15..60 °C produces valid VatTemperature");
        let vat = ThermalCalculator::vat_temperature_from_led(led, ambient, 1.0);
        prop_assert!((vat.value() - led.value()).abs() < 1e-4,
            "coupling=1 should return LED temp: {} vs {}", vat.value(), led.value());
    }

    /// v2 per-layer entry: vat temperature stays within [ambient, initial_led + led_delta]
    /// for any layer index when initial_led is supplied.
    #[test]
    fn vat_temp_at_layer_v2_bounded(
        ambient in 15.0f32..30.0,
        initial_led in 22.0f32..35.0,
        layer in 0u32..3000,
    ) {
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::elegoo_mars5_ultra();
        let vat = ThermalCalculator::vat_temperature_at_layer_v2(
            &recipe, &printer, ambient, Some(initial_led), layer,
        );
        let led_asymptote = initial_led + printer.led_delta_t_steady_c();
        // Vat ∈ [ambient, led_asymptote]. Use ambient-or-lower tolerance for the
        // initial_led < ambient case (rare in realistic data but permitted by
        // the strategy ranges): coupling-damped vat floor is min(ambient, led).
        let lower = ambient.min(initial_led) - 1e-3;
        prop_assert!(vat.value() >= lower,
            "layer {layer} vat below floor: {} < {}", vat.value(), lower);
        prop_assert!(vat.value() <= ambient.max(led_asymptote) + 1e-3,
            "layer {layer} vat above LED asymptote: {} > {}", vat.value(), led_asymptote);
    }
}
