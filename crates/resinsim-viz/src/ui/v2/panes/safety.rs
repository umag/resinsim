//! Safety factor pane. Single series with a 1.0 threshold line in
//! `THRESHOLD_RED` — values below 1.0 mean "predicted total force
//! exceeds support capacity" which is a hard fail. Filter rule
//! mirrors `crate::ui::plots::build_layer_chart_data`: drop layers
//! whose `safety_factor` is non-finite (∞ on zero-force layers) so
//! the line has gaps rather than spikes.
//!
//! Default Y bounds are `[0, 5]` — most prints sit in 1–3, so this
//! frames the threshold band without forcing the user to pan.

use bevy_egui::egui;
use egui_plot::{HLine, Line, PlotPoints};
use resinsim_core::simulation::PrintSimulation;

use crate::ui::v2::pane::{
    configure_plot, consume_plot_inputs, cursor_vline, pane_header, render_pane_states, PaneCtx,
    PaneId,
};
use crate::ui::v2::theme;
use crate::ui::v2::zoom::percentile_bounds;

#[derive(Default)]
pub struct SafetyPane;

impl SafetyPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Safety factor");
        render_pane_states(ui, ctx, PaneId::Safety, "Layer", "ratio", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(sim.layers().len());
    for (i, layer) in sim.layers().iter().enumerate() {
        let sf = f64::from(layer.safety_factor);
        if sf.is_finite() {
            points.push([i as f64, sf]);
        }
    }

    // Default bounds: percentile-clamped (p2..p98) so a handful of
    // huge outliers (e.g. safety_factor crossing 1e6 on near-zero-
    // force layers) don't crush the steady-state band. Always
    // include the 1.0 fail threshold so the brief's "trouble layers
    // earn weight" rule reads at default zoom.
    let values: Vec<f64> = points.iter().map(|p| p[1]).collect();
    let (data_lo, data_hi) =
        percentile_bounds(&values, 0.02, 0.98, 0.08).unwrap_or((0.0, 5.0));
    let lo = data_lo.min(0.0);
    let hi = data_hi.max(1.5);

    configure_plot(PaneId::Safety, "Layer", "ratio", ctx.link_group)
        .default_y_bounds(lo, hi)
        .show(ui, |plot_ui| {
            consume_plot_inputs(plot_ui, ctx);
            plot_ui.line(
                Line::new("safety_factor", PlotPoints::from(points))
                    .color(theme::SERIES_GREEN),
            );
            plot_ui.hline(
                HLine::new("fail_threshold", 1.0_f64)
                    .color(theme::THRESHOLD_RED)
                    .width(1.0_f32),
            );
            plot_ui.vline(cursor_vline(ctx.cursor_layer));
        });
}
