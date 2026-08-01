//! Step definitions for `spec/uat/base-adhesion-shifts-peel-peak.md`
//! UAT-1..UAT-3 (uat-unskip-campaign increment 1, plan step 8).
//!
//! UAT-1/UAT-2 drive `SimulationRunner::run_from_areas` end to end and
//! read `LayerResult.base_force_n` / `total_force_n` / `peel_force_n`
//! directly — never a recomputed `Δσ₀ · exp(-layer/τ) · A · 1e-3`
//! (docs/patterns/anti/test-mirrors-production-formula.md). UAT-3 asserts
//! `PeelForceCalculator::peel_force` against the KB-114 reference Newton
//! values the shipped nextest suite already pins.
//!
//! "When a job is simulated with that resin" is registered ONCE here —
//! UAT-1 and UAT-2 use the identical literal text, so cucumber-rs's
//! single global step registry serves both from one `#[when]`. It is
//! textually DISTINCT from every When in the two sibling peel-physics
//! specs (profile-vacuum-pressure-scales-suction: "the job is simulated" /
//! "a job with a sealed cavity is simulated" / "the profile is validated
//! ..."; peel-shape-factor-scales-with-aspect-ratio: "a job is simulated"
//! (no "with that resin") / "the per-layer shape factors are computed
//! [from the masks]" / "a peel_shape_factor of 0.5 is applied") — verified
//! by direct comparison against both .md files, not by assumption. No
//! shared regex, so no pointer-comment redirect is needed in the siblings;
//! this note documents that the check was made
//! (anti/cucumber-step-regex-ambiguity.md's mitigation is "run cargo uat
//! after every module", which step 9/10 do regardless).

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::PrinterProfile;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::services::PeelForceCalculator;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::world::{PrinterBuilder, ResinBuilder, UatWorld};

/// Layers to simulate — comfortably past `RecipeBuilder`'s default
/// `bottom_layer_count` (6) so the base-adhesion exponential relaxation
/// (τ = bottom_layer_count) is well past its half-life by the last layer.
const LAYER_COUNT: usize = 20;
/// `RecipeBuilder`'s default `bottom_layer_count` — mirrored here (not
/// re-read from the built resin) only for the "bottom layers" iteration
/// bound; the VALUE asserted on is always read from `LayerResult`, never
/// recomputed.
const BOTTOM_LAYER_COUNT: usize = 6;

// ---- UAT-1: an opting-in resin lifts and reveals the base term ------------

#[given(regex = r"^a resin whose base_adhesion_elevation_kpa is non-zero$")]
fn given_base_adhesion_nonzero(world: &mut UatWorld) {
    // 40 kPa matches data/resins/generic_standard.toml's shipped value
    // (KB-116 indicative default) so this scenario's fixture and the CLI
    // calibrate assertion below (which uses generic_standard unmodified)
    // agree on the same physical value.
    world.peel_resin = Some(
        ResinBuilder::new()
            .with_base_adhesion_elevation_kpa(40.0)
            .build(),
    );
}

// ---- UAT-2: an unset resin is behaviour-preserving -------------------------

#[given(regex = r"^a resin whose base_adhesion_elevation_kpa is unset \(or 0\)$")]
fn given_base_adhesion_unset(world: &mut UatWorld) {
    world.peel_resin = Some(ResinBuilder::new().build());
}

// ---- shared When ------------------------------------------------------------

#[when(regex = r"^a job is simulated with that resin$")]
fn when_job_simulated(world: &mut UatWorld) {
    let resin = world
        .peel_resin
        .clone()
        .expect("scenario invariant: Given step populated peel_resin");
    let printer: PrinterProfile = PrinterBuilder::new().build();
    let area = CrossSectionArea::new(100.0).expect("100 mm² is non-negative");
    let areas = vec![area; LAYER_COUNT];
    let supports = SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 10,
    };
    let plate = PlateAdhesionProfile::default_textured();
    let ambient = AmbientTemperature::new(22.0).expect("22 °C is in AmbientTemperature domain");

    let sim = SimulationRunner::run_from_areas(
        &areas, &resin, &printer, &supports, &plate, ambient, None,
    )
    .expect(
        "scenario fixture: ResinBuilder/PrinterBuilder output satisfies \
             run_from_areas preconditions",
    );
    world.peel_printer = Some(printer);
    world.peel_sim_layers = Some(sim.layers().to_vec());
}

// ---- UAT-1 Then steps -------------------------------------------------------

#[then(regex = r"^the bottom layers report total_force_n greater than peel_force_n$")]
fn then_bottom_layers_total_exceeds_peel(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    for l in layers.iter().take(BOTTOM_LAYER_COUNT) {
        assert!(
            l.total_force_n > l.peel_force_n,
            "layer {}: total_force_n {} must exceed peel_force_n {} (base term active)",
            l.index,
            l.total_force_n,
            l.peel_force_n,
        );
    }
}

