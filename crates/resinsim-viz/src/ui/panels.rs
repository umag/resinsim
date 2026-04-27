//! Left + right + bottom egui panel systems, all on the
//! `EguiPrimaryContextPass` schedule (bevy_egui 0.39 multi-pass).
//!
//! Layout anchors locked here per ADR-0011 (left/right) + ADR-0014
//! (bottom):
//!   - left   `SidePanel::left("controls")`         — pickers + Run
//!   - right  `SidePanel::right("inspectors")`      — summary + plots
//!   - bottom `TopBottomPanel::bottom("layer-timeline")` — issue 05
//!     layer chart with click-to-seek
//!
//! Logic helpers (`PickerState::to_run_request`, `run_block_reason`,
//! `build_plot_data`, `build_layer_chart_data`,
//! `snap_plot_x_to_layer`) are tested plugin-less per
//! `docs/patterns/bevy-app-test-seam.md`. The egui draw closures are
//! mechanical.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::CurrentLayer;
use crate::screenshot::{default_screenshot_path, spawn_button_screenshot, LastScreenshot};
use crate::sim::{loaded_basename, RunSimRequest, SimulationResult};
use crate::slice::LoadedSliceStack;
use crate::ui::plots::{
    build_layer_chart_data, build_plot_data, render_layer_timeline, render_plots,
};
use crate::ui::state::{run_block_reason, BottomPanelState, PickerState};

/// Toast lifetime for the "Captured: <basename>" label after the
/// Capture-screenshot button is clicked. 3 s @ 60 Hz keeps the
/// signal visible across a brief glance without lingering past
/// the next interaction.
const CAPTURE_TOAST_DURATION: std::time::Duration = std::time::Duration::from_secs(3);

/// Left side panel: profile pickers, read-only recipe labels, status
/// line, Run button, error ribbon, and the issue-12 Capture
/// section (button + transient toast).
///
/// UI conventions: tooltip on the Capture button + 3-s transient
/// toast are NEW patterns introduced by issue 12 (no existing buttons
/// use tooltips; status/error use persistent labels). Chosen because
/// Capture has no preconditions to gate (unlike Run, which uses
/// add_enabled) and the action fires repeatedly (a persistent
/// "last capture" label would clutter; toast suits the fire-and-forget
/// shape).
#[allow(clippy::too_many_arguments)]
pub fn left_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<PickerState>,
    mut run_writer: MessageWriter<RunSimRequest>,
    sim: Res<SimulationResult>,
    loaded_q: Query<&LoadedSliceStack>,
    mut commands: Commands,
    mut last_screenshot: ResMut<LastScreenshot>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let loaded_path = loaded_q.iter().next().map(|s| s.path.clone());
    let has_ctb = loaded_path.is_some();

    egui::SidePanel::left("controls")
        .resizable(true)
        .default_width(280.0)
        .min_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Run");
            ui.separator();

            // --- Resin picker ---
            let resin_text = state.selected_resin.clone().unwrap_or_else(|| {
                if state.available_resins.is_empty() {
                    "(no profiles — set --data-dir)".into()
                } else {
                    "(select a resin)".into()
                }
            });
            egui::ComboBox::from_label("Resin profile")
                .selected_text(resin_text)
                .show_ui(ui, |ui| {
                    for name in state.available_resins.clone() {
                        let is_sel = state.selected_resin.as_ref() == Some(&name);
                        if ui.selectable_label(is_sel, &name).clicked() {
                            state.selected_resin = Some(name);
                        }
                    }
                });

            // --- Printer picker ---
            let printer_text = state.selected_printer.clone().unwrap_or_else(|| {
                if state.available_printers.is_empty() {
                    "(no profiles — set --data-dir)".into()
                } else {
                    "(select a printer)".into()
                }
            });
            egui::ComboBox::from_label("Printer profile")
                .selected_text(printer_text)
                .show_ui(ui, |ui| {
                    for name in state.available_printers.clone() {
                        let is_sel = state.selected_printer.as_ref() == Some(&name);
                        if ui.selectable_label(is_sel, &name).clicked() {
                            state.selected_printer = Some(name);
                        }
                    }
                });

            ui.add_space(6.0);

            // --- Read-only recipe defaults from cached resin ---
            if let Some(resin) = state.loaded_resin.as_ref() {
                let recipe = resin.recipe();
                ui.label(format!(
                    "Layer height: {:.1} µm (from {})",
                    recipe.layer_height_um(),
                    resin.name()
                ));
                ui.label(format!("Exposure: {:.2} s", recipe.normal_exposure_sec()));
            } else {
                ui.label("(pick a resin to see recipe defaults)");
            }

            ui.add_space(6.0);

            // --- Loaded CTB hint ---
            match loaded_path.as_deref() {
                Some(p) => ui.label(format!("Loaded CTB: {}", loaded_basename(p))),
                None => ui.label("(drag a .ctb file in to load)"),
            };

            ui.add_space(6.0);

            // --- Status line ---
            let status =
                run_block_reason(&state, has_ctb).unwrap_or_else(|| "Ready to run".to_string());
            ui.colored_label(egui::Color32::GRAY, status);

            // --- Run button ---
            let req = state.to_run_request();
            let enabled = req.is_some() && has_ctb;
            let clicked = ui
                .add_enabled(enabled, egui::Button::new("Run simulation"))
                .clicked();
            if clicked
                && let Some(r) = req
            {
                run_writer.write(r);
            }

            ui.add_space(6.0);

            // --- Error ribbon ---
            if let Some(err) = sim.last_error.as_deref() {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }

            ui.add_space(12.0);

            // --- Capture section (issue 12) ---
            ui.separator();
            ui.heading("Capture");
            let button = ui.button("Capture screenshot").on_hover_text(
                "Saves a PNG of the current window to the working \
                     directory (timestamped filename).",
            );
            if button.clicked() {
                let path = default_screenshot_path();
                spawn_button_screenshot(&mut commands, &path);
                last_screenshot.0 = Some((path, std::time::Instant::now()));
            }
            // Transient "Captured: <basename>" toast for CAPTURE_TOAST_DURATION.
            if let Some((path, started)) = last_screenshot.0.as_ref() {
                if started.elapsed() <= CAPTURE_TOAST_DURATION {
                    let basename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("(unknown)");
                    ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Captured: {basename}"));
                }
            }
        });
}

