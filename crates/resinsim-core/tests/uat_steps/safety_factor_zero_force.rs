//! Step definitions for `spec/uat/safety-factor-zero-force.md` UAT-1.
//!
//! **Step 9 — replaces the spike's tautology mirror.** The spike tested
//! the SafetyFactor::compute formula directly (test mirrors production);
//! this rewrite drives the scenario through
//! `FailurePredictor::predict_layer` end-to-end and asserts on the
//! actual `LayerResult` + emitted `FailureEvent` list. Anti-pattern at
//! `docs/patterns/anti/test-mirrors-production-formula.md` is closed
//! for THIS scenario; 34 other UAT scenarios still carry mirror-style
//! assertions and are tracked in the rollout outcome for future
//! migration.

use cucumber::{given, then, when};
use resinsim_core::entities::{FailureEvent, FailureType};
use resinsim_core::services::failure_predictor::FailurePredictor;

use super::world::{PredictLayerInputs, UatWorld};

#[given(regex = r"^a print with zero peel force on one or more layers \(e\.g\. layer area = 0\)$")]
fn given_zero_peel_force(world: &mut UatWorld) {
    // Zero area → zero cure energy arriving at the FEP → zero peel
    // force. PredictLayerInputs::default_for_test() starts from the
    // generic_msla_4k + generic_standard canonical fixtures; the
    // with_zero_area() builder flips both `area` and `prev_area` to 0
    // so `peel_force_n` falls to zero along the production path.
    world.predict_layer_result = None;
}

#[when(regex = r"^the failure predictor runs on those layers$")]
fn when_failure_predictor_runs(world: &mut UatWorld) {
    let inputs = PredictLayerInputs::default_for_test().with_zero_area();
    let layer_height_um = inputs.resin.recipe().layer_height_um();
    let (result, failures) = FailurePredictor::predict_layer(
        inputs.layer,
        inputs.area,
        inputs.prev_area,
        &inputs.overrides,
        &inputs.resin,
        &inputs.printer,
        inputs.resin.recipe(),
        layer_height_um,
        &inputs.supports,
        &inputs.plate,
        &inputs.thermal,
    );
    world.predict_layer_result = Some((result, failures));
}

#[then(regex = r"^no SupportOverload failure event is emitted for those layers$")]
fn then_no_support_overload(world: &mut UatWorld) {
    let (_, failures) = world
        .predict_layer_result
        .as_ref()
        .expect("scenario invariant: When step populated predict_layer_result");
    let support_overloads: Vec<&FailureEvent> = failures
        .iter()
        .filter(|f| f.failure_type == FailureType::SupportOverload)
        .collect();
    assert!(
        support_overloads.is_empty(),
        "expected zero SupportOverload events at zero peel force; got {support_overloads:?}",
    );
}

#[then(regex = r"^the layer result safety_factor is recorded as Infinity$")]
fn then_safety_factor_infinity(world: &mut UatWorld) {
    let (result, _) = world
        .predict_layer_result
        .as_ref()
        .expect("scenario invariant: When step populated predict_layer_result");
    assert!(
        result.safety_factor.is_infinite() && result.safety_factor.is_sign_positive(),
        "LayerResult.safety_factor must be +Infinity at zero peel force (failure_predictor's map_or(f32::INFINITY, |s| s.value()) line); got {}",
        result.safety_factor,
    );
    // Defence-in-depth: also assert the underlying peel force is zero.
    assert!(
        result.peel_force_n.abs() < 1e-6,
        "peel_force_n must be ~0 for zero-area input; got {}",
        result.peel_force_n,
    );
}
