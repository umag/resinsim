//! Cross-section area + Δarea pane. Two series, shared y-axis, both
//! in mm². `cross_section_area_mm2` carries the absolute footprint;
//! `area_delta_mm2` is the per-layer change (positive = layer
//! widened, negative = narrowed). On the same axis the absolute
//! reads as a near-constant baseline and the delta reads as a small
//! oscillation around zero, which is exactly the visual story of an
//! MSLA print: footprint changes slowly, the layer-to-layer delta
//! is a smaller signal that sometimes spikes at transitions. The
//! user can pinch-zoom Y (per-pane) to focus on the delta signal.

use bevy_egui::egui;
use egui_plot::{Line, PlotPoints};
use resinsim_core::simulation::PrintSimulation;

use crate::ui::v2::pane::{
    configure_plot, consume_plot_inputs, cursor_vline, pane_header, render_pane_states, PaneCtx,
    PaneId,
};
use crate::ui::v2::theme;
use crate::ui::v2::zoom::percentile_bounds;

#[derive(Default)]
pub struct AreaDeltaPane;

impl AreaDeltaPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Area + Δarea");
        render_pane_states(ui, ctx, PaneId::AreaDelta, "Layer", "mm²", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let n = sim.layers().len();
    let mut area: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut delta: Vec<[f64; 2]> = Vec::with_capacity(n);
    for (i, layer) in sim.layers().iter().enumerate() {
        let x = i as f64;
        let a = f64::from(layer.cross_section_area_mm2);
        if a.is_finite() {
            area.push([x, a]);
        }
        let d = f64::from(layer.area_delta_mm2);
        if d.is_finite() {
            delta.push([x, d]);
        }
    }

    // Percentile-clamped: bottom-layer area-delta spike is large
    // and dwarfs the steady-state oscillation. Drop both tails so
    // the steady-state variation in both series reads at default
    // zoom.
    let combined: Vec<f64> = area.iter().chain(delta.iter()).map(|p| p[1]).collect();
    let (lo, hi) = percentile_bounds(&combined, 0.02, 0.98, 0.08).unwrap_or((-100.0, 1000.0));

    configure_plot(PaneId::AreaDelta, "Layer", "mm²", ctx.link_group)
        .default_y_bounds(lo, hi)
        .show(ui, |plot_ui| {
            consume_plot_inputs(plot_ui, ctx);
            plot_ui.line(Line::new("area", PlotPoints::from(area)).color(theme::SERIES_YELLOW));
            plot_ui.line(Line::new("Δarea", PlotPoints::from(delta)).color(theme::SERIES_CYAN));
            plot_ui.vline(cursor_vline(ctx.cursor_layer));
        });
}
