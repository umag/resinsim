use serde::{Deserialize, Serialize};

use crate::values::{DEFAULT_VOXEL_SIZE_MM, FloatRange};

fn default_voxel_size_mm() -> f32 {
    DEFAULT_VOXEL_SIZE_MM
}

/// Conservative legacy-matching default for LED steady-state rise. Parallels
/// existing `delta_t_steady_c` legacy default. Unit: °C.
fn default_led_delta_t_steady_c() -> f32 {
    10.0
}

/// Conservative legacy-matching default for LED thermal time constant. Unit: seconds.
fn default_led_tau_sec() -> f32 {
    1200.0
}

/// Conservative midpoint default for LED → vat coupling factor. Dimensionless.
/// A legacy profile without measurements gets 0.5 (halfway between no coupling
/// and full coupling). Known-calibrated profiles override (Mars 5 Ultra = 0.71).
fn default_led_to_vat_coupling() -> f32 {
    0.5
}

/// Hardware build envelope of a printer (ADR-0012, extends ADR-0005 Axis 1).
///
/// Optional on [`PrinterProfile`] — see ADR-0012. When present, all three
/// dimensions must be positive and finite. When absent, viz consumers must
/// fall back to alternative envelope sources (e.g. CTB header bed_size_mm
/// plus a sentinel max_z) and surface the missing-envelope state to the
/// user (typically via a one-shot warn).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BuildEnvelope {
    /// Build-volume X extent (LCD-pixel-grid axis). Unit: mm.
    pub width_mm: f32,
    /// Build-volume Y extent (LCD-pixel-grid axis). Unit: mm.
    pub depth_mm: f32,
    /// Build-volume Z extent (build axis). Unit: mm.
    pub max_z_mm: f32,
}

impl BuildEnvelope {
    /// Validate that all three extents are positive and finite. Called from
    /// [`PrinterProfile::validate`] when the envelope field is `Some`.
    pub fn validate(&self) -> Result<(), String> {
        let checks: &[(f32, &str)] = &[
            (self.width_mm, "width_mm"),
            (self.depth_mm, "depth_mm"),
            (self.max_z_mm, "max_z_mm"),
        ];
        for (val, field) in checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!(
                    "build_envelope_mm.{field} must be finite and > 0 (got {val})"
                ));
            }
        }
        Ok(())
    }
}

/// How the printer separates a cured layer from the FEP film (ADR-0007).
///
/// Distinct mechanical paradigms produce distinct per-layer time profiles, so
/// `LayerTimingCalculator` branches on this value. Two variants today; more
/// may be added (e.g. Anycubic peel-lift hybrids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseMechanism {
    /// Build plate (arm) lifts straight up, vat stationary. Athena II, Mars 4,
    /// classic MSLA. Recipe's `lift_distance_mm` + `lift_speed_mm_min` represent
    /// actual plate travel distance and speed.
    #[default]
    Linear,
    /// Vat hinges to peel layer off, plate stationary. Mars 5 Ultra, Saturn 4
    /// variants. Recipe's `lift_distance_mm` + `lift_speed_mm_min` are CTB-file
    /// METADATA ONLY for Tilt printers — `LayerTimingCalculator` does NOT use
    /// them; it uses `Recipe::lift_cycle_sec` as the canonical lumped release
    /// duration. Tilt-angular geometry refinement
    /// (`tilt_angle_deg` × `tilt_rate_deg_s`) is a follow-on issue.
    Tilt,
}

