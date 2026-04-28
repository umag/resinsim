//! Step definitions for `spec/uat/recipe-inside-printer-range.md`
//! UAT-1 + UAT-2.
//!
//! The affirmative counterpart to recipe_out_of_range. Locks that
//! `validate_pairing` accepts in-range recipes (happy path) AND
//! boundary-inclusive values — `value >= range.min && value <= range.max`.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use resinsim_core::app::simulation_runner::SimulationRunner;
use resinsim_core::entities::ResinProfile;
use resinsim_core::services::pairing_validator;

use super::fixtures::{
    cube_areas, default_plate, printer_with_ranges, test_ambient, test_supports,
};
use super::world::UatWorld;

// ---- UAT-1: happy-path pairing ---------------------------------------------

#[given(regex = r#"^a printer profile "P" with:$"#)]
fn given_printer_profile_datatable(world: &mut UatWorld, step: &Step) {
    let table = step
        .table
        .as_ref()
        .expect("scenario invariant: Given step carries a DataTable");
    // Row 0: header "| field | min | max |".
    let mut layer_range = (20.0f32, 100.0f32);
    let mut exposure_range = (1.0f32, 60.0f32);
    for row in table.rows.iter().skip(1) {
        let name = row.first().map(String::as_str).unwrap_or_default();
        let min: f32 = row.get(1).and_then(|s| s.parse().ok()).unwrap_or(f32::NAN);
        let max: f32 = row.get(2).and_then(|s| s.parse().ok()).unwrap_or(f32::NAN);
        match name {
            "layer_height_range_um" => layer_range = (min, max),
            "exposure_range_sec" => exposure_range = (min, max),
            "lift_speed_range_mm_min" => {} // accepted by fixture defaults
            other => panic!("unrecognised printer field in DataTable: '{other}'"),
        }
    }
    world.printer = Some(printer_with_ranges(
        layer_range.0,
        layer_range.1,
        exposure_range.0,
        exposure_range.1,
    ));
}

#[given(regex = r#"^printer "P" has bottom_layer_count_max = 15$"#)]
fn given_bottom_layer_count(_world: &mut UatWorld) {
    // Covered by the TOML helper's fixture default; step is narrative.
}

#[given(regex = r#"^a resin profile "R" whose recipe has:$"#)]
fn given_resin_recipe_datatable(world: &mut UatWorld, step: &Step) {
    // Sanity-assert the declared values match generic_standard's recipe
    // so the narrative stays honest (recipe fields are pub(crate)).
    let table = step
        .table
        .as_ref()
        .expect("scenario invariant: Given step carries a DataTable");
    for row in table.rows.iter().skip(1) {
        let field = row.first().map(String::as_str).unwrap_or_default();
        let declared: f32 = row.get(1).and_then(|s| s.parse().ok()).unwrap_or(f32::NAN);
        let (actual, label) = match field {
            "layer_height_um" => (50.0_f32, "layer_height_um"),
            "normal_exposure_sec" => (2.5_f32, "normal_exposure_sec"),
            "bottom_exposure_sec" => (25.0_f32, "bottom_exposure_sec"),
            "lift_speed_mm_min" => (60.0_f32, "lift_speed_mm_min"),
            "bottom_layer_count" => (6.0_f32, "bottom_layer_count"),
            other => panic!("unrecognised recipe field: '{other}'"),
        };
        assert!(
            (declared - actual).abs() < 1e-3,
            "UAT-1 DataTable declares {label}={declared}; must match generic_standard's {actual}",
        );
    }
    world.resin = Some(ResinProfile::generic_standard());
}

#[when(regex = r"^SimulationRunner\.run_from_areas is invoked on a non-trivial area vector$")]
fn when_run_simulation(world: &mut UatWorld) {
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: Given step set printer");
    let resin = world
        .resin
        .as_ref()
        .expect("scenario invariant: Given step set resin");
    world.pairing_result = Some(pairing_validator::validate_pairing(printer, resin.recipe()));
    let areas = cube_areas(10, 100.0);
    let result = SimulationRunner::run_from_areas(
        &areas,
        resin,
        printer,
        &test_supports(),
        &default_plate(),
        test_ambient(),
        None,
    );
    match result {
        Ok(sim) => {
            world.sim_primary = Some(sim);
            world.last_sim_err = None;
        }
        Err(e) => {
            world.last_sim_err = Some(e);
        }
    }
}

#[then(regex = r"^validate_pairing\(P, R\.recipe\(\)\) returns Ok\(\(\)\)$")]
fn then_pairing_ok(world: &mut UatWorld) {
    let res = world
        .pairing_result
        .as_ref()
        .expect("scenario invariant: When step ran pairing_validator");
    assert!(
        res.is_ok(),
        "pairing must be Ok for in-range recipe: {res:?}"
    );
}

#[then(regex = r"^SimulationRunner proceeds through slice_areas → predict_layer for every layer$")]
fn then_simulation_proceeds(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: simulation produced a PrintSimulation");
    assert!(
        !sim.layers().is_empty(),
        "simulation must yield non-empty layers"
    );
}

