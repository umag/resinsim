//! UI module tree for resinsim-viz egui panels.
//!
//! - `state` — `PickerState` resource + `refresh_*` helpers + the
//!   `run_block_reason` pure helper.
//! - `plots` — `PlotData` projection + `render_plots` helper.
//! - `panels` — left/right side-panel egui systems.
//! - `v2` — Grafana-style multi-pane dashboard for the redesign
//!   captured in `spec/viz-v2-design-brief.md`. Selected via `--v2`
//!   at runtime, mutually exclusive with the v1 panel set.

pub mod panels;
pub mod plots;
pub mod state;
pub mod v2;
