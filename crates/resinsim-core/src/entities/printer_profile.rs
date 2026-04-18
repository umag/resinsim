use serde::{Deserialize, Serialize};

/// Mechanical and optical properties of a printer.
/// Identity: name. Loaded from TOML profiles in data/printers/.
///
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` — external code cannot construct or mutate a
/// `PrinterProfile`. Construction is restricted to the factory methods on
/// this type (`generic_msla_4k`, `elegoo_mars5_ultra`) and to TOML
/// deserialisation via `PrinterProfileRepository`, both of which run
/// `validate()` before returning. After any field mutation by intra-crate
/// code (typically tests), `validate()` MUST be re-called before treating
/// the profile as trusted by downstream services. `simulation_runner`
/// provides defence-in-depth by calling `validate()` again at run entry
/// (lines 43, 80). See `docs/patterns/entity-validate-on-mutation.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterProfile {
    pub(crate) name: String,

    // Light source
    /// LED power density at LCD plane. Unit: mW/cm². KB-121.
    pub(crate) led_power_mw_cm2: f32,
    /// Physical pixel size. Unit: µm. KB-160.
    pub(crate) pixel_pitch_um: f32,

    // Motion
    /// Lift speed. Unit: mm/min.
    pub(crate) lift_speed_mm_min: f32,
    /// Reference speed at which peel adhesion was measured. Unit: mm/min.
    pub(crate) ref_lift_speed_mm_min: f32,
    /// Lift distance. Unit: mm.
    pub(crate) lift_distance_mm: f32,
    /// Time for non-exposure portion of layer cycle. Unit: seconds.
    pub(crate) lift_cycle_sec: f32,

    // Exposure
    /// Normal layer exposure time. Unit: seconds.
    pub(crate) normal_exposure_sec: f32,
    /// Bottom layer exposure time. Unit: seconds.
    pub(crate) bottom_exposure_sec: f32,
    /// Number of bottom layers.
    pub(crate) bottom_layer_count: u32,
    /// Layer height. Unit: µm.
    pub(crate) layer_height_um: f32,

    // Z-axis
    /// Z-axis stiffness. Unit: N/mm. KB-131, KB-182.
    pub(crate) z_stiffness_n_per_mm: f32,

    // Thermal
    /// Steady-state temperature rise above ambient. Unit: °C. KB-183.
    pub(crate) delta_t_steady_c: f32,
    /// Thermal time constant. Unit: seconds. KB-183.
    pub(crate) thermal_tau_sec: f32,

    // LCD uniformity — KB-120
    /// Peak-to-peak intensity variation as fraction (0.34 = 34%). 0.0 = ideal.
    pub(crate) lcd_uniformity_variation: f32,
}

