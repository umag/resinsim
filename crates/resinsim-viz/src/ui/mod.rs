//! UI module tree for resinsim-viz egui panels.
//!
//! - `state` — `PickerState` resource + `refresh_*` helpers + the
//!   `run_block_reason` pure helper.
//! - `plots` — `PlotData` projection + `render_plots` helper.
//! - `panels` — left/right side-panel egui systems.

pub mod panels;
pub mod plots;
pub mod state;
