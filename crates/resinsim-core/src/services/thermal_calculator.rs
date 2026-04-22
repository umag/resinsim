use crate::entities::{PrinterProfile, Recipe};
use crate::services::LayerTimingCalculator;
use crate::values::{ThermalTimeConstant, VatTemperature};

/// Gas constant. Unit: J/(mol·K).
const R: f32 = 8.314;

/// Domain service: vat thermal model and temperature-dependent properties.
/// Stateless — all inputs via parameters.
///
/// # Legacy (single-stage) API — KB-150
///
/// `vat_temperature(ambient, delta_t_steady_c, tau, time)` computes
/// `T = ambient + delta_t × (1 − exp(−t/τ))` directly at the vat level. This
/// conflates two physical effects (LED heat-up + vat coupling) into one
/// lumped rise and was the pre-ADR-0007 model. Still supported for legacy
/// profiles / KB-150 regression vectors.
///
/// # Two-stage API — ADR-0007, KB-152
///
/// Stage A: LED-case temperature over time, from the initial idle baseline
/// to steady-state plateau:
///
/// ```text
/// led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 - exp(-t/tau))
/// ```
///
/// Stage B: vat temperature via coupling factor (LED heat conducts / radiates
/// to vat through the printer frame):
///
/// ```text
/// vat_temp = ambient_c + coupling × (led_temp - ambient_c)
/// ```
///
/// Per-layer entry: [`ThermalCalculator::vat_temperature_at_layer_v2`]
/// composes `LayerTimingCalculator` + stage A + stage B.
pub struct ThermalCalculator;

impl ThermalCalculator {
    /// Vat temperature at time t seconds into the print.
    /// KB-150: T(t) = T_ambient + ΔT × (1 - exp(-t/τ))
    pub fn vat_temperature(
        ambient_c: f32,
        delta_t_steady_c: f32,
        tau: ThermalTimeConstant,
        time_sec: f32,
    ) -> VatTemperature {
        let rise = delta_t_steady_c * (1.0 - (-time_sec / tau.value()).exp());
        VatTemperature::new(ambient_c + rise)
            .expect("ambient + rise from validated profile is finite and above absolute zero for realistic ambient temperatures")
    }

    /// Vat temperature at a specific layer index.
    /// KB-150: t_layer = layer × (exposure + lift_cycle)
    pub fn vat_temperature_at_layer(
        ambient_c: f32,
        delta_t_steady_c: f32,
        tau: ThermalTimeConstant,
        layer: u32,
        exposure_sec: f32,
        lift_cycle_sec: f32,
    ) -> VatTemperature {
        let time = layer as f32 * (exposure_sec + lift_cycle_sec);
        Self::vat_temperature(ambient_c, delta_t_steady_c, tau, time)
    }

    /// Viscosity at a given temperature using Arrhenius model.
    /// KB-150: µ(T) = µ₀ × exp(Ea/(R×T₂)) / exp(Ea/(R×T₁))
    ///       = µ₀ × exp(Ea/R × (1/T₂ - 1/T₁))
    /// where T₁ = reference temp, T₂ = target temp (both in Kelvin).
    pub fn viscosity_at_temperature(
        viscosity_ref_mpa_s: f32,
        ref_temp_c: f32,
        target_temp_c: f32,
        activation_energy_kj_mol: f32,
    ) -> f32 {
        let t1_k = ref_temp_c + 273.15;
        let t2_k = target_temp_c + 273.15;
        let ea_j = activation_energy_kj_mol * 1000.0;
        let exponent = (ea_j / R) * (1.0 / t2_k - 1.0 / t1_k);
        viscosity_ref_mpa_s * exponent.exp()
    }

    /// Viscosity ratio µ(T₂)/µ(T₁).
    pub fn viscosity_ratio(
        ref_temp_c: f32,
        target_temp_c: f32,
        activation_energy_kj_mol: f32,
    ) -> f32 {
        let t1_k = ref_temp_c + 273.15;
        let t2_k = target_temp_c + 273.15;
        let ea_j = activation_energy_kj_mol * 1000.0;
        let exponent = (ea_j / R) * (1.0 / t2_k - 1.0 / t1_k);
        exponent.exp()
    }

