//! Property tests for LayerTimingCalculator (ADR-0007 release mechanism branch,
//! KB-152 cumulative-time foundation).
//!
//! Targeted retract / tilt branch tests live as unit tests inside
//! `services::layer_timing_calculator` because they mutate `Recipe::retract_speed_mm_min`
//! (pub(crate)) — only in-crate tests can construct a Recipe with a non-default
//! retract speed. These proptests cover the public-API surface: cumulative-time
//! monotonicity and bottom-vs-normal ordering across random layer indices.
use proptest::prelude::*;
use resinsim_core::entities::{PrinterProfile, Recipe};
use resinsim_core::services::LayerTimingCalculator;

proptest! {
    /// Cumulative time is non-decreasing — scan of non-negative per-layer times.
    #[test]
    fn cumulative_time_monotonic_on_linear(
        total_layers in 2u32..300,
    ) {
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::generic_msla_4k();
        let times = LayerTimingCalculator::cumulative_times_sec(&recipe, &printer, total_layers);
        for w in times.windows(2) {
            prop_assert!(w[1] >= w[0] - 1e-6,
                "cumulative time decreased on Linear: {} -> {}", w[0], w[1]);
        }
    }

    /// Cumulative time is also non-decreasing on Tilt printers (different branch).
    #[test]
    fn cumulative_time_monotonic_on_tilt(
        total_layers in 2u32..300,
    ) {
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::elegoo_mars5_ultra();
        let times = LayerTimingCalculator::cumulative_times_sec(&recipe, &printer, total_layers);
        for w in times.windows(2) {
            prop_assert!(w[1] >= w[0] - 1e-6,
                "cumulative time decreased on Tilt: {} -> {}", w[0], w[1]);
        }
    }

    /// Bottom layers take longer than normal layers (bottom_exposure_sec >> normal).
    /// Property across random in-phase layer indices.
    #[test]
    fn bottom_layers_take_longer_than_normal(
        bottom_idx in 0u32..5,
        normal_idx in 50u32..500,
    ) {
        let recipe = Recipe::generic_standard();
        let printer = PrinterProfile::generic_msla_4k();
        // Only meaningful when bottom_idx is actually in the bottom phase.
        prop_assume!(bottom_idx < recipe.bottom_layer_count());
        prop_assume!(normal_idx >= recipe.bottom_layer_count() + recipe.transition_layers());
        let t_bottom = LayerTimingCalculator::layer_time_sec(&recipe, &printer, bottom_idx);
        let t_normal = LayerTimingCalculator::layer_time_sec(&recipe, &printer, normal_idx);
        prop_assert!(t_bottom > t_normal,
            "bottom layer should take longer: bottom={t_bottom}, normal={t_normal}");
    }

    /// Per-layer time is always positive (non-zero exposure + non-zero release motion).
    #[test]
    fn layer_time_always_positive(
        layer in 0u32..500,
    ) {
        let recipe = Recipe::generic_standard();
        for printer in [PrinterProfile::generic_msla_4k(), PrinterProfile::elegoo_mars5_ultra()] {
            let t = LayerTimingCalculator::layer_time_sec(&recipe, &printer, layer);
            prop_assert!(t > 0.0, "layer {layer} time must be positive: {t}");
            prop_assert!(t.is_finite(), "layer {layer} time must be finite: {t}");
        }
    }
}
