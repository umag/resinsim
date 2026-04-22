//! Edge-case regression tests for the adversarial-review HIGH findings on the
//! SimulationRunner / InitialLedTemperature public API boundary (ADR-0007
//! follow-on).
//!
//! The fix introduced `InitialLedTemperature` as a validating newtype, so
//! NaN / below-absolute-zero / infinite values now fail at construction rather
//! than panicking inside `ThermalCalculator::led_temperature_at_time`. These
//! tests lock in that behaviour.

use resinsim_core::entities::ResinProfile;
use resinsim_core::values::InitialLedTemperature;

#[test]
fn initial_led_temperature_new_rejects_nan() {
    let err = InitialLedTemperature::new(f32::NAN).expect_err("NaN must fail");
    assert!(err.contains("finite"), "error must mention finite: {err}");
}

#[test]
fn initial_led_temperature_new_rejects_below_absolute_zero() {
    let err = InitialLedTemperature::new(-300.0)
        .expect_err("-300 °C must fail (below absolute zero)");
    assert!(
        err.contains("absolute zero"),
        "error must mention absolute zero: {err}"
    );
}

#[test]
fn initial_led_temperature_new_rejects_positive_infinity() {
    assert!(InitialLedTemperature::new(f32::INFINITY).is_err());
}

#[test]
fn initial_led_temperature_new_rejects_negative_infinity() {
    assert!(InitialLedTemperature::new(f32::NEG_INFINITY).is_err());
}

#[test]
fn resin_profile_factory_passes_tightened_validate() {
    // The `reference_temp_c > -273.15` bound added for the step-10 Ec(T)
    // hardening must still accept the shipped factory profiles. Mutation-based
    // rejection tests (reference_temp_c = -400, -273.15) live in the
    // resin_profile.rs unit tests; pub(crate) fields can't be mutated from
    // this integration-tests crate.
    ResinProfile::generic_standard()
        .validate()
        .expect("generic_standard must pass tightened validate()");
    ResinProfile::elegoo_ceramic_grey_v2()
        .validate()
        .expect("elegoo_ceramic_grey_v2 must pass tightened validate()");
}
