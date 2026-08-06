//! Step definitions for
//! `spec/uat/calibration-disclosure-3of3-predicate.md` (uat-unskip-band-d
//! step 7) — UAT-1, UAT-2, and the tree's ONLY Scenario Outline (UAT-3,
//! 3 authored Examples rows expanding to 3 runtime scenarios; 5 runtime
//! scenarios total for this module). FIELD-SIM-GATED: the sole producer of
//! `FailureType::WarpingRisk` is `FailurePredictor::predict_strain_failures`
//! (`#[cfg(feature = "field-sim")]`, `failure_predictor.rs`), which
//! consumes `&StrainField` / `&StressField` — themselves
//! `#[cfg(feature = "field-sim")]` re-exports (`values/mod.rs`). So this
//! module compiles only under `cargo uat-field-sim`; its `pub mod` line in
//! `uat_steps/mod.rs` carries the matching
//! `#[cfg(feature = "field-sim")]` attribute, and its `use` entry lives in
//! the SAME second, gated `use uat_steps::{...}` block
//! `honest_zero_yield_fraction_on_calibrated_solid` already uses (both
//! modules share one gate). See docs/patterns/band-membership-by-symbol.md.
//!
//! **Why a simulation run cannot drive this spec** (per the plan, and per
//! `failure_predictor.rs`'s own model-gap caveat): `WarpingRisk` only fires
//! above `YIELD_FRACTION_WARN_THRESHOLD`, and the honest-zero spec
//! (`honest_zero_yield_fraction_on_calibrated_solid.rs`, this same
//! increment) locks `voxel_yield_fraction == Some(0.0)` on every layer of
//! the natural `generic_standard` solid-mask fixture — a real
//! `SimulationRunner` run would never emit `WarpingRisk` at all. Instead,
//! this module drives `FailurePredictor::predict_strain_failures` DIRECTLY
//! against a SYNTHESISED all-zero `StrainField` plus a `StressField`
//! carrying exactly one voxel above `tensile_strength_mpa` (35 MPa on the
//! `ResinBuilder` default) — the same construction shape
//! `failure_predictor.rs`'s own `predict_strain_failures` unit tests use
//! (`fields_2x2x1` + a single `accumulate_at` yielding tensor), reused here
//! through the public API from the integration-test crate.
//!
//! Resin moduli fields (`youngs_modulus_mpa`, `poissons_ratio`,
//! `shrinkage_anisotropy_z_ratio`) are `pub(crate)` on `ResinProfile`, so
//! TOML is the only construction route from this crate — `ResinBuilder`'s
//! three new setters (`world.rs`) OMIT the TOML key entirely when unset,
//! never emitting a default that would silently satisfy the 3-of-3
//! predicate (docs/patterns/anti/fixture-copy-of-shared-builder.md).

use cucumber::{given, then, when};
use resinsim_core::entities::FailureType;
use resinsim_core::services::failure_predictor::FailurePredictor;
use resinsim_core::values::{StrainField, StressField, StressTensor};

use super::world::{ResinBuilder, UatWorld};

/// KB-163 / KB-164 canonical calibrated values — mirror
/// `ResinProfile::generic_standard()` exactly (E = 2000 MPa, ν = 0.35,
/// z_ratio = 1.5), matching the spec text's own literals.
const CALIBRATED_YOUNGS_MODULUS_MPA: f32 = 2000.0;
const CALIBRATED_POISSONS_RATIO: f32 = 0.35;
const CALIBRATED_SHRINKAGE_ANISOTROPY_Z_RATIO: f32 = 1.5;

fn pending_builder(world: &mut UatWorld) -> ResinBuilder {
    world.resin_builder_pending.take().unwrap_or_default()
}

// ---- UAT-1: z_ratio unset while E + ν explicit still fires the caveat ----

#[given(regex = r"^a resin profile with youngs_modulus_mpa = 2000$")]
fn given_youngs_modulus_2000(world: &mut UatWorld) {
    let b = pending_builder(world).with_youngs_modulus_mpa(CALIBRATED_YOUNGS_MODULUS_MPA);
    world.resin_builder_pending = Some(b);
}

#[given(regex = r"^poissons_ratio = 0\.35$")]
fn given_poissons_ratio_035(world: &mut UatWorld) {
    let b = pending_builder(world).with_poissons_ratio(CALIBRATED_POISSONS_RATIO);
    world.resin_builder_pending = Some(b);
}

#[given(regex = r"^shrinkage_anisotropy_z_ratio is unset$")]
fn given_shrinkage_anisotropy_z_ratio_unset(world: &mut UatWorld) {
    // No-op: a fresh ResinBuilder already omits this field by default: see
    // `pending_builder` above. The step exists so the Gherkin text is
    // explicit about the scenario's precondition.
    let b = pending_builder(world);
    world.resin_builder_pending = Some(b);
}