#[then(regex = r"^the returned PrintSimulation has the expected layer count$")]
fn then_expected_layer_count(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: simulation produced a PrintSimulation");
    assert_eq!(
        sim.layers().len(),
        10,
        "cube_areas(10) must yield 10 layers; got {}",
        sim.layers().len(),
    );
}

#[then(regex = r"^no pairing-prefixed error appears in the Err path$")]
fn then_no_pairing_err(world: &mut UatWorld) {
    if let Some(err) = &world.last_sim_err {
        assert!(
            !err.starts_with("pairing:"),
            "no pairing-prefixed error expected in happy path; got: {err}",
        );
    }
}

// ---- UAT-2: boundary values accepted --------------------------------------

#[given(regex = r#"^a printer "P" with layer_height_range_um min 20\.0 max 100\.0$"#)]
fn given_boundary_printer(world: &mut UatWorld) {
    world.printer = Some(printer_with_ranges(20.0, 100.0, 1.0, 60.0));
}

#[given(regex = r#"^resin "R" whose recipe\.layer_height_um = 20\.0 \(exactly at min\)$"#)]
fn given_resin_at_boundary(world: &mut UatWorld) {
    // generic_standard has layer_height_um = 50.0. For this scenario we'd
    // need a resin at 20.0 — recipe is pub(crate), so we can't mutate
    // directly. We assert the LIBRARY contract: FloatRange::contains is
    // inclusive, so pairing at a boundary is Ok. Build via TOML.
    let toml_str = r#"name = "BoundaryResin"
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
layer_height_um = 20.0
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
"#;
    let r: ResinProfile = toml::from_str(toml_str).expect("boundary resin TOML parses");
    r.validate().expect("boundary resin is valid");
    world.resin = Some(r);
}

#[when(regex = r"^validate_pairing\(P, R\.recipe\(\)\) is called$")]
fn when_pairing_called(world: &mut UatWorld) {
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: Given step set printer");
    let resin = world
        .resin
        .as_ref()
        .expect("scenario invariant: Given step set resin");
    world.pairing_result = Some(pairing_validator::validate_pairing(printer, resin.recipe()));
}

#[then(regex = r"^it returns Ok\(\(\)\)$")]
fn then_it_returns_ok(world: &mut UatWorld) {
    let res = world
        .pairing_result
        .as_ref()
        .expect("scenario invariant: When step ran pairing_validator");
    assert!(res.is_ok(), "boundary pairing must be Ok: {res:?}");
}

#[then(
    regex = r"^the same is true when recipe\.lift_speed_mm_min equals the range max of lift_speed_range_mm_min$"
)]
fn then_max_boundary_also_ok(world: &mut UatWorld) {
    // FloatRange::contains is inclusive. Build a resin at max and re-pair.
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: printer set");
    let max_mm_min = printer.lift_speed_range_mm_min().max();
    let toml_str = format!(
        r#"name = "MaxBoundaryResin"
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
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = {max_mm_min}
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#
    );
    let r: ResinProfile = toml::from_str(&toml_str).expect("max-boundary resin TOML parses");
    r.validate().expect("max-boundary resin is valid");
    let res = pairing_validator::validate_pairing(printer, r.recipe());
    assert!(
        res.is_ok(),
        "max-boundary lift_speed pairing must be Ok: {res:?}"
    );
}

#[given(
    regex = r#"^a printer "P2" pinned to a single layer height \(layer_height_range_um min 50\.0 max 50\.0\)$"#
)]
fn given_pinned_printer(world: &mut UatWorld) {
    world.printer = Some(printer_with_ranges(50.0, 50.0, 1.0, 60.0));
}

#[given(regex = r#"^resin "R2" whose recipe\.layer_height_um = 50\.0$"#)]
fn given_pinned_resin(world: &mut UatWorld) {
    world.resin = Some(ResinProfile::generic_standard()); // 50.0 default
}

#[when(regex = r"^pairing is validated$")]
fn when_pairing_validated(world: &mut UatWorld) {
    when_pairing_called(world);
}

#[then(regex = r#"^"R2" with recipe\.layer_height_um = 50\.1 returns Err naming the field$"#)]
fn then_slightly_off_returns_err(world: &mut UatWorld) {
    // Fold review finding #7: don't hardcode 50.1 — compute an out-of-
    // range layer_height programmatically from the printer's pinned
    // range. Any epsilon > 0 past max works; 0.1 is the plan-narrative
    // delta but the scenario's contract is "strictly past the max"
    // not "exactly +0.1". Keeps the scenario honest if validate()
    // gains a precision gate later.
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: printer set");
    let max_layer = printer.layer_height_range_um().max();
    // +0.1 matches scenario narrative but is derived, not hard-coded.
    let out_of_range = max_layer + 0.1;
    let toml_str = format!(
        r#"name = "SlightlyOff"
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
layer_height_um = {out_of_range}
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
    );
    let r: ResinProfile = toml::from_str(&toml_str).expect("slightly-off TOML parses");
    r.validate().expect("slightly-off resin is valid");
    let res = pairing_validator::validate_pairing(printer, r.recipe());
    let violations = res.expect_err("slightly-off pairing must return Err");
    let joined = violations.join("; ");
    assert!(
        joined.contains("layer_height_um"),
        "err must name layer_height_um: {joined}",
    );
}
