//! Vat temperature pane. Single series in `SERIES_ORANGE`. No
//! threshold — vat temp is a state variable, not a fail boundary;
//! whether a given temperature is "too hot" depends on the resin
//! recipe and is captured upstream in the safety_factor projection.

use bevy_egui::egui;
use egui_plot::{Line, PlotPoints};
use resinsim_core::simulation::PrintSimulation;

use crate::ui::v2::pane::{
    configure_plot, consume_plot_inputs, cursor_vline, pane_header, render_pane_states, PaneCtx,
    PaneId,
};
use crate::ui::v2::theme;
use crate::ui::v2::zoom::data_y_bounds;

#[derive(Default)]
pub struct VatTempPane;

impl VatTempPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Vat temperature");
        render_pane_states(ui, ctx, PaneId::VatTemp, "Layer", "°C", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(sim.layers().len());
    for (i, layer) in sim.layers().iter().enumerate() {
        let v = f64::from(layer.vat_temperature_c);
        if v.is_finite() {
            points.push([i as f64, v]);
        }
    }

    let values: Vec<f64> = points.iter().map(|p| p[1]).collect();
    let (lo, hi) = data_y_bounds(&values, 0.08).unwrap_or((15.0, 35.0));

    configure_plot(PaneId::VatTemp, "Layer", "°C", ctx.link_group)
        .default_y_bounds(lo, hi)
        .show(ui, |plot_ui| {
        consume_plot_inputs(plot_ui, ctx);
        plot_ui.line(
            Line::new("vat_temp", PlotPoints::from(points)).color(theme::SERIES_ORANGE),
        );
        plot_ui.vline(cursor_vline(ctx.cursor_layer));
    });
}