/// Hardware envelope of a printer (ADR-0005, Axis 1).
/// Identity: `name`. Loaded from TOML profiles in `data/printers/`.
///
/// # Hardware vs Recipe
///
/// This type carries only hardware-intrinsic fields (LED power, pixel pitch, Z-axis
/// stiffness, LCD uniformity, thermal, bed size) and the **range envelopes** that
/// bound which `Recipe` values a resin may request on this printer. Recipe fields
/// (exposure times, layer height, lift kinematics) live on `ResinProfile::recipe()`.
/// See ADR-0005.
///
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` — external code cannot construct or mutate a
/// `PrinterProfile`. Construction is restricted to the factory methods on this type
/// (`generic_msla_4k`, `elegoo_mars5_ultra`) and to TOML deserialisation via
/// `PrinterProfileRepository`, both of which run `validate()` before returning.
/// After any field mutation by intra-crate code (typically tests), `validate()` MUST
/// be re-called before treating the profile as trusted. `simulation_runner` provides
/// defence-in-depth by calling `validate()` again at run entry. See
/// `docs/patterns/entity-validate-on-mutation.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterProfile {
    pub(crate) name: String,

    // Light source
    /// LED power density at LCD plane. Unit: mW/cm². KB-121.
    pub(crate) led_power_mw_cm2: f32,
    /// Physical pixel size. Unit: µm. KB-160.
    pub(crate) pixel_pitch_um: f32,

    // Hardware envelope (ADR-0005 Axis 1)
    /// Range of valid layer heights for this printer. Unit: µm.
    pub(crate) layer_height_range_um: FloatRange,
    /// Range of valid exposure times for this printer. Unit: seconds.
    pub(crate) exposure_range_sec: FloatRange,
    /// Range of valid lift speeds for this printer. Unit: mm/min.
    pub(crate) lift_speed_range_mm_min: FloatRange,
    /// Maximum supported bottom-layer count. Scalar ceiling, not a range —
    /// lower bound has no hardware meaning (see ADR-0005 §2).
    pub(crate) bottom_layer_count_max: u32,

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

    /// Voxel resolution in mm for slicer mask output (Step 4 of
    /// suction-detector-raft-false-positive). Controls memory budget of
    /// per-layer `LayerMask` stacks used by `CavityDetector`. Finer values
    /// catch thinner walls at the cost of memory; coarser values save memory
    /// but may over-report solid area around sub-mm features.
    ///
    /// Default 0.5 mm (`DEFAULT_VOXEL_SIZE_MM`). Existing TOML profiles
    /// without this field deserialise with the default via `#[serde(default)]`.
    #[serde(default = "default_voxel_size_mm")]
    pub(crate) voxel_size_mm: f32,

    /// How the printer separates cured layers from the FEP film. Selects the
    /// time-per-layer branch in `LayerTimingCalculator`. Legacy TOML profiles
    /// without this field deserialise as `Linear` via `#[serde(default)]`
    /// (matches classic MSLA arm-lift behaviour).
    #[serde(default)]
    pub(crate) release_mechanism: ReleaseMechanism,

    // --- LED thermal calibration (ADR-0007, KB-152) ---
    //
    // DELTA semantics — parallels existing `delta_t_steady_c` (the vat-level rise).
    // NOT an absolute temperature. `ThermalCalculator` stage A computes:
    //   led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 - exp(-t/tau))
    //
    /// Steady-state LED-case temperature rise above `initial_led_c`. Unit: °C.
    /// For Mars 5 Ultra: 13.5 °C (plateau ~40.5 °C − idle ~27 °C from
    /// `data/elegoo/roden_uv_led_temp_dec_jan_hourly.csv`).
    #[serde(default = "default_led_delta_t_steady_c")]
    pub(crate) led_delta_t_steady_c: f32,

    /// Time constant of the LED heat-up curve. Unit: seconds. For Mars 5
    /// Ultra: 4000 s (3–4 h to 95 % of plateau ⇒ 3τ ≈ 3–4 h).
    #[serde(default = "default_led_tau_sec")]
    pub(crate) led_tau_sec: f32,

    /// LED → vat coupling factor. Dimensionless, [0, 1]. `ThermalCalculator`
    /// stage B computes: `vat_temp = ambient_c + coupling × (led_temp - ambient_c)`.
    /// For Mars 5 Ultra: 0.71 (user estimate — LED 40, vat ~35, ambient 23 ⇒
    /// ΔT_led = 17, ΔT_vat = 12, 12/17 ≈ 0.71). ESTIMATE only; the user has no
    /// vat sensor. Re-calibrate when one is added.
    #[serde(default = "default_led_to_vat_coupling")]
    pub(crate) led_to_vat_coupling: f32,

    /// Hardware build-envelope dimensions (ADR-0012). Optional so a profile
    /// can ship before its dimensions are confirmed (Athena II in v1).
    /// Consumed by `resinsim-viz` to size the build plate and frame the
    /// camera; `None` triggers a viz-side fallback to CTB header
    /// bed_size_mm + sentinel max_z + a one-shot warn.
    #[serde(default)]
    pub(crate) build_envelope_mm: Option<BuildEnvelope>,
}