#[then(
    regex = r"^each LayerResult carries a base_force_n that is largest at layer 0 and relaxes toward 0 over ~bottom_layer_count layers$"
)]
fn then_base_force_relaxes(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    let base0 = layers[0].base_force_n;
    assert!(
        base0 > 0.0,
        "layer 0 base_force_n must be non-zero; got {base0}"
    );
    for w in layers.windows(2) {
        assert!(
            w[1].base_force_n <= w[0].base_force_n + 1e-6,
            "base_force_n must be non-increasing (exp. decay): layer {} = {}, layer {} = {}",
            w[0].index,
            w[0].base_force_n,
            w[1].index,
            w[1].base_force_n,
        );
    }
    let last = layers.last().expect("LAYER_COUNT > 0").base_force_n;
    assert!(
        last < base0 * 0.1,
        "base_force_n should relax to well under 10% of its layer-0 peak by layer {}: \
         layer 0 = {base0}, last = {last}",
        LAYER_COUNT - 1,
    );
}

#[then(regex = r#"^`inspect calibrate` prints "Predicted base adhesion \(layer 0\): <N> N"$"#)]
fn then_calibrate_prints_base_adhesion(_world: &mut UatWorld) {
    // Uses the small (1.9 kB) crates/resinsim-core/tests/fixtures/mini.nanodlp
    // fixture — NOT the 37 MB Athena reference print, which must never enter
    // the test suite. data/resins/generic_standard.toml already ships
    // base_adhesion_elevation_kpa = 40.0, so no TOML patching is needed for
    // the "line present" assertion.
    let fixture = mini_nanodlp_path();
    let data_dir = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "calibrate",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "athena_ii",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "inspect calibrate must succeed; stderr={}",
        outcome.stderr
    );
    assert!(
        outcome
            .stdout
            .contains("Predicted base adhesion (layer 0):")
            && outcome.stdout.contains(" N — KB-116 first-layer term"),
        "stdout must carry the Predicted base adhesion line; got: {}",
        outcome.stdout,
    );
}

#[then(regex = r"^the predicted-vs-real peak-layer offset is smaller than without the term$")]
fn then_peak_offset_smaller(world: &mut UatWorld) {
    // NOT independently asserted against a real predicted-vs-real
    // comparison: the only small fixture available (mini.nanodlp, 3
    // layers) is too short to show the KB-115 "sim peak sits mid-print"
    // effect a real multi-hundred-layer print does — verified empirically
    // (2026-08-01): both with and without base_adhesion_elevation_kpa set,
    // `inspect calibrate` against mini.nanodlp reports "offset +0" for
    // this fixture, so a literal `<` comparison would either be
    // vacuously true or actively wrong, not a real assertion. The 37 MB
    // Athena reference print that WOULD show the effect must never enter
    // the suite (project convention). Per the plan's own fallback
    // guidance, this bullet is proxied by the observable proxy the
    // shipped nextest tests use and the prior Then step already proved
    // production-side: base_force_n peaks at layer 0 and decays — the
    // mechanism the peak-offset narrows FOR. Re-assert the same
    // production capture here (not a new formula, not a mirror) so this
    // step is a real check, not a no-op.
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    let base0 = layers[0].base_force_n;
    assert!(
        layers.iter().all(|l| l.base_force_n <= base0 + 1e-6),
        "layer 0's base_force_n must be the global maximum — the mechanism \
         that pulls the predicted peak toward the real (layer-0) peak",
    );
}

// ---- UAT-2 Then steps -------------------------------------------------------

#[then(regex = r"^every layer's base_force_n is 0\.0$")]
fn then_every_base_force_zero(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    for l in layers {
        assert_eq!(
            l.base_force_n, 0.0,
            "layer {}: base_force_n must be exactly 0.0 when unset; got {}",
            l.index, l.base_force_n
        );
    }
}

#[then(regex = r"^total_force_n equals peel_force_n \+ suction_force_n \(no base contribution\)$")]
fn then_total_equals_peel_plus_suction(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    for l in layers {
        let expected = l.peel_force_n + l.suction_force_n;
        assert!(
            (l.total_force_n - expected).abs() < 1e-4,
            "layer {}: total_force_n {} != peel {} + suction {} = {}",
            l.index,
            l.total_force_n,
            l.peel_force_n,
            l.suction_force_n,
            expected,
        );
    }
}

