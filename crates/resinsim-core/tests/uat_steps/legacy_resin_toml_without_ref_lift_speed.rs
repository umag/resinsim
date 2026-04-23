//! Step definitions for
//! `spec/uat/legacy-resin-toml-without-ref-lift-speed.md` UAT-1 + UAT-2.
//!
//! ADR-0005 §3 moved `ref_lift_speed_mm_min` from `PrinterProfile` onto
//! `ResinProfile`. Pre-ADR-0005 TOMLs without it must fail at parse
//! (no serde default); adding the field yields a valid profile.

use cucumber::{given, then, when};

use super::world::UatWorld;

fn toml_without_ref_lift_speed() -> String {
    // Every REQUIRED chemistry field present EXCEPT ref_lift_speed_mm_min.
    // Includes a valid [recipe] table so this specifically isolates the
    // ref_lift_speed_mm_min failure mode (not the [recipe] case).
    r#"name = "SansRefLiftSpeed"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1

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
    .to_string()
}

// ---- UAT-1: missing ref_lift_speed_mm_min rejected at parse ----------------

#[given(
    regex = r"^a pre-ADR-0005 resin TOML containing name, penetration_depth_um, critical_energy_mj_cm2, tensile_strength_mpa, peel_adhesion_kpa, viscosity_mpa_s, reference_temp_c, activation_energy_kj_mol, density_g_cm3, linear_shrinkage_pct$"
)]
fn given_toml_sans_ref_lift_speed(world: &mut UatWorld) {
    world.toml_text = Some(toml_without_ref_lift_speed());
}

#[given(regex = r"^a valid \[recipe\] table \(isolating the ref_lift_speed_mm_min failure mode\)$")]
fn given_valid_recipe_table(_world: &mut UatWorld) {
    // The Given chain is declarative; the helper toml_without_ref_lift_speed
    // already includes the valid [recipe] table. This step pins that the
    // recipe is deliberately present so a future change that also drops
    // the recipe would fail the wrong-reason assertion upstream.
}

#[given(regex = r"^NO ref_lift_speed_mm_min field$")]
fn given_no_ref_lift_field(world: &mut UatWorld) {
    let toml = world
        .toml_text
        .as_ref()
        .expect("scenario invariant: toml_text set by earlier Given");
    assert!(
        !toml.contains("ref_lift_speed_mm_min"),
        "fixture must not contain ref_lift_speed_mm_min; got {toml}",
    );
}

// NOTE: the `"toml::from_str::<ResinProfile>(&contents)" is called` When
// step regex is registered ONCE in legacy_resin_toml_without_recipe.rs.
// Scenarios here reuse that same step def via cucumber's global step
// registry; a duplicate registration here would cause ambiguity.

// `parse returns Err` Then step registered in
// legacy_resin_toml_without_recipe.rs; UAT-1 here reuses it.

#[then(regex = r#"^the error message names the missing "ref_lift_speed_mm_min" field$"#)]
fn then_err_names_ref_lift(world: &mut UatWorld) {
    let err = world
        .parse_result
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .expect("parse_result must carry an Err");
    assert!(
        err.contains("ref_lift_speed_mm_min"),
        "err must name 'ref_lift_speed_mm_min': {err}",
    );
}

#[then(regex = r"^validate\(\) is never reached \(parse is the gate\)$")]
fn then_validate_never_reached(world: &mut UatWorld) {
    assert!(
        world.resin.is_none(),
        "parse failed — no ResinProfile reached validate()",
    );
}

// ---- UAT-2: migration patch adds ref_lift_speed_mm_min = 60.0 --------------

#[given(regex = r"^the same pre-ADR-0005 resin TOML from UAT-1$")]
fn given_same_legacy_toml(world: &mut UatWorld) {
    world.toml_text = Some(toml_without_ref_lift_speed());
}

#[when(
    regex = r#"^a user appends "ref_lift_speed_mm_min = 60\.0" to the chemistry section \(the industry-standard default per KB-112\)$"#
)]
fn when_user_appends_ref_lift_speed(world: &mut UatWorld) {
    let orig = world
        .toml_text
        .as_ref()
        .expect("scenario invariant: Given step set toml_text");
    // Insert at the top of the file so it lands before the [recipe] table.
    let patched = format!("ref_lift_speed_mm_min = 60.0\n{orig}");
    world.toml_text = Some(patched);
}

// `"toml::from_str::<ResinProfile>(&contents)" is called` registered in
// legacy_resin_toml_without_recipe.rs; UAT-2 migration-patch scenario
// reuses the same step def.

#[then(regex = r"^parse returns Ok\(profile\)$")]
fn then_parse_ok(world: &mut UatWorld) {
    let res = world
        .parse_result
        .as_ref()
        .expect("scenario invariant: When step set parse_result");
    assert!(res.is_ok(), "parse must return Ok after migration patch; got {res:?}");
}

#[then(regex = r"^profile\.validate\(\) returns Ok$")]
fn then_validate_ok(world: &mut UatWorld) {
    let r = world
        .resin
        .as_ref()
        .expect("scenario invariant: parse produced a resin");
    r.validate().expect("migration-patched TOML must satisfy validate()");
}

#[then(regex = r"^profile\.ref_lift_speed_mm_min\(\) == 60\.0$")]
fn then_ref_lift_speed_60(world: &mut UatWorld) {
    let r = world
        .resin
        .as_ref()
        .expect("scenario invariant: parse produced a resin");
    assert!(
        (r.ref_lift_speed_mm_min() - 60.0).abs() < 1e-3,
        "expected 60.0; got {}",
        r.ref_lift_speed_mm_min(),
    );
}
