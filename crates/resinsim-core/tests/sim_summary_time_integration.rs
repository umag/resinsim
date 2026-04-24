//! Integration test for SimSummary per-phase time projection.
//!
//! Uses SimulationRunner::run_from_layer_inputs with a synthesised 100-layer
//! stack — covers Linear (generic_msla_4k) and Tilt (elegoo_mars5_ultra)
//! branches end to end. No STL slice, no CTB parse — fast.
//!
//! Under v4 (print-time-on-reportgenerator), PrintSimulation owns Recipe +
//! PrinterProfile, so `summary()` is arg-less — the aggregate's own pinned
//! recipe/printer drives the projection. Callers don't re-thread them.
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::services::LayerTimingCalculator;
use resinsim_core::values::AmbientTemperature;

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22°C is a valid ambient")
}

fn layer_inputs(
    n: u32,
    area_mm2: f64,
    exposure_sec: f32,
    lift_speed: f32,
    layer_h_um: f32,
) -> Vec<LayerInput> {
    (0..n)
        .map(|i| {
            LayerInput::new(
                i,
                area_mm2,
                exposure_sec,
                lift_speed,
                layer_h_um,
                (i as f32 + 1.0) * (layer_h_um / 1000.0),
            )
            .expect("valid layer input for factory-recipe defaults")
        })
        .collect()
}

#[test]
fn linear_summary_time_matches_calculator() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs(100, 100.0, 2.5, 60.0, 50.0);

    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("valid factory profiles satisfy run_from_layer_inputs preconditions");

    let summary = sim.summary();
    let expected = *LayerTimingCalculator::cumulative_times_sec(resin.recipe(), &printer, 100)
        .last()
        .expect("100 layers produce non-empty cumulative vector");

    // Deterministic — same recipe + printer + n produce identical cumulative
    // vectors in summary() and direct invocation. Exact equality expected.
    assert_eq!(
        summary.total_time_sec, expected,
        "Linear total_time_sec should bit-equal the calculator's last element",
    );

    let phase_sum = summary.bottom_time_sec + summary.transition_time_sec + summary.normal_time_sec;
    let tol = (summary.total_time_sec * 1e-3).max(1e-6);
    assert!(
        (phase_sum - summary.total_time_sec).abs() < tol,
        "per-phase sum {phase_sum} should equal total {} within {tol}",
        summary.total_time_sec,
    );
}

#[test]
fn tilt_summary_time_matches_calculator_and_is_less_than_linear() {
    let resin = ResinProfile::generic_standard();
    let linear = PrinterProfile::generic_msla_4k();
    let tilt = PrinterProfile::elegoo_mars5_ultra();
    let layers = layer_inputs(100, 100.0, 2.5, 60.0, 50.0);

    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &resin,
        &tilt,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("valid factory profiles satisfy run_from_layer_inputs preconditions");

    let summary_tilt = sim.summary();
    let expected_tilt = *LayerTimingCalculator::cumulative_times_sec(resin.recipe(), &tilt, 100)
        .last()
        .expect("100 layers produce non-empty cumulative vector");
    // Deterministic — exact equality expected (same reasoning as the Linear test).
    assert_eq!(
        summary_tilt.total_time_sec, expected_tilt,
        "Tilt total_time_sec should bit-equal the calculator's last element",
    );

    // Compare directions on factory defaults — ADR-0007 Tilt < Linear invariant.
    let sim_linear = SimulationRunner::run_from_layer_inputs(
        &layers,
        &resin,
        &linear,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("valid factory profiles satisfy run_from_layer_inputs preconditions");
    let summary_linear = sim_linear.summary();
    assert!(
        summary_tilt.total_time_sec < summary_linear.total_time_sec,
        "Tilt total {} should be less than Linear total {} on factory defaults",
        summary_tilt.total_time_sec,
        summary_linear.total_time_sec,
    );
}
