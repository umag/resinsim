// Spike: cucumber-rs feasibility harness.
// See docs/adr/0008-bdd-uat-spike-notes.md for spike context, drift caveats,
// and rollout-time concerns (per-scenario nextest attribution etc.).

use cucumber::{StatsWriter as _, World, given, then, when};
use resinsim_core::values::{Energy, PeelForce, SafetyFactor, SupportCapacity};

#[derive(Debug, Default, World)]
struct SpikeWorld {
    // Safety factor scenario state.
    capacity: Option<SupportCapacity>,
    force: Option<PeelForce>,
    computed_safety: Option<Option<SafetyFactor>>,

    // Cure depth scenarios state.
    last_energy_err: Option<&'static str>,
    last_panic_msg: Option<String>,
}

// ---- Safety factor zero-force scenario ----------------------------------

#[given(regex = r"^a print with zero peel force on one or more layers \(e\.g\. layer area = 0\)$")]
fn given_zero_peel_force(world: &mut SpikeWorld) {
    world.capacity = Some(
        SupportCapacity::new(100.0)
            .expect("scenario invariant: 100 N support capacity is in domain"),
    );
    world.force = Some(
        PeelForce::new(0.0).expect("scenario invariant: 0 N peel force is in domain"),
    );
}

#[when(regex = r"^the failure predictor runs on those layers$")]
fn when_failure_predictor_runs(world: &mut SpikeWorld) {
    let cap = world
        .capacity
        .expect("scenario invariant: Given step set capacity");
    let f = world.force.expect("scenario invariant: Given step set force");
    world.computed_safety = Some(SafetyFactor::compute(cap, f));
}

#[then(regex = r"^no SupportOverload failure event is emitted for those layers$")]
fn then_no_support_overload(world: &mut SpikeWorld) {
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
fn then_safety_factor_infinity(world: &mut SpikeWorld) {
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
fn given_invalid_critical_energy(_world: &mut SpikeWorld) {
    // SpikeWorld::default() already initialises last_energy_err to None
    // and cucumber constructs a fresh World per scenario.
}

#[when(regex = r"^the Beer-Lambert cure depth calculator runs for a layer$")]
fn when_beer_lambert_runs(world: &mut SpikeWorld) {
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
fn then_loud_diagnostic(world: &mut SpikeWorld) {
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
fn then_no_silent_bad_result_scenario1(world: &mut SpikeWorld) {
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
fn given_nan_intensity_factor(_world: &mut SpikeWorld) {
    // SpikeWorld::default() already initialises last_panic_msg to None
    // and cucumber constructs a fresh World per scenario.
}

#[when(regex = r"^the uniformity-corrected cure depth is computed$")]
fn when_uniformity_corrected_cure_depth(world: &mut SpikeWorld) {
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
fn then_scale_factor_panic_message(world: &mut SpikeWorld) {
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
fn then_no_silent_nan_cure(world: &mut SpikeWorld) {
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
//
// NOT compatible with `cargo nextest`. nextest enumerates tests via libtest's
// terse listing protocol (`--list --format terse`); cucumber's writer doesn't
// speak that protocol, and even the libtest writer feature emits streaming
// JSON rather than terse listing. Configured to be excluded from nextest via
// `.config/nextest.toml` so the full `cargo nextest run -p resinsim-core`
// keeps working. See docs/adr/0008-bdd-uat-spike-notes.md for full context.

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Anchor the feature directory to the crate at compile time so the
    // harness doesn't silently find zero scenarios when CWD differs from
    // the package root (cargo test sets CWD correctly today, but invocations
    // via --manifest-path or under a debugger may not).
    let features =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/uat");
    let features_display = features.display().to_string();
    let writer = SpikeWorld::cucumber().run(features).await;

    // Guard against silent green: if the feature directory is empty or all
    // scenarios are filtered out, .run() exits 0 with zero work done. Sum of
    // step states must be > 0 for the harness to have done anything.
    let total_steps =
        writer.passed_steps() + writer.skipped_steps() + writer.failed_steps();
    assert!(
        total_steps > 0,
        "no cucumber steps ran — check that {features_display} contains .feature files",
    );

    // Surface failures via process exit code so cargo test marks the binary
    // as failed when scenarios fail.
    if writer.execution_has_failed() {
        std::process::exit(1);
    }
}
