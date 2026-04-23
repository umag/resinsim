//! Step definitions for `spec/uat/legacy-resin-toml-without-recipe.md`
//! UAT-1 + UAT-2.
//!
//! ADR-0005 moves recipe fields onto a required `[recipe]` table on
//! `ResinProfile`. Legacy TOMLs without it must fail at DESERIALIZE (no
//! silent default); `[recipe]` with NaN fields must fail at VALIDATE.

use cucumber::{given, then, when};
use resinsim_core::entities::ResinProfile;

use super::world::UatWorld;

fn legacy_toml_sans_recipe() -> String {
    // All chemistry fields present, NO [recipe] table.
    r#"name = "LegacySansRecipe"
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
    .to_string()
}

fn toml_with_nan_recipe_exposure() -> String {
    r#"name = "NaNExposure"
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

[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = nan
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

// ---- UAT-1: missing [recipe] rejected at deserialize -----------------------

#[given(
    regex = r"^a pre-ADR-0005 resin TOML file containing all chemistry fields .* but NO \[recipe\] table$"
)]
fn given_toml_without_recipe(world: &mut UatWorld) {
    world.toml_text = Some(legacy_toml_sans_recipe());
}

#[when(regex = r#"^"toml::from_str::<ResinProfile>\(&contents\)" is called$"#)]
fn when_toml_from_str_called(world: &mut UatWorld) {
    let toml_str = world
        .toml_text
        .as_ref()
        .expect("scenario invariant: Given step set toml_text");
    let parsed: Result<ResinProfile, toml::de::Error> = toml::from_str(toml_str);
    world.parse_result = Some(match parsed {
        Ok(r) => {
            world.resin = Some(r);
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    });
}

#[then(regex = r"^parse returns Err$")]
fn then_parse_returns_err(world: &mut UatWorld) {
    let res = world
        .parse_result
        .as_ref()
        .expect("scenario invariant: When step set parse_result");
    assert!(res.is_err(), "parse must return Err; was Ok");
}

#[then(regex = r#"^the error message names the missing field \("recipe"\)$"#)]
fn then_err_names_recipe(world: &mut UatWorld) {
    let err = world
        .parse_result
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .expect("parse_result must carry an Err");
    assert!(
        err.contains("recipe"),
        "err must name 'recipe' as missing: {err}",
    );
}

#[then(regex = r"^validate\(\) is NEVER reached — the parse layer is the gate$")]
fn then_validate_never_reached(world: &mut UatWorld) {
    assert!(
        world.resin.is_none(),
        "parse failed — no ResinProfile reached validate(); resin: {:?}",
        world.resin,
    );
}

// ---- UAT-2: [recipe] with NaN field rejected at validate() -----------------

#[given(
    regex = r"^a resin TOML file with a full \[recipe\] table but with recipe\.normal_exposure_sec = nan$"
)]
fn given_toml_with_nan_recipe(world: &mut UatWorld) {
    world.toml_text = Some(toml_with_nan_recipe_exposure());
}

#[when(regex = r"^the file is deserialised into ResinProfile then validate\(\) is called$")]
fn when_deser_then_validate(world: &mut UatWorld) {
    let toml_str = world
        .toml_text
        .as_ref()
        .expect("scenario invariant: Given step set toml_text");
    let parsed: Result<ResinProfile, toml::de::Error> = toml::from_str(toml_str);
    match parsed {
        Ok(resin) => {
            world.parse_result = Some(Ok(()));
            world.validate_result = Some(resin.validate().map_err(|e| e.to_string()));
            world.resin = Some(resin);
        }
        Err(e) => {
            world.parse_result = Some(Err(e.to_string()));
        }
    }
}

#[then(regex = r"^deserialize succeeds \(serde accepts NaN as a valid f32\)$")]
fn then_deserialize_succeeds(world: &mut UatWorld) {
    let res = world
        .parse_result
        .as_ref()
        .expect("scenario invariant: When step set parse_result");
    assert!(
        res.is_ok(),
        "serde must accept NaN at parse time; got Err: {res:?}",
    );
}

#[then(regex = r"^validate\(\) returns Err$")]
fn then_validate_returns_err(world: &mut UatWorld) {
    let res = world
        .validate_result
        .as_ref()
        .expect("scenario invariant: When step set validate_result");
    assert!(
        res.is_err(),
        "validate() must reject NaN recipe field; was Ok",
    );
}

#[then(
    regex = r#"^the error message prefix is "recipe:" \(because ResinProfile::validate\(\) delegates to Recipe::validate\(\) and tags the error\)$"#
)]
fn then_err_prefixed_recipe(world: &mut UatWorld) {
    let err = world
        .validate_result
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .expect("validate_result must carry an Err");
    assert!(
        err.starts_with("recipe:"),
        "err must be prefixed with 'recipe:'; got: {err}",
    );
}

#[then(regex = r#"^the error names "normal_exposure_sec" as the offending field$"#)]
fn then_err_names_exposure(world: &mut UatWorld) {
    let err = world
        .validate_result
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .expect("validate_result must carry an Err");
    assert!(
        err.contains("normal_exposure_sec"),
        "err must name 'normal_exposure_sec': {err}",
    );
}
