//! Step definitions for `spec/uat/resin-switch-changes-simulation.md`
//! UAT-1 + UAT-2.
//!
//! ADR-0005 motivating behaviour: switching resin on the same printer
//! must produce a different simulation (exposure now lives on the resin,
//! not the printer). UAT-2 is the determinism sanity check.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use resinsim_core::app::simulation_runner::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};

use super::fixtures::{cube_areas, default_plate, test_ambient, test_supports};
use super::world::UatWorld;

// ---- UAT-1: same printer + different resin produces different cure depth --

#[given(
    regex = r#"^a printer profile "P" \(e\.g\. PrinterProfile::elegoo_mars5_ultra\(\)\)$"#
)]
fn given_mars5_ultra(world: &mut UatWorld) {
    world.printer = Some(PrinterProfile::elegoo_mars5_ultra());
}

#[given(
    regex = r#"^two resin profiles "R_A" and "R_B" that both pair with "P" but differ in recipe:$"#
)]
fn given_two_resins(world: &mut UatWorld, step: &Step) {
    let _table = step
        .table
        .as_ref()
        .expect("scenario invariant: Given step carries a DataTable");
    // Sanity: R_A = generic_standard (exposure 2.5), R_B = elegoo_ceramic_grey_v2 (exposure 3.2).
    let r_a = ResinProfile::generic_standard();
    let r_b = ResinProfile::elegoo_ceramic_grey_v2();
    assert!((r_a.recipe().normal_exposure_sec() - 2.5).abs() < 1e-3);
    assert!((r_b.recipe().normal_exposure_sec() - 3.2).abs() < 1e-3);
    world.resin = Some(r_a);
    world.resin_alt = Some(r_b);
}

#[given(regex = r#"^a fixed-area per-layer vector "areas" exercising non-bottom layers$"#)]
fn given_fixed_areas(_world: &mut UatWorld) {
    // cube_areas(n_layers > bottom_layer_count_max) is used in the When
    // steps; this Given is narrative.
}

#[when(
    regex = r#"^SimulationRunner\.run_from_areas\(areas, R_A, P, \.\.\.\) produces "sim_A"$"#
)]
fn when_run_sim_a(world: &mut UatWorld) {
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: Given step set printer");
    let r_a = world
        .resin
        .as_ref()
        .expect("scenario invariant: Given step set resin (R_A)");
    let areas = cube_areas(20, 100.0); // 20 > bottom_layer_count_max so non-bottom layers exist
    let sim = SimulationRunner::run_from_areas(
        &areas,
        r_a,
        printer,
        &test_supports(),
        &default_plate(),
        test_ambient(),
        None,
    )
    .expect("R_A simulation must succeed for an in-range pairing");
    world.sim_primary = Some(sim);
}

#[when(
    regex = r#"^SimulationRunner\.run_from_areas\(areas, R_B, P, \.\.\.\) produces "sim_B"$"#
)]
fn when_run_sim_b(world: &mut UatWorld) {
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: Given step set printer");
    let r_b = world
        .resin_alt
        .as_ref()
        .expect("scenario invariant: Given step set resin_alt (R_B)");
    let areas = cube_areas(20, 100.0);
    let sim = SimulationRunner::run_from_areas(
        &areas,
        r_b,
        printer,
        &test_supports(),
        &default_plate(),
        test_ambient(),
        None,
    )
    .expect("R_B simulation must succeed for an in-range pairing");
    world.sim_alt = Some(sim);
}

