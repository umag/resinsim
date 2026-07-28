//! Test fixtures shared across UAT step-def modules.
//!
//! `default_plate` / `test_ambient` / `test_supports` / `cube_areas` are
//! small closures duplicated from simulation_runner.rs's `#[cfg(test)]`
//! block (not re-exported as `pub`) — no builder covers them.
//! `printer_with_ranges` now delegates to `world::PrinterBuilder`.
//! The ADR-0020 field-sim TOML fragments below
//! (`RESIN_FIELD_SIM_THERMAL_LINES`, `PRINTER_FIELD_SIM_SCALARS`,
//! `PRINTER_BUILD_ENVELOPE_INLINE`, `resin_chemistry_root*`,
//! `valid_recipe_table`) are the single home every UAT resin/printer
//! fixture composes from.

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

// ---- ADR-0020 / t2f4 field-sim deltas (single home) ------------------------
//
// t2f4 (286b0af, 2026-05-21) made three resin scalars and four printer
// scalars/tables required under the `field-sim` feature. Every hand-rolled
// UAT fixture must compose from these fragments rather than hand-copy the
// values — see docs/patterns/anti/fixture-copy-of-shared-builder.md, written
// after the identical defect rotted a unit-test fixture in
// resin_profile.rs (`s3-peel-shape-toml-fieldsim-thermal`, c0256a1).

/// ADR-0020 resin thermal-material scalars (KB-152 literature midpoints for
/// acrylate photopolymer). Root-level TOML lines — must land BEFORE any
/// `[recipe]` table or they silently nest into it
/// (docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
pub const RESIN_FIELD_SIM_THERMAL_LINES: &str = "thermal_conductivity_w_mk = 0.20\n\
     specific_heat_j_kgk = 1700.0\n\
     convective_top_h_w_m2k = 10.0\n";

/// ADR-0020 printer vat-wall + convective-BC scalars. Values mirror
/// `PrinterProfile::generic_msla_4k()` (still-air natural convection
/// ~8 W/m²·K, ~2.0 mm Al-alloy vat wall, ~200 W/m·K Al alloy conductivity).
/// Root-level TOML lines — same before-any-table ordering constraint as
/// `RESIN_FIELD_SIM_THERMAL_LINES`.
pub const PRINTER_FIELD_SIM_SCALARS: &str = "convective_wall_h_w_m2k = 8.0\n\
     vat_wall_thickness_mm = 2.0\n\
     vat_wall_k_w_mk = 200.0\n";

/// ADR-0020 `build_envelope_mm`, required under `field-sim`. INLINE table —
/// deliberately NOT a `[build_envelope_mm]` header block, so a scalar
/// appended after it (by a future fixture edit) cannot silently nest inside
/// it and be dropped
/// (docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
/// Extents mirror `generic_msla_4k` and clear `2 x THERMAL_VOXEL_MIN_MM` on
/// every axis.
pub const PRINTER_BUILD_ENVELOPE_INLINE: &str =
    "build_envelope_mm = { width_mm = 192.0, depth_mm = 120.0, max_z_mm = 200.0 }\n";

/// Resin chemistry root fields ONLY — pre-ADR-0020 shape, no thermal-material
/// scalars, no `[recipe]` table. Single home for the chemistry field list
/// (docs/patterns/anti/fixture-copy-of-shared-builder.md) so a future
/// required chemistry field is added in exactly one place. Only fixtures that
/// deliberately exercise the pre-t2f4 / parse-failure shape should call this
/// directly — everything else wants `resin_chemistry_root`.
pub fn resin_chemistry_root_pre_t2f4(name: &str) -> String {
    format!(
        r#"name = "{name}"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
"#
    )
}

/// `resin_chemistry_root_pre_t2f4` plus the ADR-0020 thermal-material
/// scalars — the field-sim-complete resin root every UAT fixture that
/// reaches `validate()` should compose from.
pub fn resin_chemistry_root(name: &str) -> String {
    format!(
        "{}{RESIN_FIELD_SIM_THERMAL_LINES}",
        resin_chemistry_root_pre_t2f4(name)
    )
}

/// The canonical `[recipe]` block — matches `generic_standard`'s recipe
/// (layer_height_um=50, normal_exposure=2.5, bottom_exposure=25,
/// bottom_layer_count=6, transition_layers=3, lift_speed=60, ...).
pub fn valid_recipe_table() -> &'static str {
    r#"[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#
}

/// Build a narrowed-range `PrinterProfile` — thin delegate to
/// `world::PrinterBuilder`. Every other field (name aside) tracks
/// `PrinterBuilder::new()`'s defaults, which were verified field-by-field
/// to equal this function's former hand-rolled literal (led_power 4.0,
/// pixel_pitch 50, lift-speed 10..200, bottom_layer_count_max 15,
/// z_stiffness 460, delta_t_steady 10, thermal_tau 1200, lcd_uniformity
/// 0.22) — a before/after TOML diff showed the only change is the four
/// ADR-0020 lines PrinterBuilder now appends
/// (docs/patterns/anti/fixture-copy-of-shared-builder.md: "split the
/// builder, don't fork it").
pub fn printer_with_ranges(
    layer_min: f32,
    layer_max: f32,
    exposure_min: f32,
    exposure_max: f32,
) -> PrinterProfile {
    super::world::PrinterBuilder::new()
        .with_name("UatNarrowed")
        .with_layer_height_range(layer_min, layer_max)
        .with_exposure_range(exposure_min, exposure_max)
        .build()
}