impl PrinterProfile {
    /// Printer profile identity (used for display + matching by name).
    pub fn name(&self) -> &str {
        &self.name
    }

    // --- Public read-only accessors (pub(crate) fields per validate-on-mutation contract) ---

    pub fn z_stiffness_n_per_mm(&self) -> f32 {
        self.z_stiffness_n_per_mm
    }
    pub fn delta_t_steady_c(&self) -> f32 {
        self.delta_t_steady_c
    }
    pub fn thermal_tau_sec(&self) -> f32 {
        self.thermal_tau_sec
    }
    pub fn led_power_mw_cm2(&self) -> f32 {
        self.led_power_mw_cm2
    }
    pub fn pixel_pitch_um(&self) -> f32 {
        self.pixel_pitch_um
    }
    pub fn lcd_uniformity_variation(&self) -> f32 {
        self.lcd_uniformity_variation
    }
    pub fn layer_height_range_um(&self) -> FloatRange {
        self.layer_height_range_um
    }
    pub fn exposure_range_sec(&self) -> FloatRange {
        self.exposure_range_sec
    }
    pub fn lift_speed_range_mm_min(&self) -> FloatRange {
        self.lift_speed_range_mm_min
    }
    pub fn bottom_layer_count_max(&self) -> u32 {
        self.bottom_layer_count_max
    }
    pub fn voxel_size_mm(&self) -> f32 {
        self.voxel_size_mm
    }
    pub fn release_mechanism(&self) -> ReleaseMechanism {
        self.release_mechanism
    }
    pub fn led_delta_t_steady_c(&self) -> f32 {
        self.led_delta_t_steady_c
    }
    pub fn led_tau_sec(&self) -> f32 {
        self.led_tau_sec
    }
    pub fn led_to_vat_coupling(&self) -> f32 {
        self.led_to_vat_coupling
    }
    pub fn build_envelope_mm(&self) -> Option<BuildEnvelope> {
        self.build_envelope_mm
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
            (self.z_stiffness_n_per_mm, "z_stiffness_n_per_mm"),
            (self.thermal_tau_sec, "thermal_tau_sec"),
        ];
        for (val, field) in positive_checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        self.layer_height_range_um
            .validate()
            .map_err(|e| format!("layer_height_range_um: {e}"))?;
        self.exposure_range_sec
            .validate()
            .map_err(|e| format!("exposure_range_sec: {e}"))?;
        self.lift_speed_range_mm_min
            .validate()
            .map_err(|e| format!("lift_speed_range_mm_min: {e}"))?;
        if self.bottom_layer_count_max == 0 {
            return Err("bottom_layer_count_max must be >= 1".into());
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
        if !self.voxel_size_mm.is_finite() || self.voxel_size_mm <= 0.0 {
            return Err(format!(
                "voxel_size_mm must be finite and > 0 (got {})",
                self.voxel_size_mm
            ));
        }
        if !self.led_delta_t_steady_c.is_finite() {
            return Err(format!(
                "led_delta_t_steady_c must be finite (got {})",
                self.led_delta_t_steady_c
            ));
        }
        if !self.led_tau_sec.is_finite() || self.led_tau_sec <= 0.0 {
            return Err(format!(
                "led_tau_sec must be finite and > 0 (got {})",
                self.led_tau_sec
            ));
        }
        if !self.led_to_vat_coupling.is_finite()
            || !(0.0..=1.0).contains(&self.led_to_vat_coupling)
        {
            return Err(format!(
                "led_to_vat_coupling must be in [0.0, 1.0] (got {})",
                self.led_to_vat_coupling
            ));
        }
        if let Some(env) = self.build_envelope_mm.as_ref() {
            env.validate()?;
        }
        Ok(())
    }

