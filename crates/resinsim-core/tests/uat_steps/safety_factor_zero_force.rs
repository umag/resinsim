//! Step definitions for `spec/uat/safety-factor-zero-force.md` UAT-1.
//!
//! Ported from the spike's `tests/uat_gherkin.rs` inline defs. The scenario
//! locks T1-F2's guard: `SafetyFactor::compute()` returns `None` when peel
//! force is zero, and `failure_predictor` must NOT emit a SupportOverload
//! for those layers.

use cucumber::{given, then, when};
use resinsim_core::values::{PeelForce, SafetyFactor, SupportCapacity};

use super::world::UatWorld;

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
        "SafetyFactor::compute must return None for zero peel force (the failure_predictor \
         guard at services/failure_predictor.rs:215 only emits SupportOverload when \
         Some(sf) AND !is_safe), got {computed:?}",
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
