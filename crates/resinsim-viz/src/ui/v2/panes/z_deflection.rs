//! Z deflection pane. Single series in `SERIES_BLUE`. Predicted
//! plate sag in micrometres. Tall, thin layers under high lift
//! force show up here as elevated values; the cumulative effect
//! across a print can shift the build-plate alignment, which
//! correlates with peel-force spikes a few layers later.

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
pub struct ZDeflectionPane;

impl ZDeflectionPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Z deflection");
        render_pane_states(ui, ctx, PaneId::ZDeflection, "Layer", "µm", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(sim.layers().len());
    for (i, layer) in sim.layers().iter().enumerate() {
        let z = f64::from(layer.z_deflection_um);
        if z.is_finite() {
            points.push([i as f64, z]);
        }
    }

    // Percentile-clamped: the bottom-layer spike is real and
    // important, but p99 keeps the steady-state band readable
    // without hiding the spike entirely (it still pokes above
    // the top edge and the user can pinch out to see it).
    let values: Vec<f64> = points.iter().map(|p| p[1]).collect();
    let (lo, hi) = percentile_bounds(&values, 0.0, 0.99, 0.08).unwrap_or((0.0, 10.0));

    configure_plot(PaneId::ZDeflection, "Layer", "µm", ctx.link_group)
        .default_y_bounds(lo.min(0.0), hi)
        .show(ui, |plot_ui| {
            consume_plot_inputs(plot_ui, ctx);
            plot_ui.line(
                Line::new("z_deflection", PlotPoints::from(points)).color(theme::SERIES_BLUE),
            );
            plot_ui.vline(cursor_vline(ctx.cursor_layer));
        });
}
