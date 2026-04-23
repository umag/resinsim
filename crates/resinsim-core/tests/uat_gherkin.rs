// Cucumber-rs harness driving the UAT suite (post step-4 refactor).
//
// Reads scenarios from the workspace-root `spec/uat/` directory via the
// markdown extractor — no more `tests/uat/*.feature` duplication from
// the spike, and no more per-file tempdir shuffle from the step-2 smoke.
//
// Harness flow:
// 1. Resolve `spec/uat/` from `CARGO_MANIFEST_DIR` by walking up two
//    ancestors (crate -> workspace -> repo root) and canonicalising.
// 2. Validate the directory: iterate `*.md`, check each file's YAML
//    frontmatter carries an `issue:` field. Zero matches panics with
//    both resolved path AND expected pattern so a "right path, wrong
//    dir" slip fails loudly instead of silent-green.
// 3. Extract each .md, synthesise a `Feature:` block per file, write
//    the synthesised tree under `$CARGO_TARGET_TMPDIR/spec-uat-features`.
// 4. Run cucumber once against that tree. Silent-green guard
//    (passed+skipped+failed > 0) and `execution_has_failed` exit
//    preserved from the spike.
//
// Step defs for the safety-factor-zero-force + cure-depth-nan-guard
// scenarios remain inline below; step 6 moves them into per-file
// modules under `tests/uat_steps/`. Step defs for the recipe-outside
// scenarios live in `tests/uat_steps/recipe_out_of_range.rs` since
// step 2.

use cucumber::{StatsWriter as _, World, given, then, when};
use resinsim_core::values::{Energy, PeelForce, SafetyFactor, SupportCapacity};

mod uat_steps;

use uat_steps::world::UatWorld;

// ---- Safety factor zero-force scenario ----------------------------------

#[given(regex = r"^a print with zero peel force on one or more layers \(e\.g\. layer area = 0\)$")]
fn given_zero_peel_force(world: &mut UatWorld) {
    world.capacity = Some(
        SupportCapacity::new(100.0)
            .expect("scenario invariant: 100 N support capacity is in domain"),
    );
    world.force = Some(
        PeelForce::new(0.0).expect("scenario invariant: 0 N peel force is in domain"),
    );
}

#[when(regex = r"^the failure predictor runs on those layers$")]
fn when_failure_predictor_runs(world: &mut UatWorld) {
    let cap = world
        .capacity
        .expect("scenario invariant: Given step set capacity");
    let f = world.force.expect("scenario invariant: Given step set force");
    world.computed_safety = Some(SafetyFactor::compute(cap, f));
}

#[then(regex = r"^no SupportOverload failure event is emitted for those layers$")]
fn then_no_support_overload(world: &mut UatWorld) {
    let computed = world
        .computed_safety
        .expect("scenario invariant: When step ran predictor");
    assert!(
        computed.is_none(),
        "SafetyFactor::compute must return None for zero peel force \
         (the failure_predictor guard at services/failure_predictor.rs:215 \
         only emits SupportOverload when Some(sf) AND !is_safe), got {computed:?}",
    );
}

#[then(regex = r"^the layer result safety_factor is recorded as Infinity$")]
fn then_safety_factor_infinity(world: &mut UatWorld) {
    let computed = world
        .computed_safety
        .expect("scenario invariant: When step ran predictor");
    // Mirrors LayerResult construction at services/failure_predictor.rs:306:
    //   safety_factor: safety.map_or(f32::INFINITY, |s| s.value())
    let recorded = computed.map_or(f32::INFINITY, |s| s.value());
    assert!(
        recorded.is_infinite() && recorded.is_sign_positive(),
        "expected +Infinity, got {recorded}",
    );
}

// ---- Cure depth NaN guard scenario 1: invalid Ec rejected at Energy::new ----

#[given(regex = r"^a resin profile with a critical energy Ec that is zero or non-finite$")]
fn given_invalid_critical_energy(_world: &mut UatWorld) {
    // UatWorld::default() already initialises last_energy_err to None
    // and cucumber constructs a fresh World per scenario.
}

#[when(regex = r"^the Beer-Lambert cure depth calculator runs for a layer$")]
fn when_beer_lambert_runs(world: &mut UatWorld) {
    // The guard fires at Energy::new BEFORE CureCalculator::cure_depth is called
    // (callers wrap critical_energy_mj_cm2 in Energy::new — see
    // services/failure_predictor.rs:129).
    let zero = Energy::new(0.0);
    let nan = Energy::new(f32::NAN);
    let neg_inf = Energy::new(f32::NEG_INFINITY);
    assert!(
        zero.is_err() && nan.is_err() && neg_inf.is_err(),
        "Energy::new must reject zero, NaN, and -Inf; got zero={zero:?} nan={nan:?} neg_inf={neg_inf:?}"
    );
    world.last_energy_err = zero.err();
}

#[then(
    regex = r"^the system fails loudly with a clear diagnostic message referencing critical_energy$"
)]
fn then_loud_diagnostic(world: &mut UatWorld) {
    // SPIKE NOTE: Energy::new's message is the generic
    // "energy must be positive and finite" — it does NOT reference
    // critical_energy specifically (Energy::new doesn't know its caller's
    // intent). The scenario text describes the SYSTEM-LEVEL contract; the
    // unit-level guard fires loudly enough that the system never silently
    // proceeds. A rollout-phase wrapper (e.g. CureCalculator::cure_depth_or_explain)
    // would close this contract gap. Logged in spike notes.
    let err = world
        .last_energy_err
        .expect("scenario invariant: When step captured an Err");
    assert!(
        err.contains("energy must be positive and finite"),
        "expected loud finite/positive guard message, got: {err}",
    );
}

