//! Step definitions for `spec/uat/ctb-layer-height-authority.md`
//! UAT-1 (mismatch path) and UAT-2 (agreement path).
//!
//! The When step invokes `SimulationRunner::run_from_layer_inputs`
//! programmatically with a synthesised LayerInput slice rather than
//! spawning the `resinsim sim` CLI on a real CTB fixture. Rationale:
//! the resinsim repo doesn't ship CTB fixtures (only STL), and
//! generating a minimal valid CTB header for tests is heavier than
//! the value it would add — the production code path between the
//! CLI parser and `run_from_layer_inputs` is the same as the test
//! invocation, and the warning text wording is locked by unit tests
//! in `values/layer_height_provenance.rs` (`format_warning_*`).
//!
//! The pattern mirrors `safety_factor_zero_force.rs`, which also
//! drives a high-level entry point programmatically rather than via
//! CLI subprocess.
//!
//! `then_exit_zero` (below) is shared: uat-unskip-c1 generalised it with
//! an observation-mode XOR guard so `cli-sim-producer-writes-sim-json`
//! UAT-1/UAT-3 (CLI subprocess) can reuse the same `^the process exits
//! with code 0$` regex this module's own UAT-1/UAT-2 (in-process) use.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

use super::world::{RecipeBuilder, ResinBuilder, UatWorld};

// ---- Scenario builders ----------------------------------------------------

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22 °C is a valid ambient")
}

fn solid_3x3_mask() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask is valid")
}

fn layer_inputs(n: u32, ctb_layer_height_um: f32) -> Vec<LayerInput> {
    let layer_height_mm = ctb_layer_height_um / 1000.0;
    (0..n)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25,
                2.5,
                60.0,
                ctb_layer_height_um,
                (i as f32 + 1.0) * layer_height_mm,
            )
            .expect("test fixture: literal LayerInput args satisfy preconditions");
            li.mask = Some(solid_3x3_mask());
            li
        })
        .collect()
}

// Delegates to the shared ResinBuilder (docs/patterns/anti/fixture-copy-of-
// shared-builder.md) instead of a hand-rolled TOML literal. Field-by-field
// diff evidence (scratch comparison during implementation): ResinBuilder's
// chemistry defaults (critical_energy 5.0, penetration_depth 170.0,
// viscosity 200.0, tensile_strength 35.0, peel_adhesion 13.0,
// ref_lift_speed 60.0, reference_temp 25.0, activation_energy 52.0,
// density 1.1, linear_shrinkage 1.5) and RecipeBuilder's recipe defaults
// (bottom_layer_count 6, transition_layers 3, normal_exposure 2.5,
// bottom_exposure 25.0, lift 60.0, cycle 7.5, distance 5.0) are
// byte-identical in VALUE to the former literal; the only textual diff is
// Rust's f32 Display dropping trailing ".0" (pre-existing ResinBuilder
// behaviour, not introduced here) plus the three newly-required ADR-0020
// thermal lines.
fn resin_with_layer_height(recipe_um: f32) -> ResinProfile {
    ResinBuilder::new()
        .with_name("Generic Standard (test override)")
        .with_recipe(RecipeBuilder::new().with_layer_height(recipe_um))
        .build()
}

// Per-scenario state lives on the World (UatWorld.ctb_layer_inputs +
// existing UatWorld.resin / UatWorld.printer). thread_local DOES NOT
// work here: cucumber runs scenarios concurrently, so a thread_local
// leaks state across scenarios on the same thread (the bug that
// produced this comment when the first attempt failed with
// "ctb_um: 50.0" in a CTB=40 scenario).

// ---- Given steps ----------------------------------------------------------

#[given(regex = r"^a CTB input sliced at (\d+) µm$")]
fn given_ctb_sliced_at(world: &mut UatWorld, ctb_um: u32) {
    world.ctb_layer_inputs = Some(layer_inputs(5, ctb_um as f32));
}

#[given(
    regex = r"^a CTB input with per-layer layer_height_um values (\d+) (\d+) (\d+) (\d+) (\d+) µm$"
)]
fn given_ctb_variable_layer_heights(
    world: &mut UatWorld,
    h0: u32,
    h1: u32,
    h2: u32,
    h3: u32,
    h4: u32,
) {
    // 5-layer adaptive-slicing fixture; layer Z positions are
    // accumulated from the per-layer heights so the LayerInput stack is
    // internally consistent.
    let heights = [h0 as f32, h1 as f32, h2 as f32, h3 as f32, h4 as f32];
    let mut z_mm = 0.0_f32;
    let mut layers = Vec::with_capacity(5);
    for (i, h) in heights.iter().enumerate() {
        z_mm += h / 1000.0;
        let mut li = LayerInput::new(i as u32, 3.0 * 3.0 * 0.25, 2.5, 60.0, *h, z_mm)
            .expect("test fixture: literal LayerInput args satisfy preconditions");
        li.mask = Some(solid_3x3_mask());
        layers.push(li);
    }
    world.ctb_layer_inputs = Some(layers);
}

