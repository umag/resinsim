use crate::values::{ThermalTimeConstant, VatTemperature};

/// Gas constant. Unit: J/(mol·K).
const R: f32 = 8.314;

/// Domain service: vat thermal model and temperature-dependent properties.
/// Stateless — all inputs via parameters.
///
/// Core equations (KB-150):
///   T_vat(t) = T_ambient + ΔT_steady × (1 - exp(-t / τ))
///   µ(T) = µ₀ × exp(Ea / (R × T_kelvin))
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
}
