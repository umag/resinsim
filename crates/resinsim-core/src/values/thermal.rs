use std::fmt;

use serde::{Deserialize, Serialize};

/// Vat temperature at a given point in time. Unit: °C.
/// Rises during printing due to screen heat. KB-150.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct VatTemperature(pub(crate) f32);

/// Heat flux from LCD/LED screen into resin. Unit: Watts.
/// Q = P_led × duty_cycle × A_exposed. KB-151.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ScreenHeatFlux(pub(crate) f32);

/// Thermal time constant of resin volume. Unit: seconds.
/// Controls how fast vat approaches steady-state temperature. KB-183.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ThermalTimeConstant(pub(crate) f32);

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

impl ScreenHeatFlux {
    pub fn new(watts: f32) -> Result<Self, String> {
        if !watts.is_finite() {
            return Err(format!("screen heat flux must be finite, got {watts}"));
        }
        if watts < 0.0 {
            return Err(format!("screen heat flux must be non-negative, got {watts}"));
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
        let t = VatTemperature(25.0);
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
        assert_eq!(VatTemperature::new(25.0).unwrap().value(), 25.0);
    }

    #[test]
    fn vat_temperature_new_rejects_infinity() {
        assert!(VatTemperature::new(f32::INFINITY).is_err());
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
        assert_eq!(ScreenHeatFlux::new(0.0).unwrap().value(), 0.0);
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
        assert_eq!(ThermalTimeConstant::new(1200.0).unwrap().value(), 1200.0);
    }
}