#[given(regex = r"^a resin profile whose recipe\.layer_height_um is (\d+) µm$")]
fn given_resin_with_recipe_layer_height(world: &mut UatWorld, recipe_um: u32) {
    world.resin = Some(resin_with_layer_height(recipe_um as f32));
}

#[given(
    regex = r"^a printer profile whose layer_height_range_um contains (?:both \d+ and \d+|\d+) µm$"
)]
fn given_printer_with_range(world: &mut UatWorld) {
    // generic_msla_4k's layer_height_range_um is [20, 100] — covers
    // every value used in this UAT (30, 40, 50 µm).
    world.printer = Some(PrinterProfile::generic_msla_4k());
}

// ---- When step ------------------------------------------------------------

#[when(
    regex = r"^the user invokes `resinsim sim --file <CTB> --resin <RESIN> --printer <PRINTER> --out <OUT>`$"
)]
fn when_user_runs_sim(world: &mut UatWorld) {
    let layers = world
        .ctb_layer_inputs
        .as_ref()
        .expect("Given step set CTB layer inputs")
        .clone();
    let resin = world.resin.clone().expect("Given step set resin");
    let printer = world.printer.clone().expect("Given step set printer");
    let result = SimulationRunner::run_from_layer_inputs(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    );
    match result {
        Ok(sim) => world.sim_primary = Some(sim),
        Err(e) => world.last_sim_err = Some(e),
    }
}

// ---- Then steps -----------------------------------------------------------

// Shared step: owned here for this module's own in-process UAT-1/UAT-2
// scenarios, and reused (uat-unskip-c1) by CLI-subprocess scenarios in
// `cli-sim-producer-writes-sim-json` UAT-1 and UAT-3 (pointer comments
// there name this owner — a second registration of the same regex would
// be a runtime ambiguous-match error). Exactly one observation mode may
// be populated per scenario: in-process (`sim_primary` / `last_sim_err`,
// set by this module's own When) XOR CLI subprocess (`cli_exit_code`,
// set by `invoke_resinsim`). The XOR guard is load-bearing, not
// decoration — without it a scenario whose When silently populated
// neither field would pass vacuously.
#[then(regex = r"^the process exits with code 0$")]
fn then_exit_zero(world: &mut UatWorld) {
    let in_process = world.sim_primary.is_some() || world.last_sim_err.is_some();
    let cli = world.cli_exit_code.is_some();
    assert!(
        in_process ^ cli,
        "expected exactly one observation mode populated (in-process XOR cli_exit_code); \
         in_process={in_process} cli={cli}"
    );
    if cli {
        assert_eq!(
            world.cli_exit_code,
            Some(0),
            "expected CLI subprocess to exit 0; stderr={:?}",
            world.cli_stderr
        );
        let stderr = world.cli_stderr.as_deref().unwrap_or("");
        assert!(
            !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
            "expected no panic in stderr, got: {stderr}"
        );
    } else {
        assert!(
            world.sim_primary.is_some() && world.last_sim_err.is_none(),
            "expected successful simulation (exit 0 equivalent); err={:?}",
            world.last_sim_err
        );
    }
}

// Phrase-specific assertions for the layer-height-mismatch warning text.
// Each regex is anchored to the UAT-1 spec wording so it cannot collide
// with broader `stderr contains` step defs in other UAT modules (cucumber
// rejects ambiguous matches across the whole inventory).

fn assert_warning_contains(world: &mut UatWorld, needle: &str) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("Then-stderr fired before successful When");
    let provenance = sim
        .layer_height_provenance()
        .expect("CTB-based run must surface layer_height_provenance");
    let profile_name = world
        .resin
        .as_ref()
        .map(|r| r.name().to_string())
        .unwrap_or_else(|| "test".to_string());
    // Programmatic equivalent of stderr capture: re-format the warning
    // and check its content. Production CLI calls
    // `emit_layer_height_warning_if_mismatch` which delegates to
    // `LayerHeightProvenance::format_warning` and `eprintln!`s the
    // same string. Wording locked by unit tests in
    // `values/layer_height_provenance.rs`.
    let text = provenance
        .format_warning(&profile_name)
        .expect("mismatch path must format a warning");
    assert!(
        text.contains(needle),
        "expected warning to contain {needle:?}, got: {text}"
    );
}

