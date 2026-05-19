//! Cure depth pane. Plots `cure_depth_um` (green) and the
//! conservative `worst_cure_depth_um` (yellow) so the developer
//! sees both the typical and the worst-pixel cure on each layer.
//! The amber threshold line is the median `effective_layer_height_um`
//! — cure must exceed layer height for the print to bond layers.
//! When `worst_cure_depth_um` dips below it, that layer is at risk.

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
pub struct CureDepthPane;

impl CureDepthPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Cure depth");
        render_pane_states(ui, ctx, PaneId::CureDepth, "Layer", "µm", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let n = sim.layers().len();
    let mut cure: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut worst: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut layer_heights: Vec<f64> = Vec::with_capacity(n);
    for (i, layer) in sim.layers().iter().enumerate() {
        let x = i as f64;
        let c = f64::from(layer.cure_depth_um);
        if c.is_finite() {
            cure.push([x, c]);
        }
        let w = f64::from(layer.worst_cure_depth_um);
        if w.is_finite() {
            worst.push([x, w]);
        }
        let h = f64::from(layer.effective_layer_height_um);
        if h.is_finite() {
            layer_heights.push(h);
        }
    }
    let height_threshold = median_finite(&layer_heights);

    // Percentile-clamped: the bottom-layer cure spike (~3-5x
    // steady state on slow exposures) crushes the rest of the
    // print into a flat band where the load-bearing "worst dips
    // toward threshold" story lives. p2..p98 keeps the spike
    // offscreen at default zoom; the layer-height threshold is
    // always pulled into view so the FAIL boundary is visible.
    let combined: Vec<f64> = cure.iter().chain(worst.iter()).map(|p| p[1]).collect();
    let (data_lo, data_hi) = percentile_bounds(&combined, 0.02, 0.98, 0.08).unwrap_or((0.0, 200.0));
    let lo = match height_threshold {
        Some(t) => data_lo.min(t).min(0.0),
        None => data_lo.min(0.0),
    };
    let hi = match height_threshold {
        Some(t) => data_hi.max(t),
        None => data_hi,
    };

    configure_plot(PaneId::CureDepth, "Layer", "µm", ctx.link_group)
        .default_y_bounds(lo, hi)
        .show(ui, |plot_ui| {
            consume_plot_inputs(plot_ui, ctx);
            plot_ui
                .line(Line::new("cure_depth", PlotPoints::from(cure)).color(theme::SERIES_GREEN));
            plot_ui.line(
                Line::new("worst_cure_depth", PlotPoints::from(worst)).color(theme::SERIES_YELLOW),
            );
            if let Some(threshold) = height_threshold {
                plot_ui.hline(
                    HLine::new("layer_height", threshold)
                        .color(theme::THRESHOLD_AMBER)
                        .width(1.0_f32),
                );
            }
            plot_ui.vline(cursor_vline(ctx.cursor_layer));
        });
}

fn median_finite(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut s: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = s.len() / 2;
    if s.len() % 2 == 0 {
        Some((s[mid - 1] + s[mid]) / 2.0)
    } else {
        Some(s[mid])
    }
}
