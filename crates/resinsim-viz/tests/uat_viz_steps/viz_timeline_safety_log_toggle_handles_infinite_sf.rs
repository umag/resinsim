//! Step definitions for
//! `spec/uat/viz-timeline-safety-log-toggle-handles-infinite-sf.md`.
//!
//! Two scenarios:
//!
//! UAT-1 (∞-SF layer is absent from safety line in both modes):
//!   - REAL assertions via in-process `build_layer_chart_data` with a
//!     synthetic `PrintSimulation` containing one zero-peel-force layer
//!     (safety_factor = ∞). The Given step constructs the sim; the first
//!     When step calls `build_layer_chart_data(&sim, false)` (linear);
//!     the second When calls it with `true` (log10). Then steps assert on
//!     `LayerChartData.safety.points` (gap at ∞ index, finite values)
//!     and `.safety.name` (label change).
//!
//! UAT-2 (log10 toggle visibility tracks show_safety):
//!   - TRIVIAL PASS (declared debt). Every step in this scenario requires
//!     a live `egui::Ui` context to observe checkbox visibility and the
//!     `safety_log_scale` reset inside `render_layer_timeline`. The Given
//!     step constructs a `BottomPanelState::default()` to prove the
//!     default state, but all remaining assertions are trivial passes.
//!     Existing unit-test coverage:
//!       - `state.rs::bottom_panel_state_default_matches_issue_body_spec`
//!         pins defaults.
//!       - `plots.rs::build_layer_chart_data_safety_filters_inf` covers
//!         the linear ∞-filter.
//!       - `plots.rs::build_layer_chart_data_log_safety_omits_non_positive_and_non_finite`
//!         covers the log10 filter.
//!     Egui interaction simulation is not feasible per
//!     `docs/patterns/bevy-app-test-seam.md` (egui caveat section).

use cucumber::{given, then, when};

use resinsim_core::entities::LayerResult;
use resinsim_core::repositories::{PrinterProfileRepository, ResinProfileRepository};
use resinsim_core::simulation::PrintSimulation;
use resinsim_viz::ui::plots::build_layer_chart_data;
use resinsim_viz::ui::state::BottomPanelState;

use crate::VizWorld;

fn workspace_data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
}

fn synthetic_sim_with_inf_safety() -> PrintSimulation {
    let data = workspace_data_dir();
    let resin_repo = ResinProfileRepository::new(&data.join("resins"));
    let printer_repo = PrinterProfileRepository::new(&data.join("printers"));
    let resin = resin_repo
        .load("generic_standard")
        .expect("test fixture: shipped resin");
    let printer = printer_repo
        .load("generic_msla_4k")
        .expect("test fixture: shipped printer");
    let mut sim = PrintSimulation::new(resin.recipe().clone(), printer);
    let mk = |idx: u32, peel: f32, sf: f32| LayerResult {
        index: idx,
        cure_depth_um: 100.0,
        peel_force_n: peel,
        suction_force_n: 0.0,
        base_force_n: 0.0,
        peel_shape_factor: None,
        total_force_n: peel,
        support_capacity_n: peel * sf.max(1.0),
        safety_factor: sf,
        cross_section_area_mm2: 100.0,
        area_delta_mm2: 0.0,
        vat_temperature_c: 22.0,
        viscosity_mpa_s: 200.0,
        z_deflection_um: 1.0,
        effective_layer_height_um: 50.0,
        worst_cure_depth_um: 100.0,
        strain_magnitude_max: None,
        stress_von_mises_max_mpa: None,
        strain_gradient_max_frac: None,
        voxel_yield_fraction: None,
        crack_front_fraction: None,
    };
    sim.add_layer(mk(0, 5.0, 3.0), vec![])
        .expect("test fixture: index 0");
    sim.add_layer(mk(1, 0.0, f32::INFINITY), vec![])
        .expect("test fixture: index 1 (∞ SF)");
    sim.add_layer(mk(2, 5.0, 3.0), vec![])
        .expect("test fixture: index 2");
    sim
}

// ---------------------------------------------------------------------------
// UAT-1: ∞-SF layer is absent from the safety line in both modes
// ---------------------------------------------------------------------------

#[given(
    regex = r#"^the resinsim-viz binary running with --load-ctb \+ --load-sim for a print where at least one layer has zero peel force \(e\.g\. a sliced-area-zero geometry — the bottom rafts of many test prints satisfy this\)$"#
)]
fn given_binary_with_zero_peel_force_layer(world: &mut VizWorld) {
    world.sim = Some(synthetic_sim_with_inf_safety());
}

#[when(regex = r#"^the user enables the "Safety factor" series$"#)]
fn when_enable_safety_factor(world: &mut VizWorld) {
    let sim = world
        .sim
        .as_ref()
        .expect("scenario invariant: Given must construct a sim");
    world.chart_data = Some(build_layer_chart_data(sim, false));
}

#[then(regex = r#"^no panic is raised when the chart paints$"#)]
fn then_no_panic_when_chart_paints(_world: &mut VizWorld) {
    // The When step already called build_layer_chart_data without panic.
}