    /// Elegoo Mars 5 Ultra — 12K mono LCD, ParaLED, triple linear rail.
    /// Sources: Elegoo published specs; ranges derived from published hardware envelope
    /// (KB-131 for Z stiffness, Elegoo spec sheet for layer/exposure/speed bounds).
    /// z_stiffness estimated from triple-rail geometry (calibrate with Athena II data).
    pub fn elegoo_mars5_ultra() -> Self {
        Self {
            name: "Elegoo Mars 5 Ultra".into(),
            led_power_mw_cm2: 20.0, // ParaLED matrix, 12K mono LCD
            pixel_pitch_um: 19.0,   // 218.88 mm / 11520 px
            layer_height_range_um: FloatRange::new(10.0, 150.0)
                .expect("Mars 5 Ultra layer-height range 10..150 µm is valid"),
            exposure_range_sec: FloatRange::new(0.5, 60.0)
                .expect("Mars 5 Ultra exposure range 0.5..60 sec is valid"),
            lift_speed_range_mm_min: FloatRange::new(10.0, 300.0)
                .expect("Mars 5 Ultra lift-speed range 10..300 mm/min is valid"),
            bottom_layer_count_max: 20,
            z_stiffness_n_per_mm: 900.0, // estimated — triple linear rail
            delta_t_steady_c: 8.0,
            thermal_tau_sec: 1200.0,
            lcd_uniformity_variation: 0.12, // ParaLED, better than Saturn-class
            voxel_size_mm: DEFAULT_VOXEL_SIZE_MM,
            release_mechanism: ReleaseMechanism::Tilt, // ADR-0007: Mars 5 Ultra uses tilting vat release
            // LED thermal (KB-152) — fitted from data/elegoo/ overnight session.
            led_delta_t_steady_c: 13.5, // plateau 40.5 − idle 27
            led_tau_sec: 4000.0,        // 3–4 h to 95 % of plateau (3τ ≈ 3–4 h)
            led_to_vat_coupling: 0.71,  // user estimate — no vat sensor; recalibrate when added
            // Build envelope (ADR-0012). Elegoo published spec.
            build_envelope_mm: Some(BuildEnvelope {
                width_mm: 153.36,
                depth_mm: 77.76,
                max_z_mm: 165.0,
            }),
        }
    }

