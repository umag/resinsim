use serde::{Deserialize, Serialize};

use crate::values::VatTemperature;

/// Default vat-temperature degradation threshold (°C). KB-150 — most standard
/// resins begin thermal breakdown around 50 °C.
fn default_degradation_temp_c() -> f32 {
    50.0
}

/// Default minimum safe vat temperature (°C). Below this viscosity spikes and
/// peel force grows non-linearly.
fn default_min_safe_temp_c() -> f32 {
    15.0
}

/// Physical properties of a resin formulation.
/// Identity: name. Loaded from TOML profiles in data/resins/.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResinProfile {
    pub name: String,

    // Optical (Beer-Lambert)
    /// Penetration depth at 405nm. Unit: µm. KB-100, KB-101.
    pub penetration_depth_um: f32,
    /// Critical energy at 405nm. Unit: mJ/cm². KB-100, KB-101.
    pub critical_energy_mj_cm2: f32,

    // Mechanical
    /// Tensile strength (post-cure). Unit: MPa. KB-140.
    pub tensile_strength_mpa: f32,
    /// Peel adhesion to FEP. Unit: kPa. KB-110.
    pub peel_adhesion_kpa: f32,

    // Shrinkage
    /// Linear shrinkage. Unit: %. KB-142.
    pub linear_shrinkage_pct: f32,

    // Thermal/Viscosity
    /// Viscosity at reference temperature. Unit: mPa·s. KB-141.
    pub viscosity_mpa_s: f32,
    /// Reference temperature for viscosity. Unit: °C.
    pub reference_temp_c: f32,
    /// Arrhenius activation energy. Unit: kJ/mol. KB-141.
    pub activation_energy_kj_mol: f32,

    /// Density. Unit: g/cm³.
    pub density_g_cm3: f32,

    /// Temperature above which this resin begins thermal degradation. Unit: °C.
    /// KB-150. Default 50 °C for typical standard resins.
    #[serde(default = "default_degradation_temp_c")]
    pub degradation_temp_c: f32,
    /// Temperature below which viscosity spike causes peel/suction problems. Unit: °C.
    /// Default 15 °C for typical standard resins.
    #[serde(default = "default_min_safe_temp_c")]
    pub min_safe_temp_c: f32,
}