    /// Screen duty cycle: fraction of time the LEDs are exposing resin.
    /// KB-151: duty = exposure / (exposure + lift_cycle)
    pub fn duty_cycle(exposure_sec: f32, lift_cycle_sec: f32) -> f32 {
        if exposure_sec + lift_cycle_sec <= 0.0 {
            return 0.0;
        }
        exposure_sec / (exposure_sec + lift_cycle_sec)
    }

    // --- Two-stage model (ADR-0007, KB-152) -------------------------------

    /// Stage A — LED case temperature at time `t` seconds into the print.
    ///
    /// `initial_led_c` is the LED case baseline at t=0 (typically the printer's
    /// idle-standby temperature, NOT room ambient — a printer's LED case sits
    /// a few °C above ambient even with LEDs off due to controller-electronics
    /// dissipation; see data/elegoo/README.md for measurement context).
    ///
    /// `delta_t_steady_c` is the asymptotic LED rise above `initial_led_c`.
    ///
    /// ```text
    /// led_temp(t) = initial_led_c + delta_t_steady_c × (1 - exp(-t/tau))
    /// ```
    pub fn led_temperature_at_time(
        initial_led_c: f32,
        delta_t_steady_c: f32,
        tau: ThermalTimeConstant,
        time_sec: f32,
    ) -> VatTemperature {
        let rise = delta_t_steady_c * (1.0 - (-time_sec / tau.value()).exp());
        VatTemperature::new(initial_led_c + rise).expect(
            "initial_led_c + rise from validated profile is finite and above absolute zero for realistic inputs",
        )
    }

    /// Stage B — vat temperature derived from LED case temperature via a
    /// dimensionless coupling factor. The coupling captures conduction through
    /// the printer frame + radiation through the LCD + convection in the vat.
    ///
    /// ```text
    /// vat_temp = ambient_c + coupling × (led_temp - ambient_c)
    /// ```
    ///
    /// `coupling = 0` ⇒ vat temperature equals ambient (perfect isolation);
    /// `coupling = 1` ⇒ vat temperature equals LED case (perfect coupling);
    /// realistic values sit between, e.g. Mars 5 Ultra ≈ 0.71 (KB-152).
    pub fn vat_temperature_from_led(
        led_temp: VatTemperature,
        ambient_c: f32,
        coupling: f32,
    ) -> VatTemperature {
        VatTemperature::new(ambient_c + coupling * (led_temp.value() - ambient_c)).expect(
            "ambient + bounded-coupling × finite delta is finite and above absolute zero for realistic inputs",
        )
    }

    /// Two-stage per-layer vat temperature: composes `LayerTimingCalculator`
    /// (for cumulative time at `layer_index`) + stage A (LED temp vs time) +
    /// stage B (LED → vat via coupling).
    ///
    /// `initial_led_c` may be `None` (falls back to `ambient_c` — legacy
    /// single-stage behaviour where the LED is assumed to start at ambient).
    pub fn vat_temperature_at_layer_v2(
        recipe: &Recipe,
        printer: &PrinterProfile,
        ambient_c: f32,
        initial_led_c: Option<f32>,
        layer_index: u32,
    ) -> VatTemperature {
        let initial_led_c = initial_led_c.unwrap_or(ambient_c);
        // Cumulative time up to and including this layer. Layer 0's cumulative
        // time includes its own release motion — matching the legacy model
        // which placed t=0 at print start (before layer 0's exposure).
        // For layer 0 at t=0 we want stage A = initial_led_c. So feed the
        // cumulative time UP TO the START of this layer (exclusive), which is
        // what cumulative_times_sec[layer - 1] provides for layer >= 1.
        let time_sec = if layer_index == 0 {
            0.0
        } else {
            LayerTimingCalculator::cumulative_times_sec(recipe, printer, layer_index)
                .last()
                .copied()
                .unwrap_or(0.0)
        };
        let tau = ThermalTimeConstant::new(printer.led_tau_sec())
            .expect("PrinterProfile::validate() guarantees led_tau_sec > 0");
        let led_temp = Self::led_temperature_at_time(
            initial_led_c,
            printer.led_delta_t_steady_c(),
            tau,
            time_sec,
        );
        Self::vat_temperature_from_led(led_temp, ambient_c, printer.led_to_vat_coupling())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tau(sec: f32) -> ThermalTimeConstant {
        ThermalTimeConstant::new(sec)
            .expect("test fixture: positive finite seconds is in ThermalTimeConstant domain")
    }

    // --- KB-150 temperature test vectors ---

    #[test]
    fn vat_temp_at_t0_equals_ambient() {
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 0.0);
        assert!((t.value() - 22.0).abs() < 1e-6);
    }