// ---- UAT-2: all three moduli Some suppresses the caveat -------------------

#[given(regex = r"^shrinkage_anisotropy_z_ratio = 1\.5$")]
fn given_shrinkage_anisotropy_z_ratio_15(world: &mut UatWorld) {
    let b = pending_builder(world)
        .with_shrinkage_anisotropy_z_ratio(CALIBRATED_SHRINKAGE_ANISOTROPY_Z_RATIO);
    world.resin_builder_pending = Some(b);
}

// ---- UAT-3 (Scenario Outline): any single missing modulus fires the caveat

#[given(
    regex = r"^a resin profile where (youngs_modulus_mpa|poissons_ratio|shrinkage_anisotropy_z_ratio) is unset$"
)]
fn given_field_unset(world: &mut UatWorld, field: String) {
    world.calibration_disclosure_unset_field = Some(field);
}

#[given(regex = r"^the other two calibrated-moduli fields are explicit$")]
fn given_other_two_explicit(world: &mut UatWorld) {
    let unset = world
        .calibration_disclosure_unset_field
        .clone()
        .expect("scenario invariant: preceding Given named the unset field");
    let mut b = pending_builder(world);
    if unset != "youngs_modulus_mpa" {
        b = b.with_youngs_modulus_mpa(CALIBRATED_YOUNGS_MODULUS_MPA);
    }
    if unset != "poissons_ratio" {
        b = b.with_poissons_ratio(CALIBRATED_POISSONS_RATIO);
    }
    if unset != "shrinkage_anisotropy_z_ratio" {
        b = b.with_shrinkage_anisotropy_z_ratio(CALIBRATED_SHRINKAGE_ANISOTROPY_Z_RATIO);
    }
    world.resin_builder_pending = Some(b);
}

// ---- shared When: "the strain/stress pipeline emits a WarpingRisk event" --

#[when(regex = r"^the strain/stress pipeline emits a WarpingRisk event$")]
fn when_pipeline_emits_warping_risk(world: &mut UatWorld) {
    let resin = pending_builder(world).build();

    // Synthesised all-zero StrainField (no `lock_strain_at` calls — the
    // whole field stays the zero-strain default) plus a StressField
    // carrying exactly ONE voxel above `tensile_strength_mpa` (35 MPa
    // default) — same 2x2x1 shape `failure_predictor.rs`'s own
    // `predict_strain_failures` unit tests use.
    let strain = StrainField::new(2, 2, 1, 0.5, [0.0; 3])
        .expect("2x2x1 StrainField at 0.5mm voxel is a valid construction");
    let mut stress = StressField::new(2, 2, 1, 0.5, [0.0; 3])
        .expect("2x2x1 StressField at 0.5mm voxel is a valid construction");
    let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        .expect("a purely-hydrostatic 50 MPa stress tensor is a valid construction");
    stress
        .accumulate_at(0, 0, 0, yielded)
        .expect("(0,0,0) is in-bounds for a 2x2x1 field");

    let failures = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
    world.calibration_disclosure_failures = Some(failures);
}

// ---- Then steps -------------------------------------------------------------

fn warping_risk_message(world: &UatWorld) -> &str {
    world
        .calibration_disclosure_failures
        .as_ref()
        .expect("scenario invariant: When step populated calibration_disclosure_failures")
        .iter()
        .find(|e| e.failure_type == FailureType::WarpingRisk)
        .expect(
            "scenario invariant violated: the synthesised yielded voxel must produce a \
             WarpingRisk event",
        )
        .message
        .as_str()
}

#[then(regex = r#"^the FailureEvent\.message contains "uncalibrated moduli"$"#)]
fn then_message_contains_uncalibrated_moduli(world: &mut UatWorld) {
    let message = warping_risk_message(world);
    assert!(
        message.contains("uncalibrated moduli"),
        "expected the caveat 'uncalibrated moduli' in the message; got: {message}"
    );
}

#[then(regex = r#"^the message cites both "KB-163" and "KB-164"$"#)]
fn then_message_cites_kb163_and_kb164(world: &mut UatWorld) {
    let message = warping_risk_message(world);
    assert!(
        message.contains("KB-163"),
        "expected the caveat to cite KB-163; got: {message}"
    );
    assert!(
        message.contains("KB-164"),
        "expected the caveat to cite KB-164; got: {message}"
    );
}

#[then(regex = r#"^the FailureEvent\.message does NOT contain "uncalibrated moduli"$"#)]
fn then_message_does_not_contain_uncalibrated_moduli(world: &mut UatWorld) {
    let message = warping_risk_message(world);
    assert!(
        !message.contains("uncalibrated moduli"),
        "expected NO 'uncalibrated moduli' caveat on a fully-calibrated resin; got: {message}"
    );
}
