use std::fmt;

use serde::{Deserialize, Serialize};

/// Penetration depth — depth at which UV intensity drops to 1/e (37%) of surface. Unit: µm.
/// Resin-specific property. Typical range: 40-600 µm (KB-102).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PenetrationDepth(pub(crate) f32);

/// Energy dose at resin surface. Unit: mJ/cm².
/// Calculated as I₀ × exposure_time. Typical range: 1-100 mJ/cm² (KB-103).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Energy(pub(crate) f32);

/// Cure depth — distance UV light solidifies resin. Unit: µm.
/// Computed via Beer-Lambert: Cd = Dp × ln(E / Ec).
/// Positive = overcured, zero = threshold, negative = undercured.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CureDepth(pub(crate) f32);

impl PenetrationDepth {
    pub fn new(um: f32) -> Result<Self, &'static str> {
        if um <= 0.0 {
            return Err("penetration depth must be positive");
        }
        Ok(Self(um))
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Energy {
    pub fn new(mj_cm2: f32) -> Result<Self, &'static str> {
        if mj_cm2 <= 0.0 {
            return Err("energy must be positive");
        }
        Ok(Self(mj_cm2))
    }

    /// Compute energy dose from irradiance and exposure time.
    pub fn from_exposure(irradiance_mw_cm2: f32, exposure_sec: f32) -> Self {
        Self(irradiance_mw_cm2 * exposure_sec)
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl CureDepth {
    /// Construct a cure depth from a finite value.
    /// Negative values are allowed (they indicate an undercured layer).
    pub fn new(um: f32) -> Result<Self, String> {
        if !um.is_finite() {
            return Err(format!("cure depth must be finite, got {um}"));
        }
        Ok(Self(um))
    }

    /// Whether this cure depth is sufficient for a given layer height.
    pub fn is_sufficient(&self, layer_height_um: f32) -> bool {
        self.0 >= layer_height_um
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl fmt::Display for PenetrationDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} µm", self.0)
    }
}

impl fmt::Display for Energy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} mJ/cm²", self.0)
    }
}

impl fmt::Display for CureDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} µm", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn penetration_depth_rejects_zero() {
        assert!(PenetrationDepth::new(0.0).is_err());
    }

    #[test]
    fn penetration_depth_rejects_negative() {
        assert!(PenetrationDepth::new(-10.0).is_err());
    }

    #[test]
    fn penetration_depth_accepts_positive() {
        let dp = PenetrationDepth::new(170.0).unwrap();
        assert_eq!(dp.value(), 170.0);
    }

    #[test]
    fn energy_rejects_zero() {
        assert!(Energy::new(0.0).is_err());
    }

    #[test]
    fn energy_from_exposure_computes_dose() {
        // KB-121: 4.0 mW/cm² × 2.5s = 10.0 mJ/cm²
        let e = Energy::from_exposure(4.0, 2.5);
        assert!((e.value() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn cure_depth_sufficient_when_exceeds_layer() {
        let cd = CureDepth(100.0);
        assert!(cd.is_sufficient(50.0));
    }

    #[test]
    fn cure_depth_insufficient_when_below_layer() {
        let cd = CureDepth(30.0);
        assert!(!cd.is_sufficient(50.0));
    }

    #[test]
    fn cure_depth_display() {
        let cd = CureDepth(117.7);
        assert_eq!(format!("{cd}"), "117.7 µm");
    }

    #[test]
    fn energy_display() {
        let e = Energy(10.0);
        assert_eq!(format!("{e}"), "10.00 mJ/cm²");
    }

    #[test]
    fn cure_depth_new_accepts_negative() {
        // Undercured layer is a legitimate state to represent.
        let cd = CureDepth::new(-10.0).unwrap();
        assert_eq!(cd.value(), -10.0);
    }

    #[test]
    fn cure_depth_new_accepts_zero() {
        let cd = CureDepth::new(0.0).unwrap();
        assert_eq!(cd.value(), 0.0);
    }

    #[test]
    fn cure_depth_new_rejects_nan() {
        assert!(CureDepth::new(f32::NAN).is_err());
    }

    #[test]
    fn cure_depth_new_rejects_infinity() {
        assert!(CureDepth::new(f32::INFINITY).is_err());
        assert!(CureDepth::new(f32::NEG_INFINITY).is_err());
    }
}
