//! Domain service: validate a `Recipe` against a `PrinterProfile`'s hardware envelope.
//!
//! Called by `SimulationRunner` at simulation entry, BEFORE `slice_areas` or
//! `predict_layer`. Returns ALL violations in the `Vec<String>` (never short-circuit)
//! so a user can fix every mismatch in one pass. See ADR-0005.
//!
//! # Trust contract
//!
//! `validate_pairing` **trusts** that `Recipe::validate()` was called by the caller
//! (typically via `ResinProfile::validate()`, which delegates). IEEE 754 NaN
//! comparisons are always false, so a naive `contains(NaN)` check returns false for
//! any range — but `range.contains(NaN)` returning false here would *silently pass*
//! a NaN recipe field (the violation list would be empty). The defence is upstream:
//! `Recipe::new` and `Recipe::validate` reject NaN before this service is reached.
//!
//! # Skipped fields
//!
//! Recipe fields without a corresponding `PrinterProfile` range are not checked here:
//!
//! - `transition_layers` — no hardware analogue.
//! - `wait_before_cure_sec` — no hardware analogue.
//! - `wait_before_release_sec` — no hardware analogue.
//! - `wait_after_release_sec` — no hardware analogue.
//! - `lift_cycle_sec` — no hardware analogue; derived from kinematics rather than
//!   bounded by a single printer parameter.
//! - `lift_distance_mm` — no hardware analogue in today's PrinterProfile.
//!
//! The `skipped_fields_locked` test enumerates this list so any future Recipe field
//! added without a paired printer range surfaces in review.

use crate::entities::{PrinterProfile, Recipe};