impl PrinterProfile {
    /// Printer profile identity (used for display + matching by name).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Validate physical invariants. Must be called after deserialization from
    /// untrusted sources to prevent NaN/inf propagation through motion and
    /// thermal calculations.
    ///
    /// **Contract:** intra-crate code that mutates any field of a previously
    /// validated `PrinterProfile` MUST re-call `validate()` before passing the
    /// profile to a downstream service. See struct-level doc comment.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("printer name must not be empty".into());
        }
        let positive_checks: &[(f32, &str)] = &[
            (self.led_power_mw_cm2, "led_power_mw_cm2"),
            (self.pixel_pitch_um, "pixel_pitch_um"),
            (self.lift_speed_mm_min, "lift_speed_mm_min"),
            (self.ref_lift_speed_mm_min, "ref_lift_speed_mm_min"),
            (self.lift_distance_mm, "lift_distance_mm"),
            (self.lift_cycle_sec, "lift_cycle_sec"),
            (self.normal_exposure_sec, "normal_exposure_sec"),
            (self.bottom_exposure_sec, "bottom_exposure_sec"),
            (self.layer_height_um, "layer_height_um"),
            (self.z_stiffness_n_per_mm, "z_stiffness_n_per_mm"),
            (self.thermal_tau_sec, "thermal_tau_sec"),
        ];
        for (val, field) in positive_checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        if !self.delta_t_steady_c.is_finite() {
            return Err(format!(
                "delta_t_steady_c must be finite (got {})",
                self.delta_t_steady_c
            ));
        }
        if !(0.0..=1.0).contains(&self.lcd_uniformity_variation)
            || !self.lcd_uniformity_variation.is_finite()
        {
            return Err(format!(
                "lcd_uniformity_variation must be in [0.0, 1.0] (got {})",
                self.lcd_uniformity_variation
            ));
        }
        Ok(())
    }

    /// Elegoo Mars 5 Ultra — 12K mono LCD, ParaLED, triple linear rail.
    /// Sources: Elegoo published specs; z_stiffness estimated from triple-rail
    /// geometry (calibrate with Athena II data when available).
    pub fn elegoo_mars5_ultra() -> Self {
        Self {
            name: "Elegoo Mars 5 Ultra".into(),
            led_power_mw_cm2: 20.0,        // ParaLED matrix, 12K mono LCD
            pixel_pitch_um: 19.0,          // 218.88 mm / 11520 px
            lift_speed_mm_min: 65.0,
            ref_lift_speed_mm_min: 65.0,
            lift_distance_mm: 6.0,
            lift_cycle_sec: 6.5,
            normal_exposure_sec: 2.0,
            bottom_exposure_sec: 30.0,
            bottom_layer_count: 6,
            layer_height_um: 50.0,
            z_stiffness_n_per_mm: 900.0,   // estimated — triple linear rail
            delta_t_steady_c: 8.0,
            thermal_tau_sec: 1200.0,
            lcd_uniformity_variation: 0.12, // ParaLED, better than Saturn-class
        }
    }

    /// Generic MSLA 4K printer with conservative defaults.
    pub fn generic_msla_4k() -> Self {
        Self {
            name: "Generic MSLA 4K".into(),
            led_power_mw_cm2: 4.0,     // KB-121: typical LCD printer
            pixel_pitch_um: 50.0,
            lift_speed_mm_min: 60.0,
            ref_lift_speed_mm_min: 60.0,
            lift_distance_mm: 5.0,
            lift_cycle_sec: 7.5,
            normal_exposure_sec: 2.5,
            bottom_exposure_sec: 25.0,
            bottom_layer_count: 6,
            layer_height_um: 50.0,
            z_stiffness_n_per_mm: 460.0, // KB-131: Elegoo Mars class
            delta_t_steady_c: 10.0,      // KB-150: estimate
            thermal_tau_sec: 1200.0,     // KB-183: estimate
            lcd_uniformity_variation: 0.22, // KB-120: Saturn 2 class
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_msla_4k_passes_validation() {
        PrinterProfile::generic_msla_4k().validate().unwrap();
    }

    #[test]
    fn zero_layer_height_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.layer_height_um = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_above_one_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.lcd_uniformity_variation = 1.5;
        assert!(p.validate().is_err());
    }

    #[test]
    fn infinite_z_stiffness_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.z_stiffness_n_per_mm = f32::INFINITY;
        assert!(p.validate().is_err());
    }

    // Contract demonstration — see PrinterProfile struct doc comment.
    // Mirrors ResinProfile's validate_after_mutation_contract from T1-F5
    // (docs/patterns/entity-validate-on-mutation.md). Distinct from the
    // numeric range tests above because this exercises the "previously-VALID
    // → mutated → invalid" sequence on a string field, demonstrating the
    // contract requires re-running validate() after intra-crate mutation.
    #[test]
    fn validate_after_mutation_contract() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.validate().expect("baseline profile must be valid");
        p.name = "   ".into();
        assert!(
            p.validate().is_err(),
            "validate() must be re-called after intra-crate field mutation; \
             whitespace name should now be rejected"
        );
    }
}