    /// Generic MSLA 4K printer with conservative defaults.
    pub fn generic_msla_4k() -> Self {
        Self {
            name: "Generic MSLA 4K".into(),
            led_power_mw_cm2: 4.0, // KB-121: typical LCD printer
            pixel_pitch_um: 50.0,
            layer_height_range_um: FloatRange::new(20.0, 100.0)
                .expect("generic MSLA 4K layer-height range 20..100 µm is valid"),
            exposure_range_sec: FloatRange::new(1.0, 60.0)
                .expect("generic MSLA 4K exposure range 1..60 sec is valid"),
            lift_speed_range_mm_min: FloatRange::new(10.0, 200.0)
                .expect("generic MSLA 4K lift-speed range 10..200 mm/min is valid"),
            bottom_layer_count_max: 15,
            z_stiffness_n_per_mm: 460.0,    // KB-131: Elegoo Mars class
            delta_t_steady_c: 10.0,         // KB-150: estimate
            thermal_tau_sec: 1200.0,        // KB-183: estimate
            lcd_uniformity_variation: 0.22, // KB-120: Saturn 2 class
            voxel_size_mm: DEFAULT_VOXEL_SIZE_MM,
            release_mechanism: ReleaseMechanism::Linear, // classic MSLA arm-lift
            // LED thermal — conservative defaults until per-printer calibration exists.
            led_delta_t_steady_c: 10.0,
            led_tau_sec: 1200.0,
            led_to_vat_coupling: 0.5,
            // Build envelope (ADR-0012). Typical 8.9" / 4K monoLCD class.
            build_envelope_mm: Some(BuildEnvelope {
                width_mm: 192.0,
                depth_mm: 120.0,
                max_z_mm: 200.0,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_msla_4k_passes_validation() {
        PrinterProfile::generic_msla_4k()
            .validate()
            .expect("PrinterProfile::generic_msla_4k() factory must satisfy validate()");
    }

    #[test]
    fn elegoo_mars5_ultra_passes_validation() {
        PrinterProfile::elegoo_mars5_ultra()
            .validate()
            .expect("PrinterProfile::elegoo_mars5_ultra() factory must satisfy validate()");
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

    #[test]
    fn zero_bottom_layer_count_max_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.bottom_layer_count_max = 0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn inverted_layer_height_range_rejected_via_validate() {
        let mut p = PrinterProfile::generic_msla_4k();
        // Directly mutate pub(crate) field inside the range to produce a min > max shape.
        // Bypasses FloatRange::new, reaches PrinterProfile::validate() delegating through.
        p.layer_height_range_um.min = 200.0;
        p.layer_height_range_um.max = 20.0;
        let err = p.validate().expect_err("inverted range must fail validate");
        assert!(
            err.contains("layer_height_range_um") && err.contains("min"),
            "error names the offending range: {err}"
        );
    }

    // Contract demonstration — see PrinterProfile struct doc comment.
    // Distinct from numeric-range tests because this exercises the "previously-VALID →
    // mutated → invalid" sequence on a string field, demonstrating the contract requires
    // re-running validate() after intra-crate mutation.
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

    // --- Parse-path (serde) tests locking that NaN bounds are caught by validate(). ---

    fn valid_printer_toml() -> String {
        r#"
name = "Test Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = { min = 20.0, max = 100.0 }
exposure_range_sec = { min = 1.0, max = 60.0 }
lift_speed_range_mm_min = { min = 10.0, max = 200.0 }
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
"#
        .to_string()
    }

    #[test]
    fn parse_toml_then_validate_accepts_valid() {
        let p: PrinterProfile =
            toml::from_str(&valid_printer_toml()).expect("valid printer TOML must parse");
        p.validate().expect("valid TOML must satisfy validate()");
    }

    #[test]
    fn parse_toml_with_nan_range_bound_rejected() {
        let toml_str = valid_printer_toml().replace(
            "layer_height_range_um = { min = 20.0, max = 100.0 }",
            "layer_height_range_um = { min = nan, max = 100.0 }",
        );
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("NaN range bound must fail validate()");
        assert!(
            err.contains("layer_height_range_um"),
            "error names the range: {err}"
        );
    }

    // --- ReleaseMechanism (ADR-0007) tests ---

    #[test]
    fn mars5_ultra_factory_is_tilt() {
        assert_eq!(
            PrinterProfile::elegoo_mars5_ultra().release_mechanism(),
            ReleaseMechanism::Tilt,
        );
    }

    #[test]
    fn generic_msla_4k_factory_is_linear() {
        assert_eq!(
            PrinterProfile::generic_msla_4k().release_mechanism(),
            ReleaseMechanism::Linear,
        );
    }

    #[test]
    fn legacy_toml_without_release_mechanism_defaults_to_linear() {
        // Legacy TOML profiles (athena_ii, generic_msla_4k written before ADR-0007)
        // must deserialise with release_mechanism = Linear via #[serde(default)].
        let p: PrinterProfile = toml::from_str(&valid_printer_toml())
            .expect("valid printer TOML without release_mechanism must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Linear);
    }

    #[test]
    fn toml_with_explicit_tilt_deserialises() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"tilt\"\n";
        let p: PrinterProfile = toml::from_str(&toml_str).expect("explicit tilt must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Tilt);
    }

    #[test]
    fn toml_with_explicit_linear_deserialises() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"linear\"\n";
        let p: PrinterProfile = toml::from_str(&toml_str).expect("explicit linear must parse");
        assert_eq!(p.release_mechanism(), ReleaseMechanism::Linear);
    }

    // --- LED thermal fields (KB-152) tests ---

    #[test]
    fn mars5_ultra_led_thermal_fitted_values() {
        let p = PrinterProfile::elegoo_mars5_ultra();
        assert!((p.led_delta_t_steady_c() - 13.5).abs() < 1e-4);
        assert!((p.led_tau_sec() - 4000.0).abs() < 1e-4);
        assert!((p.led_to_vat_coupling() - 0.71).abs() < 1e-4);
    }

    #[test]
    fn legacy_toml_gets_conservative_led_defaults() {
        // Valid TOML without led_* fields → serde defaults kick in (10.0, 1200, 0.5).
        let p: PrinterProfile = toml::from_str(&valid_printer_toml())
            .expect("legacy printer TOML without led_* fields must parse");
        assert_eq!(p.led_delta_t_steady_c(), 10.0);
        assert_eq!(p.led_tau_sec(), 1200.0);
        assert_eq!(p.led_to_vat_coupling(), 0.5);
        p.validate().expect("legacy defaults must satisfy validate()");
    }

    #[test]
    fn led_tau_rejects_zero() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_tau_rejects_negative() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = -1.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_tau_rejects_nan() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_tau_sec = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_rejects_above_one() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = 1.5;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_rejects_negative() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = -0.1;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_coupling_boundary_zero_and_one_accepted() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_to_vat_coupling = 0.0;
        assert!(p.validate().is_ok());
        p.led_to_vat_coupling = 1.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn led_delta_t_rejects_nan() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_delta_t_steady_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn led_delta_t_accepts_zero() {
        // Zero rise is physically valid (perfectly dissipating printer).
        let mut p = PrinterProfile::generic_msla_4k();
        p.led_delta_t_steady_c = 0.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn toml_with_unknown_release_mechanism_rejected_at_parse() {
        let toml_str = valid_printer_toml() + "release_mechanism = \"bogus\"\n";
        assert!(
            toml::from_str::<PrinterProfile>(&toml_str).is_err(),
            "unknown enum variant must fail parse (not validate)"
        );
    }

    // --- BuildEnvelope (ADR-0012) tests ---

    #[test]
    fn mars5_ultra_factory_populates_build_envelope() {
        let env = PrinterProfile::elegoo_mars5_ultra()
            .build_envelope_mm()
            .expect("Mars 5 Ultra factory must populate build_envelope_mm");
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
    }

    #[test]
    fn generic_msla_4k_factory_populates_build_envelope() {
        let env = PrinterProfile::generic_msla_4k()
            .build_envelope_mm()
            .expect("generic_msla_4k factory must populate build_envelope_mm");
        assert!((env.width_mm - 192.0).abs() < 1e-4);
        assert!((env.depth_mm - 120.0).abs() < 1e-4);
        assert!((env.max_z_mm - 200.0).abs() < 1e-4);
    }

    #[test]
    fn legacy_toml_without_build_envelope_defaults_to_none() {
        // Legacy TOML profiles (athena_ii in v1) deserialise with
        // build_envelope_mm = None via #[serde(default)]. validate() must
        // accept None — the field is Optional per ADR-0012.
        let p: PrinterProfile = toml::from_str(&valid_printer_toml())
            .expect("valid printer TOML without build_envelope_mm must parse");
        assert!(p.build_envelope_mm().is_none());
        p.validate()
            .expect("None build_envelope must satisfy validate() (Option per ADR-0012)");
    }

    #[test]
    fn toml_with_explicit_build_envelope_round_trips() {
        let toml_str = valid_printer_toml()
            + "\n[build_envelope_mm]\nwidth_mm = 153.36\ndepth_mm = 77.76\nmax_z_mm = 165.0\n";
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("explicit build_envelope must parse");
        let env = p
            .build_envelope_mm()
            .expect("Some build_envelope after explicit TOML");
        assert!((env.width_mm - 153.36).abs() < 1e-4);
        assert!((env.depth_mm - 77.76).abs() < 1e-4);
        assert!((env.max_z_mm - 165.0).abs() < 1e-4);
    }

    #[test]
    fn build_envelope_with_zero_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 0.0,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        });
        let err = p
            .validate()
            .expect_err("zero width_mm must fail validate()");
        assert!(
            err.contains("build_envelope_mm.width_mm"),
            "error names the offending field: {err}"
        );
    }

    #[test]
    fn build_envelope_with_negative_max_z_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 192.0,
            depth_mm: 120.0,
            max_z_mm: -1.0,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn build_envelope_with_nan_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: f32::NAN,
            depth_mm: 120.0,
            max_z_mm: 200.0,
        });
        let err = p
            .validate()
            .expect_err("NaN width_mm must fail validate()");
        assert!(
            err.contains("build_envelope_mm.width_mm"),
            "error names the offending field: {err}"
        );
    }

    #[test]
    fn build_envelope_with_infinite_extent_rejected() {
        let mut p = PrinterProfile::generic_msla_4k();
        p.build_envelope_mm = Some(BuildEnvelope {
            width_mm: 192.0,
            depth_mm: f32::INFINITY,
            max_z_mm: 200.0,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn parse_toml_with_min_greater_than_max_rejected() {
        let toml_str = valid_printer_toml().replace(
            "exposure_range_sec = { min = 1.0, max = 60.0 }",
            "exposure_range_sec = { min = 60.0, max = 1.0 }",
        );
        let p: PrinterProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("inverted range bounds must fail validate()");
        assert!(
            err.contains("exposure_range_sec"),
            "error names the range: {err}"
        );
    }
}
