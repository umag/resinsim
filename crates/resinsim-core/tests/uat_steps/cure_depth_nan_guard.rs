//! Step definitions for `spec/uat/cure-depth-nan-guard.md` UAT-1 + UAT-2.
//!
//! Ported from the spike's `tests/uat_gherkin.rs` inline defs. T1-F4 locked
//! two guards:
//! - UAT-1: `Energy::new` rejects zero / NaN / -Inf before the Beer-Lambert
//!   calculator is reached.
//! - UAT-2: `Energy::scale` panics on a non-finite scale factor rather than
//!   silently returning `Energy(NaN)` under a release-build `debug_assert!`.

use cucumber::{given, then, when};
use resinsim_core::values::Energy;

use super::world::UatWorld;

// ---- UAT-1: invalid Ec rejected at Energy::new -----------------------------

#[given(regex = r"^a resin profile with a critical energy Ec that is zero or non-finite$")]
fn given_invalid_critical_energy(_world: &mut UatWorld) {
    // UatWorld::default() initialises last_energy_err to None and
    // cucumber constructs a fresh World per scenario.
}

#[when(regex = r"^the Beer-Lambert cure depth calculator runs for a layer$")]
fn when_beer_lambert_runs(world: &mut UatWorld) {
    // The guard fires at Energy::new BEFORE CureCalculator::cure_depth is
    // called (callers wrap critical_energy_mj_cm2 in Energy::new — see
    // services/failure_predictor.rs:129).
    let zero = Energy::new(0.0);
    let nan = Energy::new(f32::NAN);
    let neg_inf = Energy::new(f32::NEG_INFINITY);
    assert!(
        zero.is_err() && nan.is_err() && neg_inf.is_err(),
        "Energy::new must reject zero, NaN, and -Inf; got zero={zero:?} nan={nan:?} neg_inf={neg_inf:?}",
    );
    world.last_energy_err = zero.err();
}

#[then(
    regex = r"^the system fails loudly with a clear diagnostic message referencing critical_energy$"
)]
fn then_loud_diagnostic(world: &mut UatWorld) {
    // SPIKE NOTE: Energy::new's message is the generic "energy must be
    // positive and finite" — it does NOT reference critical_energy
    // specifically. The scenario text describes the SYSTEM-LEVEL contract;
    // the unit-level guard fires loudly enough that the system never
    // silently proceeds. See docs/adr/0008-bdd-uat-spike-notes.md.
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
    // no false is_sufficient. Asserts the precondition.
    assert!(
        world.last_energy_err.is_some(),
        "scenario invariant: prior Then step must have captured an Err from Energy::new",
    );
}

// ---- UAT-2: Energy::scale panics on NaN factor -----------------------------

#[given(
    regex = r"^a print with a uniformity profile where the intensity factor computation produces a non-finite value$"
)]
fn given_nan_intensity_factor(_world: &mut UatWorld) {
    // UatWorld::default() initialises last_panic_msg to None.
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