#[then(regex = r#"^`inspect calibrate` prints no "Predicted base adhesion" line$"#)]
fn then_calibrate_prints_no_base_adhesion(_world: &mut UatWorld) {
    let fixture = mini_nanodlp_path();
    let tmp_data_dir = resin_data_dir_without_base_adhesion();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "calibrate",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "athena_ii",
            "--data-dir",
            tmp_data_dir.to_str().expect("tmp data dir path is UTF-8"),
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "inspect calibrate must succeed; stderr={}",
        outcome.stderr
    );
    assert!(
        !outcome.stdout.contains("Predicted base adhesion"),
        "stdout must NOT carry the Predicted base adhesion line when the \
         resin has no base_adhesion_elevation_kpa; got: {}",
        outcome.stdout,
    );
}

// ---- UAT-3: the KB-114 peel vectors are undisturbed ------------------------

#[given(regex = r"^the KB-114 reference cases \(σ, A, f\(v\)\)$")]
fn given_kb114_cases(_world: &mut UatWorld) {
    // Narrative — the vectors are named literals in the Then step below,
    // matching the pattern already established for peel_force_calculator's
    // own nextest suite (peel_force_50mm_square_standard_fep et al.).
}

#[when(regex = r"^peel_force is evaluated$")]
fn when_peel_force_evaluated(_world: &mut UatWorld) {
    // Narrative — PeelForceCalculator::peel_force is a pure function with
    // no World state to capture; the Then step calls it directly.
}

#[then(regex = r"^it returns the KB-114 Newton values unchanged$")]
fn then_kb114_values_unchanged(_world: &mut UatWorld) {
    // (σ kPa, A mm², f(v), expected N) — the exact vectors
    // peel_force_calculator.rs's own KB-114 nextest suite pins. Calling
    // the production function and comparing to these fixed reference
    // numbers is NOT a formula mirror (docs/patterns/anti/test-mirrors-
    // production-formula.md): the arithmetic itself is never re-typed
    // here, only the production output vs. a literal expected number.
    const CASES: [(f32, f64, f32, f32); 6] = [
        (13.0, 2500.0, 1.0, 32.5),
        (13.0, 100.0, 1.0, 1.3),
        (13.0, 8160.0, 1.0, 106.08),
        (18.0, 2500.0, 1.0, 45.0),
        (12.0, 2500.0, 1.0, 30.0),
        (13.0, 2500.0, 2.3, 74.75),
    ];
    for (sigma_kpa, area_mm2, f_v, expected_n) in CASES {
        let area = CrossSectionArea::new(area_mm2).expect("KB-114 fixture area is non-negative");
        let f = PeelForceCalculator::peel_force(sigma_kpa, area, f_v);
        assert!(
            (f.value() - expected_n).abs() < 0.01,
            "peel_force({sigma_kpa}, {area_mm2}, {f_v}) = {}, expected {expected_n} (KB-114)",
            f.value(),
        );
    }
}

// ---- helpers ----------------------------------------------------------------

fn mini_nanodlp_path() -> std::path::PathBuf {
    workspace_data_dir()
        .parent()
        .expect("workspace_data_dir has a repo-root parent")
        .join("crates/resinsim-core/tests/fixtures/mini.nanodlp")
}

/// A temp `--data-dir` (printers/ copied unchanged, resins/generic_standard.toml
/// with `base_adhesion_elevation_kpa` REMOVED). Parse-then-rewrite via
/// `toml::Table`, not a string-append/strip — robust to trailing-whitespace
/// or line-ending edits in the source TOML, matching the pattern already
/// used for the KB-153 measured-Ea fixture in
/// cli_temperature_flag_validation.rs.
fn resin_data_dir_without_base_adhesion() -> std::path::PathBuf {
    let tmpdir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("uat-no-base-adhesion");
    let resins = tmpdir.join("resins");
    let printers = tmpdir.join("printers");
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&resins).expect("mkdir resins");
    std::fs::create_dir_all(&printers).expect("mkdir printers");
    let src_dir = workspace_data_dir();
    for entry in std::fs::read_dir(src_dir.join("printers")).expect("readdir printers") {
        let e = entry.expect("entry");
        std::fs::copy(e.path(), printers.join(e.file_name())).expect("copy printer");
    }
    let src_toml = std::fs::read_to_string(src_dir.join("resins/generic_standard.toml"))
        .expect("read generic_standard");
    let mut parsed: toml::Table =
        toml::from_str(&src_toml).expect("source generic_standard.toml must be valid TOML");
    parsed.remove("base_adhesion_elevation_kpa");
    let patched =
        toml::to_string(&parsed).expect("serialise patched generic_standard.toml back to TOML");
    std::fs::write(resins.join("generic_standard.toml"), &patched).expect("write patched toml");
    let profile: resinsim_core::entities::ResinProfile =
        toml::from_str(&patched).expect("patched TOML must round-trip back to ResinProfile");
    assert_eq!(
        profile.base_adhesion_elevation_kpa(),
        None,
        "fixture invariant: base_adhesion_elevation_kpa must be absent after patching"
    );
    tmpdir
}
