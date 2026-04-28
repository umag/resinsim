//! v2 dashboard module — Grafana-style 5×2 pane grid for the
//! `resinsim-viz` redesign captured in `spec/viz-v2-design-brief.md`.
//!
//! Pass 1 (this commit): module skeleton, theme tokens, Pane
//! abstraction, static 5×2 grid, ForcesPane wired end-to-end against
//! real `LoadedSimulation`. Other 9 panes render an empty axis
//! placeholder. Selected at runtime via the `--v2` CLI flag; the v1
//! left/right/bottom panel set is unaffected when the flag is off.
//!
//! Subsequent passes (see `spec/viz-v2-design-brief.md` follow-ups):
//!   Pass 2 — splitter resize between cells.
//!   Pass 3 — fill in the remaining 9 panes + Bevy-PinchGesture bridge
//!            for trackpad pinch zoom (X linked across panes, Y per
//!            pane). Per `docs/patterns/mac-trackpad-panorbit-config.md`
//!            we use Bevy 0.18's `bevy::input::gestures::PinchGesture`
//!            rather than depending on bevy_egui forwarding.
//!   Pass 4 — drag-to-reorder via `egui::dnd_drop_zone_*`. Drop on
//!            another cell swaps; insert+shift was rejected (open Q #4).
//!   Pass 5 — layout persistence via the `directories` crate, the
//!            full `PaneState` matrix from brief §6, and the right-
//!            click context menu.

pub mod failures_rail;
pub mod grid;
pub mod layout_persist;
pub mod pane;
pub mod panes;
pub mod readout;
pub mod scrubber;
pub mod summary;
pub mod theme;
pub mod zoom;

use std::path::PathBuf;
use std::time::Duration;

use bevy::ecs::message::MessageReader;
use bevy::input::gestures::PinchGesture;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use crate::CurrentLayer;
use crate::LoadedSimulation;
use crate::LoadedSliceMasks;

use self::grid::PaneGrid;

/// Debounce window between the user's last layout-mutating gesture
/// and the on-disk save. Tuned to be longer than a typical splitter
/// drag burst (~100ms of MouseMove events) but shorter than a
/// genuine "user paused to think" interval, so the save is reliable
/// without thrashing the disk during continuous drags.
const LAYOUT_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Bevy resource holding the v2 dashboard's pane grid layout. Survives
/// across frames so the user's resize/reorder state persists between
/// paints, and survives across sessions via the on-disk
/// [`layout_persist`] sidecar.
#[derive(Resource)]
pub struct V2Dashboard {
    pub grid: PaneGrid,
    /// One-shot guard so `apply_dark_theme` runs once on first paint.
    /// (egui's context isn't constructed until after Bevy `Startup`,
    /// so theme application has to happen lazily inside the egui pass.)
    themed: bool,
    /// Path to the persisted layout JSON, resolved at startup. `None`
    /// when the platform doesn't expose a config dir; in that case
    /// the dashboard works in-memory only and never saves.
    layout_path: Option<PathBuf>,
}

impl Default for V2Dashboard {
    fn default() -> Self {
        let layout_path = layout_persist::default_layout_path();
        let grid = layout_path
            .as_deref()
            .and_then(|p| match layout_persist::load(p) {
                Ok(layout) => Some(PaneGrid::from_layout(layout)),
                Err(layout_persist::LoadError::Io(e))
                    if e.kind() == std::io::ErrorKind::NotFound =>
                {
                    // First run on this machine — no error, just
                    // use the default layout.
                    None
                }
                Err(e) => {
                    warn!("failed to load v2 layout from disk, using default: {e}");
                    None
                }
            })
            .unwrap_or_else(PaneGrid::default_layout);
        Self {
            grid,
            themed: false,
            layout_path,
        }
    }
}

/// Bevy plugin that mounts the v2 dashboard. Registered conditionally
/// from `main.rs` based on the `--v2` CLI flag, parallel to (but
/// mutually exclusive with) the v1 panel set on `EguiPrimaryContextPass`.
pub struct V2UiPlugin;

impl Plugin for V2UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<V2Dashboard>().add_systems(
            EguiPrimaryContextPass,
            draw_v2_dashboard,
        );
    }
}

