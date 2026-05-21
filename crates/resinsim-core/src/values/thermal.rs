use std::fmt;

use serde::{Deserialize, Serialize};

/// Vat temperature at a given point in time. Unit: °C.
/// Rises during printing due to screen heat. KB-150.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct VatTemperature(f32);

/// Initial LED case temperature at print start. Unit: °C.
/// Idle-standby baseline BEFORE the UV LEDs ramp up (ADR-0007 / KB-152);
/// typically a few °C above ambient due to controller-electronics dissipation.
/// Feeds `ThermalCalculator::led_temperature_at_time` as the stage-A starting
/// point.
///
/// Enforces the same physical bounds as `VatTemperature` (finite, above
/// absolute zero). Callers that accept a raw `f32` from user input (CLI,
/// TOML) construct via `new` at the trust boundary so unphysical values fail
/// with a readable error rather than panicking mid-simulation.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct InitialLedTemperature(f32);

/// Ambient (room) temperature. Unit: °C.
/// User-supplied print-environment constant — drives the stage-B coupling
/// formula `vat = ambient + coupling × (led − ambient)` (ADR-0007 / KB-152).
///
/// Enforces the same physical bounds as `VatTemperature` and
/// `InitialLedTemperature` (finite, above absolute zero). CLI and
/// SimulationRunner convert from raw `f32` at the trust boundary so unphysical
/// values fail with a readable error rather than panicking mid-simulation.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AmbientTemperature(f32);

/// Heat flux from LCD/LED screen into resin. Unit: Watts.
/// Q = P_led × duty_cycle × A_exposed. KB-151.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ScreenHeatFlux(f32);

/// Thermal time constant of resin volume. Unit: seconds.
/// Controls how fast vat approaches steady-state temperature. KB-183.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ThermalTimeConstant(f32);

/// Thermal conductivity. Unit: W/(m·K).
/// Tier-2 thermal diffusion (ADR-0020, t2f4) input — set on `ResinProfile`
/// (`thermal_conductivity_w_mk`, acrylate ~0.20) and on `PrinterProfile`
/// (`vat_wall_k_w_mk`, Al ~200). Constructor rejects non-finite / non-
/// positive values at the trust boundary.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ThermalConductivity(f32);

/// Mass density. Unit: kg/m³.
/// Tier-2 thermal diffusion input — set on `ResinProfile.density_kg_m3`
/// (acrylate ~1100). Constructor rejects non-finite / non-positive values.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Density(f32);

/// Specific heat capacity. Unit: J/(kg·K).
/// Tier-2 thermal diffusion input — set on `ResinProfile.specific_heat_j_kgk`
/// (acrylate ~1700). Constructor rejects non-finite / non-positive values.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SpecificHeatCapacity(f32);

/// Convective heat-transfer coefficient. Unit: W/(m²·K).
/// Tier-2 thermal diffusion BC input — set on
/// `ResinProfile.convective_top_h_w_m2k` (still-air ~10) and on
/// `PrinterProfile.convective_wall_h_w_m2k` (~8). Constructor rejects
/// non-finite / negative values (zero is allowed = perfectly insulating).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ConvectiveCoefficient(f32);

/// Vat wall thickness. Unit: millimetres.
/// Tier-2 thermal diffusion BC input — set on
/// `PrinterProfile.vat_wall_thickness_mm` (~2.0 for typical MSLA vats).
/// Lumped through `1/h_eff = 1/h_air + wall_thickness/wall_k` per ADR-0020
/// §Decision iv. Constructor rejects non-finite / non-positive values.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct VatWallThickness(f32);

impl VatTemperature {
    /// Absolute zero. Temperatures below this are unphysical.
    const ABSOLUTE_ZERO_C: f32 = -273.15;

    pub fn new(celsius: f32) -> Result<Self, String> {
        if !celsius.is_finite() {
            return Err(format!("vat temperature must be finite, got {celsius}"));
        }
        if celsius <= Self::ABSOLUTE_ZERO_C {
            return Err(format!(
                "vat temperature must be above absolute zero ({:.2} °C), got {celsius}",
                Self::ABSOLUTE_ZERO_C
            ));
        }
        Ok(Self(celsius))
    }

