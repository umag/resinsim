//! Test fixtures shared across UAT step-def modules.
//!
//! Helpers duplicated from simulation_runner.rs's `#[cfg(test)]` block
//! (not re-exported as `pub`). Step 7 of the rollout replaces these
//! with explicit builders (`PrinterBuilder`, `ResinBuilder`, etc.) in
//! `world.rs` — for now, inline closures + TOML round-trips suffice.

use resinsim_core::entities::PrinterProfile;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

pub fn default_plate() -> PlateAdhesionProfile {
    PlateAdhesionProfile::default_textured()
}

pub fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22.0 °C is in AmbientTemperature domain")
}

pub fn test_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 10,
    }
}

pub fn cube_areas(n_layers: usize, area_mm2: f64) -> Vec<CrossSectionArea> {
    let a = CrossSectionArea::new(area_mm2).expect("cube area is non-negative and finite");
    vec![a; n_layers]
}

/// Build a `PrinterProfile` via TOML round-trip — lets integration tests
/// override `pub(crate)` range fields without piercing the visibility.
/// Other fields match `PrinterProfile::generic_msla_4k()` defaults.
pub fn printer_with_ranges(
    layer_min: f32,
    layer_max: f32,
    exposure_min: f32,
    exposure_max: f32,
) -> PrinterProfile {
    let toml_str = format!(
        r#"
name = "UatNarrowed"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = {{ min = {layer_min}, max = {layer_max} }}
exposure_range_sec = {{ min = {exposure_min}, max = {exposure_max} }}
lift_speed_range_mm_min = {{ min = 10.0, max = 200.0 }}
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
"#
    );
    let p: PrinterProfile =
        toml::from_str(&toml_str).expect("narrowed printer TOML parses into PrinterProfile");
    p.validate()
        .expect("narrowed printer satisfies PrinterProfile::validate()");
    p
}
