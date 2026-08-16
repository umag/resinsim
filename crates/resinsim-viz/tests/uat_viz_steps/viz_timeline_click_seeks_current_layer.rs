//! Step definitions for `spec/uat/viz-timeline-click-seeks-current-layer.md`.
//!
//! ALL 3 SCENARIOS ARE DECLARED DEBT. Every scenario requires egui chart
//! pointer interaction — `plot_ui.response().clicked()` +
//! `plot_ui.pointer_coordinate()` in `render_layer_timeline`
//! (`crates/resinsim-viz/src/ui/plots.rs`) — which needs synthetic egui
//! pointer events that bevy_egui 0.39 cannot produce.
//!
//! This is the SAME limitation documented for:
//! - `viz-screenshot-flag` UAT-6 (needs synthetic egui pointer click,
//!   bevy_egui 0.39 has no synthetic pointer-click API)
//! - `viz-load-sim-missing-sidecar` UAT-2 (drag-drop, needs synthetic
//!   egui pointer events)
//!
//! The underlying function `snap_plot_x_to_layer` IS unit-tested
//! (`crates/resinsim-viz/src/ui/plots.rs`, tests module:
//! `snap_plot_x_to_layer_empty_count_is_none`,
//! `snap_plot_x_to_layer_clamps_below_zero`,
//! `snap_plot_x_to_layer_clamps_above_max`,
//! `snap_plot_x_to_layer_rounds_to_nearest`,
//! `snap_plot_x_to_layer_in_range_round_trip`). The blocked path is
//! egui's input pipeline (delivering pointer events through
//! `EguiContexts` so `PlotUi::response().clicked()` returns true), not
//! Bevy type access — the lib.rs split makes `CurrentLayer` importable,
//! but that doesn't help when the bottleneck is egui input injection.
//!
//! REVISIT TRIGGER: bevy_egui gains a public API for injecting
//! synthetic pointer events into egui's `RawInput`, or the project
//! migrates to a bevy_egui version that exposes `Context::run()` with
//! caller-supplied `RawInput`. At that point, these scenarios can be
//! driven in-process: construct a `CurrentLayer` resource, call the
//! `bottom_panel` system with injected pointer coordinates, and assert
//! the resulting `CurrentLayer.index`.
