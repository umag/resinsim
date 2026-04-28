//! Step definitions for `spec/uat/legacy-resin-toml-defaults.md` UAT-1 + UAT-2.
//!
//! T1-F6 locks the KB-150 serde-default contract: a legacy resin TOML
//! missing `degradation_temp_c` and/or `min_safe_temp_c` deserialises
//! with the module defaults (50.0 / 15.0), but combinations that cross
//! the strict-less-than ordering invariant must fail `validate()`.

use cucumber::{given, then, when};
use resinsim_core::entities::ResinProfile;

use super::world::UatWorld;

// Minimal legacy-resin TOML with a full `[recipe]` table — isolates the
// thermal-threshold defaulting behaviour. Other required fields are set
// to values inside their validate() domains.
fn legacy_resin_toml(explicit_min_safe: Option<f32>) -> String {
    let min_line = match explicit_min_safe {
        Some(v) => format!("min_safe_temp_c = {v}\n"),
        None => String::new(),
    };
    format!(
        r#"name = "LegacyResin"
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
{min_line}
[recipe]
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
    )
}

// ---- UAT-1: missing thermal thresholds apply documented defaults -----------

#[given(
    regex = r#"^a resin TOML file written before KB-150 that is missing both "degradation_temp_c" and "min_safe_temp_c"$"#
)]
fn given_legacy_toml_missing_both(world: &mut UatWorld) {
    world.toml_text = Some(legacy_resin_toml(None));
}

#[when(regex = r"^the resin profile is deserialised and validated$")]
fn when_deserialise_and_validate(world: &mut UatWorld) {
    let toml_str = world
        .toml_text
        .as_ref()
        .expect("scenario invariant: Given step set toml_text");
    let parsed: Result<ResinProfile, _> = toml::from_str(toml_str);
    match parsed {
        Ok(resin) => {
            world.validate_result = Some(resin.validate().map_err(|e| e.to_string()));
            world.resin = Some(resin);
        }
        Err(e) => {
            // For UAT-1/2 of defaults, parsing must succeed.
            world.validate_result = Some(Err(format!("parse: {e}")));
        }
    }
}

#[then(regex = r#"^"degradation_temp_c" takes the documented default of 50\.0 °C$"#)]
fn then_degradation_default_50(world: &mut UatWorld) {
    let r = world
        .resin
        .as_ref()
        .expect("scenario invariant: parse produced a resin");
    assert!(
        (r.degradation_temp_c() - 50.0).abs() < 1e-3,
        "expected degradation_temp_c default 50.0; got {}",
        r.degradation_temp_c(),
    );
}

#[then(regex = r#"^"min_safe_temp_c" takes the documented default of 15\.0 °C$"#)]
fn then_min_safe_default_15(world: &mut UatWorld) {
    let r = world
        .resin
        .as_ref()
        .expect("scenario invariant: parse produced a resin");
    assert!(
        (r.min_safe_temp_c() - 15.0).abs() < 1e-3,
        "expected min_safe_temp_c default 15.0; got {}",
        r.min_safe_temp_c(),
    );
}

#[then(
    regex = r"^validate\(\) returns Ok because the defaulted pair satisfies min_safe_temp_c < degradation_temp_c$"
)]
fn then_validate_ok(world: &mut UatWorld) {
    let res = world
        .validate_result
        .as_ref()
        .expect("scenario invariant: When step set validate_result");
    assert!(
        res.is_ok(),
        "validate() must be Ok when both defaults apply; got {res:?}",
    );
}

// ---- UAT-2: invariant-crossing via serde default ---------------------------

#[given(
    regex = r#"^a resin TOML file with "min_safe_temp_c" explicitly set to 55\.0 °C but with "degradation_temp_c" absent$"#
)]
fn given_legacy_toml_explicit_min_only(world: &mut UatWorld) {
    world.toml_text = Some(legacy_resin_toml(Some(55.0)));
}

#[then(regex = r#"^serde applies the default "degradation_temp_c" of 50\.0 °C$"#)]
fn then_serde_applies_50_default(world: &mut UatWorld) {
    let r = world
        .resin
        .as_ref()
        .expect("scenario invariant: parse produced a resin");
    assert!(
        (r.degradation_temp_c() - 50.0).abs() < 1e-3,
        "serde should apply default 50.0; got {}",
        r.degradation_temp_c(),
    );
}

#[then(
    regex = r"^validate\(\) returns Err citing BOTH fields by name because 55\.0 > 50\.0 violates the strict-less-than ordering invariant$"
)]
fn then_validate_err_both_fields(world: &mut UatWorld) {
    let res = world
        .validate_result
        .as_ref()
        .expect("scenario invariant: When step set validate_result");
    let err = res
        .as_ref()
        .err()
        .unwrap_or_else(|| panic!("validate() must be Err when min_safe > degradation; was Ok"));
    assert!(
        err.contains("min_safe_temp_c") && err.contains("degradation_temp_c"),
        "err must cite BOTH fields by name; got: {err}",
    );
}

#[then(regex = r"^the profile is NOT silently accepted with misaligned thermal thresholds$")]
fn then_not_silently_accepted(world: &mut UatWorld) {
    let res = world
        .validate_result
        .as_ref()
        .expect("scenario invariant: When step set validate_result");
    assert!(
        res.is_err(),
        "misaligned thermal thresholds must not be silently accepted",
    );
}