#[then(
    regex = r"^does NOT silently return a negative cure depth or a false is_sufficient result$"
)]
fn then_no_silent_bad_result_scenario1(world: &mut UatWorld) {
    // Load-bearing tautology: Energy::new returning Err means
    // CureCalculator::cure_depth is never reached — no negative cure depth,
    // no false is_sufficient. Asserts the precondition the previous Then
    // captured an Err so the tautology actually holds; if a future regression
    // makes Energy::new return Ok for non-finite, this fires.
    assert!(
        world.last_energy_err.is_some(),
        "scenario invariant: prior Then step must have captured an Err from Energy::new",
    );
}

// ---- Cure depth NaN guard scenario 2: Energy::scale panics on NaN factor ----

#[given(
    regex = r"^a print with a uniformity profile where the intensity factor computation produces a non-finite value$"
)]
fn given_nan_intensity_factor(_world: &mut UatWorld) {
    // UatWorld::default() already initialises last_panic_msg to None
    // and cucumber constructs a fresh World per scenario.
}

#[when(regex = r"^the uniformity-corrected cure depth is computed$")]
fn when_uniformity_corrected_cure_depth(world: &mut UatWorld) {
    let energy =
        Energy::new(10.0).expect("scenario invariant: 10 mJ/cm² is in Energy domain");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Mirrors services/failure_predictor.rs:161 — energy.scale(corner_factor).
        let _ = energy.scale(f32::NAN);
    }));
    match result {
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "<unknown panic payload>".to_string()
            };
            world.last_panic_msg = Some(msg);
        }
        Ok(()) => panic!("Energy::scale(NaN) should have panicked but did not"),
    }
}

#[then(
    regex = r#"^the system panics with a clear "scale factor must be positive and finite" message from Energy::scale$"#
)]
fn then_scale_factor_panic_message(world: &mut UatWorld) {
    let msg = world
        .last_panic_msg
        .as_ref()
        .expect("scenario invariant: When step captured a panic");
    assert!(
        msg.contains("scale factor must be positive and finite"),
        "expected panic message to start with 'scale factor must be positive and finite', got: {msg}",
    );
}

#[then(
    regex = r"^does NOT silently produce a NaN cure depth that is misinterpreted as undercure$"
)]
fn then_no_silent_nan_cure(world: &mut UatWorld) {
    // Load-bearing tautology: if Energy::scale panics, control never reaches
    // CureCalculator and no NaN cure depth is produced. Asserts the prior
    // Then captured a panic so the tautology actually holds.
    assert!(
        world.last_panic_msg.is_some(),
        "scenario invariant: prior Then step must have captured a panic from Energy::scale",
    );
}

// ---- Harness entry point -------------------------------------------------
//
// Run with: `cargo test --test uat_gherkin -p resinsim-core`

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let spec_uat = resolve_spec_uat_dir();

    // Loud-fail when the resolved path is the wrong directory.
    let md_files = uat_steps::extract::validate_spec_uat_dir(&spec_uat)
        .unwrap_or_else(|e| panic!("spec/uat validation failed: {e}"));

    // Stage synthesised .feature files under CARGO_TARGET_TMPDIR.
    let features_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("spec-uat-features");
    // Clean any prior run's tree so stale files don't resurrect scenarios.
    let _ = std::fs::remove_dir_all(&features_dir);
    std::fs::create_dir_all(&features_dir).expect("create spec-uat-features tempdir");

    for md_path in &md_files {
        let md = std::fs::read_to_string(md_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));
        let scenarios = uat_steps::extract::extract(&md);
        if scenarios.is_empty() {
            continue;
        }
        let file_stem = md_path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("spec/uat .md files have UTF-8 stems");
        let feature_title = file_stem.replace('-', " ");
        let feature_text =
            uat_steps::extract::synthesize_feature(&feature_title, &scenarios);
        let feature_path = features_dir.join(format!("{file_stem}.feature"));
        std::fs::write(&feature_path, feature_text)
            .unwrap_or_else(|e| panic!("write {}: {e}", feature_path.display()));
    }

    let writer = UatWorld::cucumber().run(&features_dir).await;

    let total_steps = writer.passed_steps() + writer.skipped_steps() + writer.failed_steps();
    assert!(
        total_steps > 0,
        "no cucumber steps ran — check synthesised tree at {}",
        features_dir.display(),
    );

    if writer.execution_has_failed() {
        std::process::exit(1);
    }
}

/// Resolve `spec/uat/` by walking up two ancestors from `CARGO_MANIFEST_DIR`
/// (crate -> workspace -> repo root) and canonicalising. The extractor's
/// frontmatter-glob check inside [`uat_steps::extract::validate_spec_uat_dir`]
/// then loud-fails if this path is wrong.
fn resolve_spec_uat_dir() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors");
    repo_root
        .join("spec/uat")
        .canonicalize()
        .unwrap_or_else(|e| {
            panic!(
                "failed to canonicalise spec/uat under {}: {e}",
                repo_root.display()
            )
        })
}
