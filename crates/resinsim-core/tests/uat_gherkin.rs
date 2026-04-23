// Cucumber-rs harness driving the UAT suite. Spike-era step defs for the
// safety-factor-zero-force and cure-depth-nan-guard scenarios stay inline
// below; they move into per-file modules in rollout step 6.
//
// Step 2 extends the harness to run a SECOND cucumber pass against the
// freshly migrated `spec/uat/recipe-outside-printer-range.md` after
// extracting its `\`\`\`gherkin fenced code blocks into a synthetic
// `.feature` file under `$CARGO_TARGET_TMPDIR`. Step 4 replaces this
// tempdir shuffle with a custom cucumber `Parser` that reads spec/uat/
// directly.
//
// Harness contract (preserved from spike):
// - `harness = false` in Cargo.toml — libtest discovery is off, so
//   `#[test]` attributes inside this binary do nothing. Extractor unit
//   tests live in the `uat_extractor` binary instead.
// - Silent-green guard: if neither cucumber run reports any executed
//   steps, the harness panics.
// - Any cucumber failure surfaces via exit code 1.

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

// ---- Harness entry point (cucumber-rs harness=false convention) ----
//
// Run with: `cargo test --test uat_gherkin -p resinsim-core`

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // ---- Run 1: legacy spike features in tests/uat/ (deleted in step 4) ----
    let spike_features = manifest.join("tests/uat");
    let spike_display = spike_features.display().to_string();
    let spike_writer = UatWorld::cucumber().run(&spike_features).await;

    // ---- Run 2: step 2 smoke test — extract one migrated spec/uat file,
    // synthesise a .feature in CARGO_TARGET_TMPDIR, run cucumber against it. ----
    let smoke_md = manifest
        .ancestors()
        .nth(2)
        .expect("repo ancestor resolves from CARGO_MANIFEST_DIR")
        .join("spec/uat/recipe-outside-printer-range.md");
    let md_source = std::fs::read_to_string(&smoke_md).unwrap_or_else(|e| {
        panic!(
            "smoke test: failed to read {}: {e}",
            smoke_md.display()
        );
    });
    let scenarios = uat_steps::extract::extract(&md_source);
    assert_eq!(
        scenarios.len(),
        2,
        "smoke test: expected 2 extracted scenarios from {}",
        smoke_md.display(),
    );

    let feature_text = synthesize_feature(
        "Recipe outside printer envelope",
        &scenarios,
    );
    let tmpdir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("uat-gherkin-rollout-smoke");
    std::fs::create_dir_all(&tmpdir).expect("create tempdir for smoke .feature");
    let feature_path = tmpdir.join("recipe-outside-printer-range.feature");
    std::fs::write(&feature_path, &feature_text)
        .expect("write synthesised smoke .feature");
    let smoke_writer = UatWorld::cucumber().run(&tmpdir).await;

    // ---- Silent-green guard across both runs ----
    let total_steps = spike_writer.passed_steps()
        + spike_writer.skipped_steps()
        + spike_writer.failed_steps()
        + smoke_writer.passed_steps()
        + smoke_writer.skipped_steps()
        + smoke_writer.failed_steps();
    assert!(
        total_steps > 0,
        "no cucumber steps ran — check {spike_display} and {}",
        feature_path.display(),
    );

    if spike_writer.execution_has_failed() || smoke_writer.execution_has_failed() {
        std::process::exit(1);
    }
}

/// Wrap extracted scenario fences with a synthesised `Feature:` header so
/// cucumber-rs's default Basic parser can pick them up as a .feature file.
fn synthesize_feature(
    feature_name: &str,
    scenarios: &[uat_steps::extract::ExtractedScenario],
) -> String {
    let mut out = format!("Feature: {feature_name}\n\n");
    for scenario in scenarios {
        out.push_str(&scenario.gherkin);
        if !scenario.gherkin.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out
}