impl ResinProfile {
    /// Validate physical invariants. Must be called after deserialization from
    /// untrusted sources (e.g. TOML) to prevent NaN/inf propagation through
    /// downstream Beer-Lambert / Arrhenius calculations.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("resin name must not be empty".into());
        }
        let checks: &[(f32, &str)] = &[
            (self.penetration_depth_um, "penetration_depth_um"),
            (self.critical_energy_mj_cm2, "critical_energy_mj_cm2"),
            (self.tensile_strength_mpa, "tensile_strength_mpa"),
            (self.peel_adhesion_kpa, "peel_adhesion_kpa"),
            (self.viscosity_mpa_s, "viscosity_mpa_s"),
            (self.activation_energy_kj_mol, "activation_energy_kj_mol"),
            (self.density_g_cm3, "density_g_cm3"),
        ];
        for (val, field) in checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        if self.linear_shrinkage_pct < 0.0 || !self.linear_shrinkage_pct.is_finite() {
            return Err(format!(
                "linear_shrinkage_pct must be finite and >= 0 (got {})",
                self.linear_shrinkage_pct
            ));
        }
        if !self.reference_temp_c.is_finite() {
            return Err(format!(
                "reference_temp_c must be finite (got {})",
                self.reference_temp_c
            ));
        }
        if !self.degradation_temp_c.is_finite() {
            return Err(format!(
                "degradation_temp_c must be finite (got {})",
                self.degradation_temp_c
            ));
        }
        if !self.min_safe_temp_c.is_finite() {
            return Err(format!(
                "min_safe_temp_c must be finite (got {})",
                self.min_safe_temp_c
            ));
        }
        if !(self.min_safe_temp_c < self.degradation_temp_c) {
            return Err(format!(
                "min_safe_temp_c ({}) must be strictly less than degradation_temp_c ({})",
                self.min_safe_temp_c, self.degradation_temp_c
            ));
        }
        Ok(())
    }

    /// Whether the given vat temperature exceeds this resin's degradation threshold.
    pub fn is_degradation_risk(&self, vat_temp: VatTemperature) -> bool {
        vat_temp.value() > self.degradation_temp_c
    }

    /// Whether the given vat temperature is below this resin's minimum safe operating point.
    pub fn is_too_cold(&self, vat_temp: VatTemperature) -> bool {
        vat_temp.value() < self.min_safe_temp_c
    }

    /// Elegoo Ceramic Grey V2.
    /// Sources: Elegoo published mechanical specs; optical/adhesion values
    /// estimated from ceramic-filled resin literature (calibrate with Athena II).
    pub fn elegoo_ceramic_grey_v2() -> Self {
        Self {
            name: "Elegoo Ceramic Grey V2".into(),
            penetration_depth_um: 145.0,   // ceramic particles scatter, shallower cure
            critical_energy_mj_cm2: 5.5,
            tensile_strength_mpa: 38.0,    // Elegoo published spec
            peel_adhesion_kpa: 9.5,        // ceramic-filled: lower FEP adhesion than standard
            linear_shrinkage_pct: 0.9,     // ceramic-constrained
            viscosity_mpa_s: 350.0,        // higher viscosity from ceramic filler
            reference_temp_c: 25.0,
            activation_energy_kj_mol: 52.0,
            density_g_cm3: 1.25,           // ceramic filler increases density
            degradation_temp_c: default_degradation_temp_c(),
            min_safe_temp_c: default_min_safe_temp_c(),
        }
    }

    /// Generic standard resin with conservative defaults from KB data.
    pub fn generic_standard() -> Self {
        Self {
            name: "Generic Standard".into(),
            penetration_depth_um: 170.0,   // KB-100: Premium Black
            critical_energy_mj_cm2: 5.0,   // KB-100: Premium Black
            tensile_strength_mpa: 35.0,    // KB-140: conservative
            peel_adhesion_kpa: 13.0,       // KB-110: standard FEP
            linear_shrinkage_pct: 1.5,     // KB-142: standard range
            viscosity_mpa_s: 200.0,        // KB-141: typical
            reference_temp_c: 25.0,
            activation_energy_kj_mol: 52.0, // KB-150: derived from 82% drop
            density_g_cm3: 1.1,
            degradation_temp_c: default_degradation_temp_c(),
            min_safe_temp_c: default_min_safe_temp_c(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_standard_passes_validation() {
        ResinProfile::generic_standard().validate().unwrap();
    }

    #[test]
    fn zero_critical_energy_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.critical_energy_mj_cm2 = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn negative_penetration_depth_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.penetration_depth_um = -5.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_field_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.viscosity_mpa_s = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn empty_name_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.name = "   ".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_degradation_temp_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.degradation_temp_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_min_safe_temp_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn min_safe_above_degradation_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 60.0;
        p.degradation_temp_c = 50.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn min_safe_equal_to_degradation_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 50.0;
        p.degradation_temp_c = 50.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn is_degradation_risk_uses_profile_threshold() {
        let mut p = ResinProfile::generic_standard();
        p.degradation_temp_c = 40.0;
        assert!(p.is_degradation_risk(VatTemperature::new(41.0).unwrap()));
        assert!(!p.is_degradation_risk(VatTemperature::new(39.0).unwrap()));
    }

    #[test]
    fn is_too_cold_uses_profile_threshold() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 18.0;
        assert!(p.is_too_cold(VatTemperature::new(17.0).unwrap()));
        assert!(!p.is_too_cold(VatTemperature::new(20.0).unwrap()));
    }
}