/// Right side panel: compact summary + three stacked plots + the
/// 06+ stub. The render anchor is fixed for issues 05/06/07.
pub fn right_panel(mut contexts: EguiContexts, sim: Res<SimulationResult>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::SidePanel::right("inspectors")
        .resizable(true)
        .default_width(420.0)
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.heading("Simulation");
            ui.separator();

            match sim.simulation.as_ref() {
                Some(s) => {
                    let summary = s.summary();
                    ui.label(format!(
                        "{} layers · {} failures · total time {:.1} s",
                        summary.total_layers, summary.critical_failures, summary.total_time_sec
                    ));
                }
                None => {
                    ui.label("(no run yet)");
                }
            }

            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let data = sim.simulation.as_ref().map(build_plot_data);
                render_plots(ui, data.as_ref());

                ui.add_space(8.0);
                ui.separator();
                ui.heading("Material editor");
                ui.label("(coming in 06)");
            });
        });
}

/// Bottom panel: layer-axis chart with click-to-seek (issue 05).
/// Mounts after left/right side panels claim their full-vertical
/// strips so this one consumes the remaining bottom band. The
/// click-to-seek path writes through the same `CurrentLayer` resource
/// the arrow keys + heatmap consumers already share — one shared
/// scrub state, no new fan-in.
pub fn bottom_panel(
    mut contexts: EguiContexts,
    sim: Res<SimulationResult>,
    mut state: ResMut<BottomPanelState>,
    mut current: ResMut<CurrentLayer>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::TopBottomPanel::bottom("layer-timeline")
        .resizable(true)
        .default_height(180.0)
        .min_height(120.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Layer timeline");
                ui.add_space(12.0);
                if let Some(s) = sim.simulation.as_ref() {
                    let n = s.layers().len();
                    if n > 0 {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            format!("Layer {} / {n}", current.index.saturating_add(1)),
                        );
                    }
                }
            });
            ui.separator();

            let Some(s) = sim.simulation.as_ref() else {
                ui.label("(no run yet — Run a sim to see the layer timeline)");
                return;
            };
            if s.layers().is_empty() {
                ui.label("(simulation has no layers)");
                return;
            }

            let data = build_layer_chart_data(s, state.safety_log_scale);
            if let Some(idx) =
                render_layer_timeline(ui, &data, current.index, current.max, &mut state)
            {
                // Click-to-seek: write into the shared CurrentLayer
                // resource. `update_layer_cursor` + `log_layer_change`
                // pick this up via Changed<CurrentLayer> next frame.
                current.index = idx;
            }
        });
}