    #[test]
    fn vat_temp_at_tau_is_63_pct_rise() {
        // At t=τ: rise = ΔT × (1 - 1/e) = 10 × 0.632 = 6.32
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 1200.0);
        assert!((t.value() - 28.32).abs() < 0.1);
    }

    #[test]
    fn vat_temp_at_10min_matches_kb150() {
        // KB-150 vector: T_ambient=22, ΔT=10, τ=1200, t=600 → 25.9
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 600.0);
        assert!((t.value() - 25.9).abs() < 0.1);
    }

    #[test]
    fn vat_temp_at_20min_matches_kb150() {
        // KB-150: t=1200 → 28.3
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 1200.0);
        assert!((t.value() - 28.3).abs() < 0.1);
    }

    #[test]
    fn vat_temp_at_40min_matches_kb150() {
        // KB-150: t=2400 → 30.6
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 2400.0);
        assert!((t.value() - 30.6).abs() < 0.1);
    }

    #[test]
    fn vat_temp_approaches_steady_state() {
        // KB-150: t=6000 → ~31.9 (99% rise)
        let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 6000.0);
        assert!((t.value() - 32.0).abs() < 0.2);
    }

    // --- Per-layer temperature ---

    #[test]
    fn vat_temp_layer_0_equals_ambient() {
        let t = ThermalCalculator::vat_temperature_at_layer(22.0, 10.0, tau(1200.0), 0, 2.5, 7.5);
        assert!((t.value() - 22.0).abs() < 1e-6);
    }

    #[test]
    fn vat_temp_layer_100_matches_kb150() {
        // KB-150: layer 100, 10s/layer → t=1000s → 27.7°C
        let t = ThermalCalculator::vat_temperature_at_layer(22.0, 10.0, tau(1200.0), 100, 2.5, 7.5);
        assert!((t.value() - 27.7).abs() < 0.1);
    }

    // --- KB-150 viscosity test vectors ---

    #[test]
    fn viscosity_at_ref_temp_equals_ref() {
        let mu = ThermalCalculator::viscosity_at_temperature(200.0, 25.0, 25.0, 52.0);
        assert!((mu - 200.0).abs() < 0.1);
    }

    #[test]
    fn viscosity_drops_with_temperature() {
        let mu_25 = ThermalCalculator::viscosity_at_temperature(200.0, 25.0, 25.0, 52.0);
        let mu_50 = ThermalCalculator::viscosity_at_temperature(200.0, 25.0, 50.0, 52.0);
        assert!(mu_50 < mu_25);
    }

    #[test]
    fn viscosity_82pct_drop_at_50c_with_ea_52() {
        // KB-141: 82% drop from 25→50°C. With Ea=52 kJ/mol this should match.
        let ratio = ThermalCalculator::viscosity_ratio(25.0, 50.0, 52.0);
        // 82% drop means ratio ≈ 0.18
        assert!((ratio - 0.18).abs() < 0.03);
    }

    #[test]
    fn viscosity_ratio_at_30c() {
        // KB-150 vector: T=30°C → µ/µ₀ ≈ 0.775 (Ea=36 kJ/mol)
        let ratio = ThermalCalculator::viscosity_ratio(25.0, 30.0, 36.0);
        assert!((ratio - 0.775).abs() < 0.02);
    }

    #[test]
    fn viscosity_increases_when_cooled() {
        let mu_20 = ThermalCalculator::viscosity_at_temperature(200.0, 25.0, 20.0, 52.0);
        assert!(mu_20 > 200.0);
    }

    // --- Duty cycle ---

    #[test]
    fn duty_cycle_typical() {
        // KB-151: 2.5s exposure / 10s total = 25%
        let dc = ThermalCalculator::duty_cycle(2.5, 7.5);
        assert!((dc - 0.25).abs() < 1e-6);
    }

    #[test]
    fn duty_cycle_long_exposure() {
        // KB-151: 8s / 20s = 40%
        let dc = ThermalCalculator::duty_cycle(8.0, 12.0);
        assert!((dc - 0.40).abs() < 1e-6);
    }

    #[test]
    fn duty_cycle_zero_when_no_exposure() {
        let dc = ThermalCalculator::duty_cycle(0.0, 10.0);
        assert!((dc).abs() < 1e-6);
    }

    // --- Degradation threshold ---

    #[test]
    fn degradation_risk_detected() {
        use crate::entities::ResinProfile;
        let t = ThermalCalculator::vat_temperature(40.0, 15.0, tau(600.0), 3000.0);
        let resin = ResinProfile::generic_standard();
        assert!(resin.is_degradation_risk(t)); // 40 + ~15 = ~55°C
    }

    // --- Two-stage model (ADR-0007, KB-152) tests -------------------------

    #[test]
    fn stage_a_led_temp_at_t0_equals_initial() {
        let t = ThermalCalculator::led_temperature_at_time(27.0, 13.5, tau(4000.0), 0.0);
        assert!((t.value() - 27.0).abs() < 1e-6);
    }

    #[test]
    fn stage_a_led_temp_approaches_steady_state() {
        // At t = 5τ the rise is 99.3% of the asymptote.
        let t = ThermalCalculator::led_temperature_at_time(27.0, 13.5, tau(4000.0), 20_000.0);
        let asymptote = 27.0 + 13.5;
        assert!((t.value() - asymptote).abs() < 0.1);
    }

    #[test]
    fn stage_a_led_temp_at_tau_is_63_pct() {
        // At t = τ the rise is (1 - 1/e) = 63.2% of the asymptote.
        let t = ThermalCalculator::led_temperature_at_time(27.0, 13.5, tau(4000.0), 4000.0);
        let expected = 27.0 + 13.5 * (1.0 - std::f32::consts::E.recip());
        assert!((t.value() - expected).abs() < 1e-3);
    }

    #[test]
    fn stage_b_coupling_zero_returns_ambient() {
        let led = VatTemperature::new(40.0).expect("40 °C is valid");
        let vat = ThermalCalculator::vat_temperature_from_led(led, 23.0, 0.0);
        assert!((vat.value() - 23.0).abs() < 1e-6);
    }

    #[test]
    fn stage_b_coupling_one_returns_led_temp() {
        let led = VatTemperature::new(40.0).expect("40 °C is valid");
        let vat = ThermalCalculator::vat_temperature_from_led(led, 23.0, 1.0);
        assert!((vat.value() - 40.0).abs() < 1e-6);
    }

    #[test]
    fn stage_b_mars5_ultra_coupling_matches_user_estimate() {
        // Mars 5 Ultra: LED 40, ambient 23, coupling 0.71 ⇒ vat ≈ 35.07.
        let led = VatTemperature::new(40.0).expect("40 °C is valid");
        let vat = ThermalCalculator::vat_temperature_from_led(led, 23.0, 0.71);
        assert!((vat.value() - 35.07).abs() < 0.01);
    }

    #[test]
    fn stage_b_vat_temp_bounded_between_ambient_and_led() {
        let led = VatTemperature::new(40.0).expect("40 °C is valid");
        let ambient = 23.0;
        // Any coupling in [0, 1] must produce a vat temp in [ambient, led].
        for c in [0.0, 0.1, 0.3, 0.5, 0.71, 0.9, 1.0] {
            let vat = ThermalCalculator::vat_temperature_from_led(led, ambient, c);
            assert!(vat.value() >= ambient - 1e-5);
            assert!(vat.value() <= led.value() + 1e-5);
        }
    }

    #[test]
    fn v2_layer_0_returns_initial_led_mapped_to_vat() {
        // At layer 0 (t=0): led_temp = initial_led_c = 27,
        // vat_temp = 23 + 0.71 × (27 - 23) = 25.84.
        use crate::entities::{PrinterProfile, Recipe};
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::elegoo_mars5_ultra();
        let vat = ThermalCalculator::vat_temperature_at_layer_v2(
            &recipe,
            &printer,
            23.0,       // ambient
            Some(27.0), // initial_led
            0,          // layer index
        );
        let expected = 23.0 + 0.71 * (27.0 - 23.0);
        assert!(
            (vat.value() - expected).abs() < 0.01,
            "layer-0 vat: expected {expected:.3}, got {:.3}",
            vat.value(),
        );
    }

    #[test]
    fn v2_legacy_path_initial_led_none_equals_ambient() {
        // initial_led_c = None ⇒ uses ambient as initial. At layer 0, LED = ambient,
        // so vat = ambient + coupling × 0 = ambient.
        use crate::entities::{PrinterProfile, Recipe};
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::elegoo_mars5_ultra();
        let vat =
            ThermalCalculator::vat_temperature_at_layer_v2(&recipe, &printer, 23.0, None, 0);
        assert!((vat.value() - 23.0).abs() < 1e-4);
    }

    #[test]
    fn v2_plateau_after_many_layers() {
        // Many normal layers at ~10.5 sec each (Tilt Mars 5 Ultra) ⇒ cumulative time
        // hits the asymptote region. At t >> τ: led → initial + delta = 27 + 13.5 = 40.5,
        // vat → 23 + 0.71 × (40.5 - 23) = 35.425.
        use crate::entities::{PrinterProfile, Recipe};
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::elegoo_mars5_ultra();
        // Tilt normal layer = 10.5 sec. 2000 layers ≈ 21000 sec ≈ 5.25τ.
        let vat = ThermalCalculator::vat_temperature_at_layer_v2(
            &recipe,
            &printer,
            23.0,
            Some(27.0),
            2000,
        );
        let asymptote_led = 27.0 + 13.5;
        let asymptote_vat = 23.0 + 0.71 * (asymptote_led - 23.0);
        // Within 1% of asymptote after 5τ.
        assert!(
            (vat.value() - asymptote_vat).abs() < 0.3,
            "plateau vat: expected ≈{asymptote_vat:.2}, got {:.2}",
            vat.value(),
        );
    }

    #[test]
    fn v2_legacy_delegation_matches_kb150_vector() {
        // Regression test: the two-stage model, run with initial_led_c = ambient and
        // a printer whose led_delta_t_steady_c equals the legacy delta_t_steady_c
        // AND led_to_vat_coupling = 1.0, must produce the same result as the legacy
        // single-stage vat_temperature. This proves the new API is a strict superset
        // of the old and preserves KB-150 semantics under the right parameterisation.
        let old = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 1000.0);
        let new_led = ThermalCalculator::led_temperature_at_time(22.0, 10.0, tau(1200.0), 1000.0);
        let new_vat = ThermalCalculator::vat_temperature_from_led(new_led, 22.0, 1.0);
        assert!((old.value() - new_vat.value()).abs() < 1e-5);
    }
}
