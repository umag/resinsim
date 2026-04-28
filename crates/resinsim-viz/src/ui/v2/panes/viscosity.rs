//! Viscosity pane. Single series in `SERIES_PURPLE`. Like vat_temp,
//! viscosity is a state variable rather than a fail boundary — it
//! shapes safety_factor upstream. The pane shows the trajectory so
//! the developer can see, e.g., a creeping rise that explains a
//! late-print peel-force spike.

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
pub struct ViscosityPane;

impl ViscosityPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Viscosity");
        render_pane_states(ui, ctx, PaneId::Viscosity, "Layer", "mPa·s", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(sim.layers().len());
    for (i, layer) in sim.layers().iter().enumerate() {
        let v = f64::from(layer.viscosity_mpa_s);
        if v.is_finite() {
            points.push([i as f64, v]);
        }
    }

    let values: Vec<f64> = points.iter().map(|p| p[1]).collect();
    let (lo, hi) = data_y_bounds(&values, 0.08).unwrap_or((0.0, 500.0));

    configure_plot(PaneId::Viscosity, "Layer", "mPa·s", ctx.link_group)
        .default_y_bounds(lo, hi)
        .show(ui, |plot_ui| {
        consume_plot_inputs(plot_ui, ctx);
        plot_ui.line(
            Line::new("viscosity", PlotPoints::from(points)).color(theme::SERIES_PURPLE),
        );
        plot_ui.vline(cursor_vline(ctx.cursor_layer));
    });
}
