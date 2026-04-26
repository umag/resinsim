//! Left + right side-panel egui systems. Both run on the
//! `EguiPrimaryContextPass` schedule (bevy_egui 0.39 multi-pass).
//!
//! Layout anchors locked here per ADR-0011:
//!   - left  `SidePanel::left("controls")`   — pickers + Run button
//!   - right `SidePanel::right("inspectors")` — summary line + plots
//!
//! Logic helpers (`PickerState::to_run_request`, `run_block_reason`,
//! `build_plot_data`) are tested plugin-less per
//! `docs/patterns/bevy-app-test-seam.md`. The egui draw closures are
//! mostly mechanical.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim::{RunSimRequest, SimulationResult, loaded_basename};
use crate::slice::LoadedSliceStack;
use crate::ui::plots::{build_plot_data, render_plots};
use crate::ui::state::{PickerState, run_block_reason};

/// Left side panel: profile pickers, read-only recipe labels, status
/// line, Run button, error ribbon.
pub fn left_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<PickerState>,
    mut run_writer: MessageWriter<RunSimRequest>,
    sim: Res<SimulationResult>,
    loaded_q: Query<&LoadedSliceStack>,
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
            let resin_text = state
                .selected_resin
                .clone()
                .unwrap_or_else(|| {
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
            let printer_text = state
                .selected_printer
                .clone()
                .unwrap_or_else(|| {
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
                ui.label(format!(
                    "Exposure: {:.2} s",
                    recipe.normal_exposure_sec()
                ));
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
            let status = run_block_reason(&state, has_ctb)
                .unwrap_or_else(|| "Ready to run".to_string());
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
                        summary.total_layers,
                        summary.critical_failures,
                        summary.total_time_sec
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