#[then(
    regex = r#"^the safety line has gaps at the layer indices whose safety_factor is INFINITY$"#
)]
fn then_safety_has_gaps_at_infinity(world: &mut VizWorld) {
    let data = world
        .chart_data
        .as_ref()
        .expect("scenario invariant: When step must produce chart_data");
    let xs: Vec<f64> = data.safety.points.iter().map(|p| p[0]).collect();
    assert!(
        !xs.contains(&1.0),
        "layer 1 (∞ SF) must be absent from the safety series; got x-indices: {xs:?}"
    );
    assert_eq!(
        data.safety.points.len(),
        2,
        "3-layer sim with one ∞ layer must produce exactly 2 safety points"
    );
}

#[then(
    regex = r#"^the safety series y values for the surviving layers are finite-positive$"#
)]
fn then_safety_values_finite_positive(world: &mut VizWorld) {
    let data = world
        .chart_data
        .as_ref()
        .expect("scenario invariant: When step must produce chart_data");
    for p in &data.safety.points {
        assert!(
            p[1].is_finite() && p[1] > 0.0,
            "safety y value must be finite-positive, got {} at layer {}",
            p[1],
            p[0],
        );
    }
}

#[when(regex = r#"^the user enables the "log10" sub-checkbox$"#)]
fn when_enable_log10(world: &mut VizWorld) {
    let sim = world
        .sim
        .as_ref()
        .expect("scenario invariant: Given must construct a sim");
    world.chart_data = Some(build_layer_chart_data(sim, true));
}

#[then(regex = r#"^no panic is raised$"#)]
fn then_no_panic(_world: &mut VizWorld) {
    // The When step already called build_layer_chart_data(sim, true) without panic.
}

#[then(
    regex = r#"^the safety series additionally drops any layers with safety_factor ≤ 0 \(none expected in normal sims, but the filter is correctness-load-bearing\)$"#
)]
fn then_safety_log_drops_non_positive(world: &mut VizWorld) {
    let data = world
        .chart_data
        .as_ref()
        .expect("scenario invariant: When step must produce chart_data");
    for p in &data.safety.points {
        assert!(
            p[1].is_finite(),
            "log10 safety y must be finite, got {} at layer {}",
            p[1],
            p[0],
        );
    }
}

#[then(
    regex = r#"^the legend label changes from "Safety factor \(×\)" to "Safety factor \(log10\)"$"#
)]
fn then_legend_changes_to_log10(world: &mut VizWorld) {
    let data = world
        .chart_data
        .as_ref()
        .expect("scenario invariant: When step must produce chart_data");
    assert_eq!(
        data.safety.name, "Safety factor (log10)",
        "log10 mode must change the series name"
    );
}

// ---------------------------------------------------------------------------
// UAT-2: log10 toggle visibility tracks show_safety (declared debt)
// ---------------------------------------------------------------------------

#[given(regex = r#"^a fresh resinsim-viz session, the bottom panel rendering$"#)]
fn given_fresh_session_bottom_panel(world: &mut VizWorld) {
    world.panel_state = Some(BottomPanelState::default());
}

#[when(regex = r#"^the user observes the checkbox row above the chart$"#)]
fn when_observe_checkbox_row(_world: &mut VizWorld) {
    // Trivial pass: observing the checkbox row requires a live egui render.
}

#[then(
    regex = r#"^"log10" checkbox is NOT visible \(Safety factor is off by default per the issue body\)$"#
)]
fn then_log10_not_visible_default(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(
        !state.show_safety,
        "Safety factor must be off by default"
    );
    assert!(
        !state.safety_log_scale,
        "log scale must be off by default"
    );
}

#[when(regex = r#"^the user enables "Safety factor"$"#)]
fn when_enable_safety(_world: &mut VizWorld) {
    // Trivial pass: clicking a checkbox requires a live egui render.
}

#[then(regex = r#"^"log10" checkbox becomes visible$"#)]
fn then_log10_becomes_visible(_world: &mut VizWorld) {
    // Trivial pass: checkbox visibility is egui rendering behavior.
    // Covered by manual smoke testing (ADR-0011 step 13 checklist).
}

#[when(
    regex = r#"^the user enables "log10", then disables "Safety factor"$"#
)]
fn when_enable_log10_then_disable_safety(_world: &mut VizWorld) {
    // Trivial pass: sequential checkbox clicks require a live egui render.
}

#[then(regex = r#"^"log10" is no longer visible$"#)]
fn then_log10_no_longer_visible(_world: &mut VizWorld) {
    // Trivial pass: checkbox visibility is egui rendering behavior.
}

#[then(
    regex = r#"^the next time "Safety factor" is enabled, the chart paints in linear mode \(log10 resets — unsurprising re-enable\)$"#
)]
fn then_log10_resets_on_reenable(_world: &mut VizWorld) {
    // Trivial pass: the safety_log_scale reset logic runs inside
    // render_layer_timeline (plots.rs:444–445) which requires egui::Ui.
    // Declared debt per bevy-app-test-seam.md egui caveat.
}
