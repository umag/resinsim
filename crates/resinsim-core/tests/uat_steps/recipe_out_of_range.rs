//! Step definitions for `spec/uat/recipe-outside-printer-range.md`
//! (UAT-1 + UAT-2).
//!
//! Step 2 smoke test. Validates that cucumber-rs parses and executes the
//! scenarios after they round-trip through the markdown extractor, with
//! BOTH DataTable (`|` rows) AND DocString (triple-quote) compound inputs
//! used as cucumber step arguments.
//!
//! Trust contract: the scenario text hard-codes the 50.0 / 2.5 default
//! values that `ResinProfile::generic_standard()` provides. Step bodies
//! assert those defaults match the declared Given values so a future
//! drift between spec values and factory defaults surfaces here rather
//! than as a cryptic pairing-violation diff.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use resinsim_core::app::simulation_runner::SimulationRunner;
use resinsim_core::entities::ResinProfile;

use super::fixtures::{cube_areas, default_plate, printer_with_ranges, test_ambient, test_supports};
use super::world::UatWorld;

// ---- UAT-1: single-range narrowing + natural-prose Given/And ----

#[given(
    regex = r#"^a narrowed printer "P" with layer_height_range_um min ([0-9.]+) max ([0-9.]+)$"#
)]
fn given_narrowed_printer_layer_only(world: &mut UatWorld, min: f32, max: f32) {
    world.printer = Some(printer_with_ranges(min, max, 1.0, 60.0));
}

#[given(regex = r#"^a resin profile "R" whose recipe has layer_height_um ([0-9.]+)$"#)]
fn given_resin_profile_layer_height(world: &mut UatWorld, declared: f32) {
    // Sanity: scenario text must match the factory default so we don't lie
    // about what ResinProfile::generic_standard() provides.
    assert!(
        (declared - 50.0).abs() < 1e-3,
        "UAT-1 Given declares layer_height_um = {declared}; must match generic_standard's 50.0",
    );
    world.resin = Some(ResinProfile::generic_standard());
}

#[when(regex = r"^SimulationRunner\.run_from_areas is invoked$")]
fn when_run_from_areas(world: &mut UatWorld) {
    let printer = world
        .printer
        .as_ref()
        .expect("scenario invariant: Given step set printer");
    let resin = world
        .resin
        .as_ref()
        .expect("scenario invariant: Given step set resin");
    let areas = cube_areas(5, 100.0);
    let result = SimulationRunner::run_from_areas(
        &areas,
        resin,
        printer,
        &test_supports(),
        &default_plate(),
        test_ambient(),
        None,
    );
    world.last_sim_err = Some(
        result
            .err()
            .unwrap_or_else(|| String::from("<no error — scenario expected Err>")),
    );
}

#[then(regex = r#"^the call returns Err whose message begins with "pairing:"$"#)]
fn then_uat1_err_begins_with_pairing(world: &mut UatWorld) {
    let err = world
        .last_sim_err
        .as_ref()
        .expect("scenario invariant: When step set last_sim_err");
    assert!(
        err.starts_with("pairing:"),
        "expected 'pairing:' prefix; got: {err}",
    );
}

#[then(regex = r#"^the error names "([a-z_]+)" as the offending recipe field$"#)]
fn then_err_names_field(world: &mut UatWorld, field: String) {
    let err = world
        .last_sim_err
        .as_ref()
        .expect("scenario invariant: When step set last_sim_err");
    assert!(
        err.contains(&field),
        "err must name field '{field}': {err}",
    );
}

// ---- UAT-2: DataTable ranges + DataTable recipe + DocString assertion ----

#[given(regex = r#"^a narrowed printer "P" with ranges:$"#)]
fn given_narrowed_printer_datatable(world: &mut UatWorld, step: &Step) {
    let table = step
        .table
        .as_ref()
        .expect("scenario invariant: Given step carries a DataTable");
    // Row 0 is the header "| range | min | max |".
    let mut layer_range = (20.0f32, 100.0f32);
    let mut exposure_range = (1.0f32, 60.0f32);
    for row in table.rows.iter().skip(1) {
        let name = row.first().map(String::as_str).unwrap_or_default();
        let min: f32 = row
            .get(1)
            .and_then(|s| s.parse().ok())
            .expect("DataTable min cell is a finite f32");
        let max: f32 = row
            .get(2)
            .and_then(|s| s.parse().ok())
            .expect("DataTable max cell is a finite f32");
        match name {
            "layer_height_range_um" => layer_range = (min, max),
            "exposure_range_sec" => exposure_range = (min, max),
            other => panic!("unrecognised printer range name in DataTable: '{other}'"),
        }
    }
    world.printer = Some(printer_with_ranges(
        layer_range.0,
        layer_range.1,
        exposure_range.0,
        exposure_range.1,
    ));
}

#[given(regex = r#"^a resin "R" whose recipe has:$"#)]
fn given_resin_recipe_datatable(world: &mut UatWorld, step: &Step) {
    let table = step
        .table
        .as_ref()
        .expect("scenario invariant: Given step carries a DataTable");
    // Row 0 is header "| field | value |". Sanity-check each declared value
    // against the factory default — ResinProfile's recipe fields are
    // pub(crate) so the scenario can't mutate them; the step therefore
    // asserts narrative / factory alignment.
    for row in table.rows.iter().skip(1) {
        let field = row.first().map(String::as_str).unwrap_or_default();
        let value: f32 = row
            .get(1)
            .and_then(|s| s.parse().ok())
            .expect("DataTable value cell is a finite f32");
        match field {
            "layer_height_um" => assert!(
                (value - 50.0).abs() < 1e-3,
                "UAT-2 DataTable declares layer_height_um = {value}; must match generic_standard's 50.0",
            ),
            "normal_exposure_sec" => assert!(
                (value - 2.5).abs() < 1e-3,
                "UAT-2 DataTable declares normal_exposure_sec = {value}; must match generic_standard's 2.5",
            ),
            other => panic!("unrecognised recipe field in DataTable: '{other}'"),
        }
    }
    world.resin = Some(ResinProfile::generic_standard());
}

#[then(regex = r#"^the returned Err begins with "pairing:"$"#)]
fn then_uat2_err_begins_with_pairing(world: &mut UatWorld) {
    let err = world
        .last_sim_err
        .as_ref()
        .expect("scenario invariant: When step set last_sim_err");
    assert!(
        err.starts_with("pairing:"),
        "expected 'pairing:' prefix; got: {err}",
    );
}

#[then(regex = r"^the returned Err mentions every field:$")]
fn then_err_mentions_docstring_fields(world: &mut UatWorld, step: &Step) {
    let docstring = step
        .docstring
        .as_ref()
        .expect("scenario invariant: Then step carries a DocString");
    let err = world
        .last_sim_err
        .as_ref()
        .expect("scenario invariant: When step set last_sim_err");
    for field in docstring.lines() {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        assert!(
            err.contains(field),
            "err must mention field '{field}' from DocString: {err}",
        );
    }
}

#[then(regex = r#"^violations are joined with "; " in a single error message$"#)]
fn then_violations_joined_semicolon(world: &mut UatWorld) {
    let err = world
        .last_sim_err
        .as_ref()
        .expect("scenario invariant: When step set last_sim_err");
    assert!(
        err.contains("; "),
        "err must join violations with '; ' separator: {err}",
    );
}