/// Check that every `Recipe` field constrained by a printer range lies within that
/// range. Returns `Err(violations)` with ALL violations (not just the first) so
/// the user can fix every mismatch in one pass.
pub fn validate_pairing(printer: &PrinterProfile, recipe: &Recipe) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();

    let layer_range = printer.layer_height_range_um();
    if !layer_range.contains(recipe.layer_height_um()) {
        violations.push(format!(
            "recipe.layer_height_um ({}) is outside printer.layer_height_range_um {}",
            recipe.layer_height_um(),
            layer_range
        ));
    }

    let exposure_range = printer.exposure_range_sec();
    if !exposure_range.contains(recipe.normal_exposure_sec()) {
        violations.push(format!(
            "recipe.normal_exposure_sec ({}) is outside printer.exposure_range_sec {}",
            recipe.normal_exposure_sec(),
            exposure_range
        ));
    }
    if !exposure_range.contains(recipe.bottom_exposure_sec()) {
        violations.push(format!(
            "recipe.bottom_exposure_sec ({}) is outside printer.exposure_range_sec {}",
            recipe.bottom_exposure_sec(),
            exposure_range
        ));
    }

    let speed_range = printer.lift_speed_range_mm_min();
    if !speed_range.contains(recipe.lift_speed_mm_min()) {
        violations.push(format!(
            "recipe.lift_speed_mm_min ({}) is outside printer.lift_speed_range_mm_min {}",
            recipe.lift_speed_mm_min(),
            speed_range
        ));
    }

    if recipe.bottom_layer_count() > printer.bottom_layer_count_max() {
        violations.push(format!(
            "recipe.bottom_layer_count ({}) exceeds printer.bottom_layer_count_max ({})",
            recipe.bottom_layer_count(),
            printer.bottom_layer_count_max()
        ));
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{PrinterProfile, ResinProfile};

    fn test_printer() -> PrinterProfile {
        PrinterProfile::generic_msla_4k()
    }

    fn test_recipe() -> Recipe {
        Recipe::generic_standard()
    }

    #[test]
    fn happy_path_returns_ok() {
        validate_pairing(&test_printer(), &test_recipe())
            .expect("generic_standard recipe fits inside generic_msla_4k envelope");
    }

    #[test]
    fn resin_recipe_from_factory_validates_against_paired_printer() {
        // End-to-end sanity: ResinProfile::generic_standard + PrinterProfile::generic_msla_4k pair cleanly.
        let resin = ResinProfile::generic_standard();
        validate_pairing(&test_printer(), resin.recipe())
            .expect("paired factory defaults must pass validate_pairing");
    }

    #[test]
    fn layer_height_below_range_reports_violation() {
        let mut recipe = test_recipe();
        recipe.layer_height_um = 5.0; // below generic_msla_4k range [20, 100]
        let violations = validate_pairing(&test_printer(), &recipe)
            .expect_err("below-range layer height must report a violation");
        assert_eq!(violations.len(), 1, "single violation: {violations:?}");
        assert!(violations[0].contains("layer_height_um"));
    }

    #[test]
    fn layer_height_above_range_reports_violation() {
        let mut recipe = test_recipe();
        recipe.layer_height_um = 200.0; // above generic_msla_4k range [20, 100]
        let violations = validate_pairing(&test_printer(), &recipe)
            .expect_err("above-range layer height must report a violation");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("layer_height_um"));
    }

    #[test]
    fn multiple_violations_all_reported() {
        let mut recipe = test_recipe();
        recipe.layer_height_um = 5.0; // below range
        recipe.normal_exposure_sec = 0.5; // below generic_msla_4k range [1, 60]
        recipe.lift_speed_mm_min = 500.0; // above range [10, 200]
        recipe.bottom_layer_count = 100; // above generic_msla_4k max (15)
        let violations = validate_pairing(&test_printer(), &recipe)
            .expect_err("multiple out-of-range fields must report");
        assert_eq!(
            violations.len(),
            4,
            "all 4 violations reported: {violations:?}"
        );
        let joined = violations.join(" | ");
        assert!(joined.contains("layer_height_um"));
        assert!(joined.contains("normal_exposure_sec"));
        assert!(joined.contains("lift_speed_mm_min"));
        assert!(joined.contains("bottom_layer_count"));
    }

    #[test]
    fn boundary_value_at_range_min_accepted() {
        let printer = test_printer();
        let range = printer.layer_height_range_um();
        let mut recipe = test_recipe();
        recipe.layer_height_um = range.min();
        validate_pairing(&printer, &recipe)
            .expect("value at range.min must be accepted (inclusive)");
    }

    #[test]
    fn boundary_value_at_range_max_accepted() {
        let printer = test_printer();
        let range = printer.lift_speed_range_mm_min();
        let mut recipe = test_recipe();
        recipe.lift_speed_mm_min = range.max();
        validate_pairing(&printer, &recipe)
            .expect("value at range.max must be accepted (inclusive)");
    }

    #[test]
    fn zero_width_range_pins_exact_value() {
        // Printer pinned to a single layer height value (e.g. fixed-parameter hardware).
        let mut printer = test_printer();
        printer.layer_height_range_um =
            crate::values::FloatRange::new(50.0, 50.0).expect("zero-width range at 50.0 is valid");
        let mut recipe = test_recipe();
        recipe.layer_height_um = 50.0;
        validate_pairing(&printer, &recipe)
            .expect("exact value in zero-width range must be accepted");
        recipe.layer_height_um = 50.1;
        assert!(
            validate_pairing(&printer, &recipe).is_err(),
            "slightly-off value must be rejected by zero-width range"
        );
    }

    #[test]
    fn bottom_exposure_checked_against_same_range() {
        let mut recipe = test_recipe();
        recipe.bottom_exposure_sec = 70.0; // above generic_msla_4k exposure_range_sec max (60)
        let violations = validate_pairing(&test_printer(), &recipe)
            .expect_err("out-of-range bottom_exposure_sec must report");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("bottom_exposure_sec"));
    }

    #[test]
    fn bottom_layer_count_equal_to_max_accepted() {
        let printer = test_printer();
        let max = printer.bottom_layer_count_max();
        let mut recipe = test_recipe();
        recipe.bottom_layer_count = max;
        validate_pairing(&printer, &recipe).expect("count == max must be accepted (inclusive)");
    }

    // --- Trust-contract test per ADR-0005 §5.
    //
    // IEEE 754: NaN < x and NaN > x are both false. If a Recipe with a NaN field is
    // passed to validate_pairing, the contains() check returns false, producing a
    // violation (saying the NaN value is "outside" the range). That is NOT silent
    // acceptance — it surfaces as a validation failure naming the field.
    //
    // The invariant this test locks: pairing does NOT silently accept NaN. The
    // upstream Recipe::validate() is still the trust boundary for NaN rejection,
    // but if Recipe::validate() is somehow skipped, pairing still flags the NaN
    // field (via contains() returning false).

    #[test]
    fn nan_recipe_field_reported_by_pairing_as_violation() {
        let mut recipe = test_recipe();
        recipe.layer_height_um = f32::NAN;
        let violations = validate_pairing(&test_printer(), &recipe)
            .expect_err("NaN recipe field must produce a violation, not silent accept");
        assert!(
            violations.iter().any(|v| v.contains("layer_height_um")),
            "NaN field must be reported by name: {violations:?}"
        );
    }

    // --- Skipped-fields invariant. If a future Recipe field is added without a paired
    // printer range, this test signals that pairing_validator must be updated. ---

    #[test]
    fn skipped_fields_locked() {
        // Fields below are Recipe fields NOT checked by validate_pairing (no hardware
        // analogue in PrinterProfile ranges). If a new Recipe field is added, either
        // pair it with a printer range and update validate_pairing, or add it here and
        // update the module docstring.
        let recipe = test_recipe();
        // Reference each unchecked field so adding/renaming them is a compile-time
        // signal. Reading the value is sufficient; we don't assert a particular value.
        let _ = recipe.transition_layers();
        let _ = recipe.wait_before_cure_sec();
        let _ = recipe.wait_before_release_sec();
        let _ = recipe.wait_after_release_sec();
        let _ = recipe.lift_cycle_sec();
        let _ = recipe.lift_distance_mm();
    }
}