/// Single egui-pass system that paints the entire v2 dashboard. Owns
/// the full screen — there's no v1-style left/right/bottom split when
/// `--v2` is on.
///
/// Drains macOS-trackpad `PinchGesture` events and accumulates the
/// per-frame delta. The hovered pane consumes the delta inside its
/// `Plot::show` closure (see `super::zoom::apply_pinch_zoom`).
pub fn draw_v2_dashboard(
    mut contexts: EguiContexts,
    mut dashboard: ResMut<V2Dashboard>,
    loaded: Res<LoadedSimulation>,
    loaded_masks: Res<LoadedSliceMasks>,
    mut cursor: ResMut<CurrentLayer>,
    mut pinch_events: MessageReader<PinchGesture>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    if !dashboard.themed {
        theme::apply_dark_theme(ctx);
        dashboard.themed = true;
    }

    let pinch_delta: f32 = pinch_events.read().map(|e| e.0).sum();

    // Surface the most recent --load-sim parse error to the grid so
    // the brief §6 ParseError block can replace the dashboard when
    // the sim couldn't be loaded.
    let last_error: Option<&str> = loaded
        .last_attempt
        .as_ref()
        .and_then(|r| r.as_ref().err().map(String::as_str));

    // Panel mount order (matters — egui panels claim space in the
    // order they're declared):
    //   1. Top summary strip: full-window width per brief §5.
    //   2. Bottom scrubber: full-window width.
    //   3. Right readout: spans between strip and scrubber.
    //   4. Left failures rail: spans between strip and scrubber.
    //   5. Central pane grid: fills the remaining rectangle.
    //
    // ParseError replaces the entire central panel; rails + scrubber
    // + summary strip are suppressed so the user sees only the error
    // block.
    if last_error.is_none() {
        let cursor_layer = cursor.index;
        let cursor_max = cursor.max;
        let sim_ref = loaded.simulation.as_ref();
        let source_path = loaded.source_path.as_deref();
        let failures: &[resinsim_core::entities::FailureEvent] = sim_ref
            .map(|s| s.failures())
            .unwrap_or(&[]);

        bevy_egui::egui::TopBottomPanel::top("v2-summary")
            .resizable(false)
            .default_height(summary::SUMMARY_HEIGHT_PX)
            .show(ctx, |ui| {
                if let Some(new_layer) = summary::render(ui, sim_ref, source_path) {
                    if new_layer != cursor.index {
                        cursor.index = new_layer.min(cursor_max);
                    }
                }
            });

        bevy_egui::egui::TopBottomPanel::bottom("v2-scrubber")
            .resizable(false)
            .default_height(scrubber::SCRUBBER_HEIGHT_PX)
            .show(ctx, |ui| {
                if let Some(new_layer) =
                    scrubber::render(ui, cursor_layer, cursor_max, failures)
                {
                    if new_layer != cursor.index {
                        cursor.index = new_layer;
                    }
                }
            });

        bevy_egui::egui::SidePanel::left("v2-failures")
            .resizable(true)
            .default_width(failures_rail::FAILURES_WIDTH_DEFAULT)
            .min_width(failures_rail::FAILURES_WIDTH_MIN)
            .show(ctx, |ui| {
                if let Some(new_layer) =
                    failures_rail::render(ui, sim_ref, cursor.index)
                {
                    if new_layer != cursor.index {
                        cursor.index = new_layer.min(cursor_max);
                    }
                }
            });

        bevy_egui::egui::SidePanel::right("v2-readout")
            .resizable(true)
            .default_width(readout::READOUT_WIDTH_DEFAULT)
            .min_width(readout::READOUT_WIDTH_MIN)
            .show(ctx, |ui| {
                readout::render(ui, loaded.simulation.as_ref(), cursor.index);
            });
    }

    let slice_masks: Option<&[resinsim_core::io::sliced::LayerInput]> =
        if loaded_masks.layers.is_empty() {
            None
        } else {
            Some(&loaded_masks.layers)
        };

    bevy_egui::egui::CentralPanel::default().show(ctx, |ui| {
        dashboard.grid.render(
            ui,
            loaded.simulation.as_ref(),
            slice_masks,
            last_error,
            cursor.index,
            pinch_delta,
        );
    });

    // Layout persistence: debounced write after the user stops
    // mutating the layout. The grid tracks its own dirty timestamp;
    // we only do the IO work here.
    if let Some(path) = dashboard.layout_path.clone() {
        if let Some(layout) = dashboard.grid.take_save_if_due(LAYOUT_SAVE_DEBOUNCE) {
            if let Err(e) = layout_persist::save(&path, &layout) {
                warn!("failed to persist v2 layout to {}: {e}", path.display());
            }
        }
    }
}