#[then(regex = r#"^stderr contains "WARNING: CTB layer_height \(40"$"#)]
fn then_stderr_contains_warning_prefix(world: &mut UatWorld) {
    assert_warning_contains(world, "WARNING: CTB layer_height (40");
}

#[then(regex = r#"^stderr contains "does NOT match recipe layer_height"$"#)]
fn then_stderr_contains_mismatch_phrase(world: &mut UatWorld) {
    assert_warning_contains(world, "does NOT match recipe layer_height");
}

#[then(regex = r#"^stderr contains "GUESS"$"#)]
fn then_stderr_contains_guess(world: &mut UatWorld) {
    assert_warning_contains(world, "GUESS");
}

#[then(regex = r#"^stderr contains "WRONG LAYER COUNT"$"#)]
fn then_stderr_contains_wrong_layer_count(world: &mut UatWorld) {
    assert_warning_contains(world, "WRONG LAYER COUNT");
}

#[then(regex = r#"^stderr contains "variable layer height"$"#)]
fn then_stderr_contains_variable_layer_height(world: &mut UatWorld) {
    assert_warning_contains(world, "variable layer height");
}

#[then(
    regex = r#"^the produced sim\.json's `simulation\.layer_height_provenance\.mismatch\.kind` equals "variable"$"#
)]
fn then_provenance_mismatch_kind_variable(world: &mut UatWorld) {
    use resinsim_core::values::MismatchKind;
    let p = world
        .sim_primary
        .as_ref()
        .expect("provenance assertion after successful When")
        .layer_height_provenance()
        .expect("CTB-based run surfaces provenance");
    let m = p
        .mismatch()
        .expect("variable-Z runs must populate mismatch");
    assert!(
        matches!(m.kind, MismatchKind::Variable),
        "expected Variable mismatch kind, got {:?}",
        m.kind
    );
}

#[then(regex = r#"^stderr does NOT contain "WARNING: CTB layer_height"$"#)]
fn then_stderr_does_not_contain_warning(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("Then-no-stderr fired before successful When");
    let provenance = sim
        .layer_height_provenance()
        .expect("CTB-based run must surface layer_height_provenance");
    assert!(
        provenance.format_warning("test").is_none(),
        "agreement path must NOT format a warning (provenance: {provenance:?})"
    );
}

#[then(
    regex = r"^the produced sim\.json's `simulation\.layer_height_provenance\.ctb_um` equals (\d+)$"
)]
fn then_provenance_ctb_um_equals(world: &mut UatWorld, expected: u32) {
    let p = world
        .sim_primary
        .as_ref()
        .expect("provenance assertion after successful When")
        .layer_height_provenance()
        .expect("CTB-based run surfaces provenance");
    // The Gherkin shorthand `.ctb_um` resolves to the uniform value for
    // uniform CTBs (the common case under UAT-1 / UAT-2 fixtures). On
    // variable-Z runs the test would fail loudly here — by design;
    // adaptive-slicing assertions go through a dedicated step.
    let ctb_um = p
        .uniform_height_um()
        .expect("uniform CTB expected for ctb_um Gherkin shorthand");
    assert!(
        (ctb_um - expected as f32).abs() < 1e-3,
        "ctb_um = {ctb_um} ≠ expected {expected}"
    );
}

#[then(
    regex = r"^the produced sim\.json's `simulation\.layer_height_provenance\.recipe_um` equals (\d+)$"
)]
fn then_provenance_recipe_um_equals(world: &mut UatWorld, expected: u32) {
    let p = world
        .sim_primary
        .as_ref()
        .expect("provenance assertion after successful When")
        .layer_height_provenance()
        .expect("CTB-based run surfaces provenance");
    assert!(
        (p.recipe_um() - expected as f32).abs() < 1e-3,
        "recipe_um = {} ≠ expected {expected}",
        p.recipe_um()
    );
}

#[then(
    regex = r"^the produced sim\.json's `simulation\.layer_height_provenance\.mismatch` is present$"
)]
fn then_provenance_mismatch_present(world: &mut UatWorld) {
    let p = world
        .sim_primary
        .as_ref()
        .expect("provenance assertion after successful When")
        .layer_height_provenance()
        .expect("CTB-based run surfaces provenance");
    assert!(p.has_mismatch(), "mismatch must be present: {p:?}");
}

#[then(
    regex = r"^the produced sim\.json's `simulation\.layer_height_provenance\.mismatch` is absent$"
)]
fn then_provenance_mismatch_absent(world: &mut UatWorld) {
    let p = world
        .sim_primary
        .as_ref()
        .expect("provenance assertion after successful When")
        .layer_height_provenance()
        .expect("CTB-based run surfaces provenance");
    assert!(
        !p.has_mismatch(),
        "mismatch must be absent on agreement: {p:?}"
    );
}