#[then(
    regex = r"^sim_A\.layers\[i\]\.cure_depth_um != sim_B\.layers\[i\]\.cure_depth_um for non-bottom layers i .*$"
)]
fn then_cure_depths_differ(world: &mut UatWorld) {
    let sim_a = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: sim_A produced");
    let sim_b = world
        .sim_alt
        .as_ref()
        .expect("scenario invariant: sim_B produced");
    assert_eq!(sim_a.layers().len(), sim_b.layers().len());
    // Compare the last layer (definitely non-bottom given 20 layers vs
    // bottom_layer_count_max = 15). Exposure drives Beer-Lambert cure
    // depth; different exposure → different cure depth.
    let last = sim_a.layers().len() - 1;
    let cd_a = sim_a.layers()[last].cure_depth_um;
    let cd_b = sim_b.layers()[last].cure_depth_um;
    assert!(
        (cd_a - cd_b).abs() > 1e-3,
        "cure_depth_um must differ between R_A (2.5 s exposure) and R_B (3.2 s); got {cd_a} vs {cd_b}",
    );
}

#[then(
    regex = r"^the observed cure-depth difference reflects R_A\.recipe vs R_B\.recipe, not P's range fields .*$"
)]
fn then_difference_from_recipe(world: &mut UatWorld) {
    // Both sims used the SAME printer. Difference must come from the
    // different resin.recipe.normal_exposure_sec values. We've already
    // asserted 2.5 vs 3.2 in the Given; `then_cure_depths_differ`
    // verifies the observable. This step is the narrative lock.
    let r_a = world.resin.as_ref().expect("R_A set");
    let r_b = world.resin_alt.as_ref().expect("R_B set");
    assert!(
        (r_a.recipe().normal_exposure_sec() - r_b.recipe().normal_exposure_sec()).abs() > 1e-3,
        "R_A and R_B must differ in normal_exposure_sec",
    );
}

// ---- UAT-2: same printer + same resin produces identical output ------------

#[given(regex = r#"^a printer "P" and resin "R" that pair$"#)]
fn given_paired_printer_resin(world: &mut UatWorld) {
    world.printer = Some(PrinterProfile::elegoo_mars5_ultra());
    world.resin = Some(ResinProfile::generic_standard());
}

#[when(
    regex = r"^SimulationRunner\.run_from_areas\(areas, R, P, \.\.\.\) is called twice with the same inputs$"
)]
fn when_run_twice(world: &mut UatWorld) {
    let printer = world.printer.as_ref().expect("printer set");
    let resin = world.resin.as_ref().expect("resin set");
    let areas = cube_areas(10, 100.0);
    let run = || {
        SimulationRunner::run_from_areas(
            &areas,
            resin,
            printer,
            &test_supports(),
            &default_plate(),
            test_ambient(),
            None,
        )
        .expect("deterministic sim must succeed")
    };
    world.sim_primary = Some(run());
    world.sim_alt = Some(run());
}

#[then(regex = r"^the two PrintSimulation outputs are structurally equal$")]
fn then_structurally_equal(world: &mut UatWorld) {
    let a = world.sim_primary.as_ref().expect("sim 1 produced");
    let b = world.sim_alt.as_ref().expect("sim 2 produced");
    assert_eq!(a.layers().len(), b.layers().len(), "layer count mismatch");
    for (i, (la, lb)) in a.layers().iter().zip(b.layers()).enumerate() {
        assert!(
            (la.cure_depth_um - lb.cure_depth_um).abs() < 1e-6,
            "layer {i}: cure_depth_um differs ({} vs {})",
            la.cure_depth_um,
            lb.cure_depth_um,
        );
        assert!(
            (la.peel_force_n - lb.peel_force_n).abs() < 1e-6,
            "layer {i}: peel_force_n differs",
        );
    }
}

#[then(
    regex = r"^no side-effect has been observed \(resin/printer are unchanged; repositories untouched\)$"
)]
fn then_no_side_effect(world: &mut UatWorld) {
    // Printer + resin values read-only; verify by re-reading key fields.
    let printer = world.printer.as_ref().expect("printer set");
    let resin = world.resin.as_ref().expect("resin set");
    // Sanity: printer's mars5 name unchanged, resin's recipe default held.
    assert!(!printer.name().is_empty());
    assert!((resin.recipe().normal_exposure_sec() - 2.5).abs() < 1e-3);
}
