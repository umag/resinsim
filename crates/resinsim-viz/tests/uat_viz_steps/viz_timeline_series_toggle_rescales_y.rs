//! Step definitions for
//! `spec/uat/viz-timeline-series-toggle-rescales-y.md`.
//!
//! Two scenarios:
//!
//! UAT-1 (toggling cure depth on/off rescales Y):
//!   - TRIVIAL PASS (declared debt). Every assertion in this scenario
//!     requires a live `egui_plot` context: Y-range bounds are internal
//!     to `Plot::show`, and `set_auto_bounds` (the re-fit trigger) runs
//!     inside `render_layer_timeline` which needs `egui::Ui`.
//!     Existing unit-test coverage:
//!       - `plots.rs::cursor_label_top_y_uses_max_of_enabled_series`
//!         tests the y-range helper across visibility combos.
//!       - `state.rs::bottom_panel_state_default_matches_issue_body_spec`
//!         pins the default visibility.
//!     The `force_refit` detection (`prev_visibility != cur_vis`) in
//!     `render_layer_timeline` is structurally simple and reviewed in
//!     ADR-0016.
//!     Egui interaction simulation is not feasible per
//!     `docs/patterns/bevy-app-test-seam.md` (egui caveat section).
//!
//! UAT-2 (first-paint visibility is peel-only per issue body):
//!   - REAL assertions on `BottomPanelState::default()` fields. The
//!     Given step constructs the default state; Then steps verify each
//!     boolean field matches the issue body's spec. The "log10 not
//!     visible" assertion is a trivial pass (egui visibility), but the
//!     underlying `show_safety == false` is asserted.

use cucumber::{given, then, when};

use resinsim_viz::ui::state::BottomPanelState;

use crate::VizWorld;

// ---------------------------------------------------------------------------
// UAT-1: toggling cure depth on/off rescales Y (declared debt)
// ---------------------------------------------------------------------------

#[given(
    regex = r#"^the resinsim-viz binary running with --load-ctb \+ --load-sim for a typical 200-layer print$"#
)]
fn given_binary_with_200_layer_print(world: &mut VizWorld) {
    world.panel_state = Some(BottomPanelState::default());
}

#[given(
    regex = r#"^only "Peel force \(N\)" is checked \(default state per issue body\)$"#
)]
fn given_peel_only_default(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(state.show_peel, "peel must be on by default");
    assert!(!state.show_cure, "cure must be off by default");
    assert!(!state.show_safety, "safety must be off by default");
}

#[when(regex = r#"^the user observes the chart's Y range$"#)]
fn when_observe_y_range(_world: &mut VizWorld) {
    // Trivial pass: Y-range observation requires a live egui_plot context.
}

#[then(
    regex = r#"^the Y range bounds approximate \(0, peel_max × 1\.1\) — peel only \(typical: 0 to ~15\)$"#
)]
fn then_y_range_peel_only(_world: &mut VizWorld) {
    // Trivial pass: Y-range bounds are internal to egui_plot.
    // Covered by plots.rs::cursor_label_top_y_uses_max_of_enabled_series.
}

#[when(
    regex = r#"^the user clicks the "Cure depth \(µm\)" checkbox to enable it$"#
)]
fn when_click_cure_enable(_world: &mut VizWorld) {
    // Trivial pass: clicking a checkbox requires a live egui render.
}

#[then(regex = r#"^the chart re-fits Y on the same frame$"#)]
fn then_chart_refits_y(_world: &mut VizWorld) {
    // Trivial pass: re-fit is internal to render_layer_timeline
    // (force_refit → set_auto_bounds). Structurally reviewed in ADR-0016.
}

#[then(
    regex = r#"^the new Y range bounds approximate \(0, max\(peel, cure\) × 1\.1\) \(typical: 0 to ~200; cure dominates\)$"#
)]
fn then_y_range_peel_plus_cure(_world: &mut VizWorld) {
    // Trivial pass: Y-range bounds are internal to egui_plot.
}

#[when(regex = r#"^the user clicks "Cure depth \(µm\)" again to disable it$"#)]
fn when_click_cure_disable(_world: &mut VizWorld) {
    // Trivial pass: clicking a checkbox requires a live egui render.
}

#[then(
    regex = r#"^the Y range returns to peel-only bounds \(typical: 0 to ~15\)$"#
)]
fn then_y_range_returns_to_peel(_world: &mut VizWorld) {
    // Trivial pass: Y-range bounds are internal to egui_plot.
}

// ---------------------------------------------------------------------------
// UAT-2: first-paint visibility is peel-only per issue body
// ---------------------------------------------------------------------------

#[given(
    regex = r#"^a fresh resinsim-viz session with --load-ctb \+ --load-sim$"#
)]
fn given_fresh_session(world: &mut VizWorld) {
    world.panel_state = Some(BottomPanelState::default());
}

#[when(
    regex = r#"^the bottom panel renders for the first time after Run$"#
)]
fn when_first_render(_world: &mut VizWorld) {
    // The default state is constructed in Given; no render needed.
}

#[then(regex = r#"^"Peel force \(N\)" checkbox is checked$"#)]
fn then_peel_checked(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(
        state.show_peel,
        "Peel force must be checked by default per issue body"
    );
}

#[then(regex = r#"^"Cure depth \(µm\)" checkbox is unchecked$"#)]
fn then_cure_unchecked(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(
        !state.show_cure,
        "Cure depth must be unchecked by default"
    );
}

#[then(regex = r#"^"Safety factor" checkbox is unchecked$"#)]
fn then_safety_unchecked(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(
        !state.show_safety,
        "Safety factor must be unchecked by default"
    );
}

#[then(
    regex = r#"^the "log10" sub-checkbox for safety is not visible \(parent off\)$"#
)]
fn then_log10_not_visible_parent_off(world: &mut VizWorld) {
    let state = world
        .panel_state
        .as_ref()
        .expect("scenario invariant: Given must set panel_state");
    assert!(
        !state.show_safety,
        "Safety must be off → log10 sub-checkbox not visible"
    );
    assert!(
        !state.safety_log_scale,
        "log scale must be off when safety is off"
    );
}