    pub fn to_kelvin(&self) -> f32 {
        self.0 + 273.15
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl InitialLedTemperature {
    /// Absolute zero. Temperatures below this are unphysical.
    const ABSOLUTE_ZERO_C: f32 = -273.15;

    pub fn new(celsius: f32) -> Result<Self, String> {
        if !celsius.is_finite() {
            return Err(format!(
                "initial LED temperature must be finite, got {celsius}"
            ));
        }
        if celsius <= Self::ABSOLUTE_ZERO_C {
            return Err(format!(
                "initial LED temperature must be above absolute zero ({:.2} °C), got {celsius}",
                Self::ABSOLUTE_ZERO_C
            ));
        }
        Ok(Self(celsius))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl AmbientTemperature {
    /// Absolute zero. Temperatures below this are unphysical.
    const ABSOLUTE_ZERO_C: f32 = -273.15;

    pub fn new(celsius: f32) -> Result<Self, String> {
        if !celsius.is_finite() {
            return Err(format!("ambient temperature must be finite, got {celsius}"));
        }
        if celsius <= Self::ABSOLUTE_ZERO_C {
            return Err(format!(
                "ambient temperature must be above absolute zero ({:.2} °C), got {celsius}",
                Self::ABSOLUTE_ZERO_C
            ));
        }
        Ok(Self(celsius))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl ScreenHeatFlux {
    pub fn new(watts: f32) -> Result<Self, String> {
        if !watts.is_finite() {
            return Err(format!("screen heat flux must be finite, got {watts}"));
        }
        if watts < 0.0 {
            return Err(format!(
                "screen heat flux must be non-negative, got {watts}"
            ));
        }
        Ok(Self(watts))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl ThermalTimeConstant {
    pub fn new(sec: f32) -> Result<Self, String> {
        if !sec.is_finite() {
            return Err(format!("thermal time constant must be finite, got {sec}"));
        }
        if sec <= 0.0 {
            return Err(format!("thermal time constant must be positive, got {sec}"));
        }
        Ok(Self(sec))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl ThermalConductivity {
    pub fn new(w_mk: f32) -> Result<Self, String> {
        if !w_mk.is_finite() {
            return Err(format!("thermal conductivity must be finite, got {w_mk}"));
        }
        if w_mk <= 0.0 {
            return Err(format!(
                "thermal conductivity must be positive (W/m·K), got {w_mk}"
            ));
        }
        Ok(Self(w_mk))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Density {
    pub fn new(kg_m3: f32) -> Result<Self, String> {
        if !kg_m3.is_finite() {
            return Err(format!("density must be finite, got {kg_m3}"));
        }
        if kg_m3 <= 0.0 {
            return Err(format!("density must be positive (kg/m³), got {kg_m3}"));
        }
        Ok(Self(kg_m3))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl SpecificHeatCapacity {
    pub fn new(j_kgk: f32) -> Result<Self, String> {
        if !j_kgk.is_finite() {
            return Err(format!("specific heat capacity must be finite, got {j_kgk}"));
        }
        if j_kgk <= 0.0 {
            return Err(format!(
                "specific heat capacity must be positive (J/kg·K), got {j_kgk}"
            ));
        }
        Ok(Self(j_kgk))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl ConvectiveCoefficient {
    pub fn new(w_m2k: f32) -> Result<Self, String> {
        if !w_m2k.is_finite() {
            return Err(format!(
                "convective coefficient must be finite, got {w_m2k}"
            ));
        }
        if w_m2k < 0.0 {
            return Err(format!(
                "convective coefficient must be non-negative (W/m²·K), got {w_m2k}"
            ));
        }
        Ok(Self(w_m2k))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl VatWallThickness {
    pub fn new(mm: f32) -> Result<Self, String> {
        if !mm.is_finite() {
            return Err(format!("vat wall thickness must be finite, got {mm}"));
        }
        if mm <= 0.0 {
            return Err(format!(
                "vat wall thickness must be positive (mm), got {mm}"
            ));
        }
        Ok(Self(mm))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl fmt::Display for VatTemperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} °C", self.0)
    }
}

impl fmt::Display for ScreenHeatFlux {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} W", self.0)
    }
}

impl fmt::Display for ThermalTimeConstant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.0} s", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celsius_to_kelvin() {
        let t =
            VatTemperature::new(25.0).expect("test fixture: 25.0 °C is in VatTemperature domain");
        assert!((t.to_kelvin() - 298.15).abs() < 0.01);
    }

    #[test]
    fn vat_temperature_new_rejects_nan() {
        assert!(VatTemperature::new(f32::NAN).is_err());
    }

    #[test]
    fn vat_temperature_new_rejects_below_absolute_zero() {
        assert!(VatTemperature::new(-273.15).is_err());
        assert!(VatTemperature::new(-300.0).is_err());
    }

    #[test]
    fn vat_temperature_new_accepts_normal() {
        assert_eq!(
            VatTemperature::new(25.0)
                .expect("test fixture: 25.0 °C is in VatTemperature domain")
                .value(),
            25.0
        );
    }

    #[test]
    fn vat_temperature_new_rejects_infinity() {
        assert!(VatTemperature::new(f32::INFINITY).is_err());
    }

    // --- InitialLedTemperature ---

    #[test]
    fn initial_led_temperature_new_rejects_nan() {
        assert!(InitialLedTemperature::new(f32::NAN).is_err());
    }

    #[test]
    fn initial_led_temperature_new_rejects_below_absolute_zero() {
        assert!(InitialLedTemperature::new(-273.15).is_err());
        assert!(InitialLedTemperature::new(-300.0).is_err());
    }

    #[test]
    fn initial_led_temperature_new_rejects_infinity() {
        assert!(InitialLedTemperature::new(f32::INFINITY).is_err());
        assert!(InitialLedTemperature::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn initial_led_temperature_new_accepts_normal() {
        assert_eq!(
            InitialLedTemperature::new(27.0)
                .expect("test fixture: 27.0 °C is in InitialLedTemperature domain")
                .value(),
            27.0
        );
    }

    // --- AmbientTemperature ---

    #[test]
    fn ambient_temperature_new_rejects_nan() {
        assert!(AmbientTemperature::new(f32::NAN).is_err());
    }

    #[test]
    fn ambient_temperature_new_rejects_below_absolute_zero() {
        assert!(AmbientTemperature::new(-273.15).is_err());
        assert!(AmbientTemperature::new(-300.0).is_err());
    }

    #[test]
    fn ambient_temperature_new_rejects_infinity() {
        assert!(AmbientTemperature::new(f32::INFINITY).is_err());
        assert!(AmbientTemperature::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn ambient_temperature_new_accepts_normal() {
        assert_eq!(
            AmbientTemperature::new(22.0)
                .expect("test fixture: 22.0 °C is in AmbientTemperature domain")
                .value(),
            22.0
        );
    }

    #[test]
    fn screen_heat_flux_new_rejects_nan() {
        assert!(ScreenHeatFlux::new(f32::NAN).is_err());
    }

    #[test]
    fn screen_heat_flux_new_rejects_negative() {
        assert!(ScreenHeatFlux::new(-1.0).is_err());
    }

    #[test]
    fn screen_heat_flux_new_accepts_zero() {
        assert_eq!(
            ScreenHeatFlux::new(0.0)
                .expect("test fixture: 0.0 W/m² is in ScreenHeatFlux domain")
                .value(),
            0.0
        );
    }

    #[test]
    fn thermal_time_constant_new_rejects_zero() {
        assert!(ThermalTimeConstant::new(0.0).is_err());
    }

    #[test]
    fn thermal_time_constant_new_rejects_negative() {
        assert!(ThermalTimeConstant::new(-1.0).is_err());
    }

    #[test]
    fn thermal_time_constant_new_rejects_nan() {
        assert!(ThermalTimeConstant::new(f32::NAN).is_err());
    }

    #[test]
    fn thermal_time_constant_new_accepts_positive() {
        assert_eq!(
            ThermalTimeConstant::new(1200.0)
                .expect("test fixture: 1200.0 s is in ThermalTimeConstant domain")
                .value(),
            1200.0
        );
    }

    // --- ADR-0020 / t2f4 typed-boundary VOs ---

    #[test]
    fn thermal_conductivity_rejects_nan_and_nonpositive() {
        assert!(ThermalConductivity::new(f32::NAN).is_err());
        assert!(ThermalConductivity::new(0.0).is_err());
        assert!(ThermalConductivity::new(-0.1).is_err());
        assert_eq!(
            ThermalConductivity::new(0.20)
                .expect("test fixture: 0.20 W/m·K in ThermalConductivity domain")
                .value(),
            0.20
        );
    }

    #[test]
    fn density_rejects_nan_and_nonpositive() {
        assert!(Density::new(f32::NAN).is_err());
        assert!(Density::new(0.0).is_err());
        assert!(Density::new(-1.0).is_err());
        assert_eq!(
            Density::new(1100.0)
                .expect("test fixture: 1100 kg/m³ in Density domain")
                .value(),
            1100.0
        );
    }

    #[test]
    fn specific_heat_capacity_rejects_nan_and_nonpositive() {
        assert!(SpecificHeatCapacity::new(f32::NAN).is_err());
        assert!(SpecificHeatCapacity::new(0.0).is_err());
        assert!(SpecificHeatCapacity::new(-1.0).is_err());
        assert_eq!(
            SpecificHeatCapacity::new(1700.0)
                .expect("test fixture: 1700 J/kg·K in SpecificHeatCapacity domain")
                .value(),
            1700.0
        );
    }

    #[test]
    fn convective_coefficient_rejects_nan_and_negative_but_accepts_zero() {
        assert!(ConvectiveCoefficient::new(f32::NAN).is_err());
        assert!(ConvectiveCoefficient::new(-0.1).is_err());
        // Zero is allowed — represents a perfectly insulating boundary.
        assert_eq!(
            ConvectiveCoefficient::new(0.0)
                .expect("test fixture: zero ConvectiveCoefficient = insulated BC")
                .value(),
            0.0
        );
        assert_eq!(
            ConvectiveCoefficient::new(10.0)
                .expect("test fixture: 10 W/m²·K in ConvectiveCoefficient domain")
                .value(),
            10.0
        );
    }

    #[test]
    fn vat_wall_thickness_rejects_nan_and_nonpositive() {
        assert!(VatWallThickness::new(f32::NAN).is_err());
        assert!(VatWallThickness::new(0.0).is_err());
        assert!(VatWallThickness::new(-1.0).is_err());
        assert_eq!(
            VatWallThickness::new(2.0)
                .expect("test fixture: 2.0 mm in VatWallThickness domain")
                .value(),
            2.0
        );
    }
}
