//! Step definitions for `spec/uat/viz-timeline-drag-pan-does-not-seek.md`.
//!
//! ALL 2 SCENARIOS ARE DECLARED DEBT. Both scenarios require egui chart
//! pointer interaction — UAT-1 needs a synthetic drag gesture
//! (press-move-release) and UAT-2 needs a synthetic click (press-release
//! without movement). Both exercise `plot_ui.response().clicked()` in
//! `render_layer_timeline` (`crates/resinsim-viz/src/ui/plots.rs`),
//! which requires egui's input system to have received pointer events.
//! bevy_egui 0.39 has no API for injecting synthetic pointer events.
//!
//! Same limitation as:
//! - `viz-timeline-click-seeks-current-layer` (all 3 scenarios)
//! - `viz-screenshot-flag` UAT-6
//! - `viz-load-sim-missing-sidecar` UAT-2
//!
//! The drag-vs-click distinction is load-bearing for chart navigability
//! (see spec rationale): `Response::clicked()` fires only on non-drag
//! clicks (press-and-release without movement). A drag-to-pan gesture
//! returns `false` from `clicked()` and does NOT invoke
//! `snap_plot_x_to_layer`. This UAT pins the invariant so an egui_plot
//! upgrade with different click semantics surfaces immediately.
//!
//! REVISIT TRIGGER: same as `viz_timeline_click_seeks_current_layer` —
//! bevy_egui gains synthetic pointer injection, or project migrates to
//! a version that exposes caller-supplied `RawInput`.
