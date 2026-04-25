//! Property tests for SimSummary's time projection (total + per-phase).
//!
//! Mirrors `layer_timing_properties.rs` — proptest over total_layers with
//! factory Recipe + PrinterProfile. The Tilt < Linear direction invariant is
//! intentionally scoped to factory defaults (see below) because a hand-
//! crafted Tilt recipe with large `lift_cycle_sec` can reverse the
//! direction; proptesting it across arbitrary recipes would be false.
//!
//! Under v4 (print-time-on-reportgenerator), PrintSimulation OWNS Recipe +
//! PrinterProfile — so `build_sim` takes both at construction and `summary()`
//! is arg-less.
use proptest::prelude::*;
use resinsim_core::entities::{LayerResult, PrinterProfile, Recipe, ResinProfile};
use resinsim_core::services::LayerTimingCalculator;
use resinsim_core::simulation::PrintSimulation;

fn dummy_layer(index: u32) -> LayerResult {
    LayerResult {
        index,
        cure_depth_um: 100.0,
        peel_force_n: 1.0,
        suction_force_n: 0.0,
        total_force_n: 1.0,
        support_capacity_n: 10.0,
        safety_factor: 10.0,
        cross_section_area_mm2: 100.0,
        area_delta_mm2: 0.0,
        vat_temperature_c: 22.0,
        viscosity_mpa_s: 200.0,
        z_deflection_um: 2.0,
        effective_layer_height_um: 48.0,
        worst_cure_depth_um: 100.0,
    }
}

fn build_sim(n: u32, recipe: Recipe, printer: PrinterProfile) -> PrintSimulation {
    let mut sim = PrintSimulation::new(recipe, printer);
    for i in 0..n {
        sim.add_layer(dummy_layer(i), vec![])
            .expect("test fixture: sequential index i in 0..n satisfies add_layer's contiguity precondition");
    }
    sim
}

fn default_recipe() -> Recipe {
    ResinProfile::generic_standard().recipe().clone()
}

proptest! {
    /// total_time_sec(N) >= total_time_sec(N-1) — monotonic in layer count.
    #[test]
    fn total_time_monotonic_in_layer_count(
        n in 1u32..300,
    ) {
        let recipe = default_recipe();
        let printer = PrinterProfile::generic_msla_4k();
        let s_n = build_sim(n, recipe.clone(), printer.clone()).summary();
        let s_prev = build_sim(n - 1, recipe, printer).summary();
        prop_assert!(
            s_n.total_time_sec + 1e-6 >= s_prev.total_time_sec,
            "total_time_sec dropped: n={n} total={} vs n-1={} total={}",
            s_n.total_time_sec, n - 1, s_prev.total_time_sec,
        );
    }

    /// total_time_sec > 0 iff total_layers > 0.
    #[test]
    fn total_positive_when_nonempty(
        n in 0u32..100,
    ) {
        let recipe = default_recipe();
        let printer = PrinterProfile::generic_msla_4k();
        let s = build_sim(n, recipe, printer).summary();
        if n == 0 {
            prop_assert_eq!(s.total_time_sec, 0.0);
        } else {
            prop_assert!(s.total_time_sec > 0.0,
                "non-empty run must have positive total time: {}", s.total_time_sec);
        }
    }

    /// Per-phase fields sum to total_time_sec within max(1e-3 * total, 1e-6).
    /// Covers Linear and Tilt branches.
    #[test]
    fn phase_sum_equals_total(
        n in 1u32..300,
        tilt in any::<bool>(),
    ) {
        let recipe = default_recipe();
        let printer = if tilt {
            PrinterProfile::elegoo_mars5_ultra()
        } else {
            PrinterProfile::generic_msla_4k()
        };
        let s = build_sim(n, recipe, printer).summary();
        let sum = s.bottom_time_sec + s.transition_time_sec + s.normal_time_sec;
        let tol = (s.total_time_sec.abs() * 1e-3).max(1e-6);
        prop_assert!(
            (sum - s.total_time_sec).abs() < tol,
            "phase sum {sum} != total {} (tol {tol}) at n={n} tilt={tilt}",
            s.total_time_sec,
        );
    }
}

/// Factory-scoped (non-proptest): Tilt total_time_sec < Linear total_time_sec
/// for identical recipe and layer count on the shipped factory pair
/// (elegoo_mars5_ultra + generic_standard vs generic_msla_4k + generic_standard).
/// Verified numerically: Linear normal layer = 14.0s, Tilt normal layer = 10.5s
/// (layer_timing_calculator.rs tests). Holds for all N >= 1 on factory defaults.
/// Proptest-scope would be wrong: a hand-crafted Tilt recipe with large
/// lift_cycle_sec can reverse the direction.
#[test]
fn tilt_strictly_less_than_linear_on_default_factories() {
    let recipe = default_recipe();
    let linear = PrinterProfile::generic_msla_4k();
    let tilt = PrinterProfile::elegoo_mars5_ultra();
    for n in [1u32, 10, 100, 500] {
        let s_linear = build_sim(n, recipe.clone(), linear.clone()).summary();
        let s_tilt = build_sim(n, recipe.clone(), tilt.clone()).summary();
        assert!(
            s_tilt.total_time_sec < s_linear.total_time_sec,
            "Tilt total must be < Linear total on factory defaults at n={n}: tilt={}, linear={}",
            s_tilt.total_time_sec,
            s_linear.total_time_sec,
        );
    }
    // Spot-check against the direct calculator too — belt-and-braces.
    let n = 100;
    let direct_linear = LayerTimingCalculator::cumulative_times_sec(&recipe, &linear, n);
    let direct_tilt = LayerTimingCalculator::cumulative_times_sec(&recipe, &tilt, n);
    let last_linear = direct_linear
        .last()
        .expect("100 layers produce non-empty Linear cumulative vector");
    let last_tilt = direct_tilt
        .last()
        .expect("100 layers produce non-empty Tilt cumulative vector");
    assert!(last_tilt < last_linear);
}
