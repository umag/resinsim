//! Forces pane: peel + suction + total + support_capacity threshold.
//!
//! The most load-bearing pane in the dashboard. Bottom-layer peel
//! spikes are typically 5–20× steady-state, so auto-bounds would
//! flatten the rest of the run; we cap the default Y at p95 × 1.5
//! and let the user pinch out (per-pane Y zoom) to see the spike.
//! The amber `support_capacity` threshold sits at the median of the
//! per-layer support capacity series — robust to a single bottom-
//! layer outlier.

use bevy_egui::egui;
use egui_plot::{HLine, Line, PlotPoints};
use resinsim_core::simulation::PrintSimulation;

use crate::ui::v2::pane::{
    configure_plot, consume_plot_inputs, cursor_vline, pane_header, render_pane_states, PaneCtx,
    PaneId,
};
use crate::ui::v2::theme;
use crate::ui::v2::zoom::data_y_bounds;

#[derive(Default)]
pub struct ForcesPane;

impl ForcesPane {
    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &PaneCtx<'_>) {
        pane_header(ui, "Forces");
        render_pane_states(ui, ctx, PaneId::Forces, "Layer", "N", draw_loaded);
    }
}

fn draw_loaded(ui: &mut egui::Ui, sim: &PrintSimulation, ctx: &PaneCtx<'_>) {
    let layers = sim.layers();
    let mut peel: Vec<[f64; 2]> = Vec::with_capacity(layers.len());
    let mut suction: Vec<[f64; 2]> = Vec::with_capacity(layers.len());
    let mut total: Vec<[f64; 2]> = Vec::with_capacity(layers.len());
    let mut support_caps: Vec<f64> = Vec::with_capacity(layers.len());
    for (i, layer) in layers.iter().enumerate() {
        let x = i as f64;
        push_finite(&mut peel, x, layer.peel_force_n);
        push_finite(&mut suction, x, layer.suction_force_n);
        push_finite(&mut total, x, layer.total_force_n);
        let cap = f64::from(layer.support_capacity_n);
        if cap.is_finite() {
            support_caps.push(cap);
        }
    }
    let support_threshold = median(&support_caps);
    // Default Y bounds fit the combined peel+suction+total range
    // (per "scale graphs based on max value on graph" — the
    // support_capacity threshold may sit one or two orders of
    // magnitude above steady-state and would otherwise flatten the
    // signal). User pinches out to see the threshold.
    let combined: Vec<f64> = peel
        .iter()
        .chain(suction.iter())
        .chain(total.iter())
        .map(|p| p[1])
        .collect();
    let (lo, hi) = data_y_bounds(&combined, 0.08).unwrap_or((0.0, 20.0));
    let mut plot = configure_plot(PaneId::Forces, "Layer", "N", ctx.link_group);
    // Force is non-negative by construction, so floor at 0 keeps the
    // visual baseline anchored even when min sits above 0.
    plot = plot.default_y_bounds(lo.min(0.0), hi);

    plot.show(ui, |plot_ui| {
        consume_plot_inputs(plot_ui, ctx);
        plot_ui.line(Line::new("Peel", PlotPoints::from(peel)).color(theme::SERIES_GREEN));
        plot_ui.line(Line::new("Suction", PlotPoints::from(suction)).color(theme::SERIES_CYAN));
        plot_ui.line(Line::new("Total", PlotPoints::from(total)).color(theme::SERIES_YELLOW));
        if let Some(threshold) = support_threshold {
            plot_ui.hline(
                HLine::new("support_capacity", threshold)
                    .color(theme::THRESHOLD_AMBER)
                    .width(1.0_f32),
            );
        }
        plot_ui.vline(cursor_vline(ctx.cursor_layer));
    });
}

fn push_finite(out: &mut Vec<[f64; 2]>, x: f64, y: f32) {
    let y = f64::from(y);
    if y.is_finite() {
        out.push([x, y]);
    }
}

/// Robust central tendency for the support_capacity threshold line.
/// Median over mean because a single bottom-layer outlier shouldn't
/// shift the rendered threshold.
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    } else {
        Some(sorted[mid])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_empty_is_none() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[f64::NAN, f64::INFINITY]), None);
    }

    #[test]
    fn median_odd_count() {
        assert_eq!(median(&[1.0, 3.0, 2.0]), Some(2.0));
    }

    #[test]
    fn median_even_count() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn median_filters_non_finite() {
        let v = [1.0, 2.0, f64::NAN, 3.0, f64::INFINITY, 4.0, 5.0];
        assert_eq!(median(&v), Some(3.0));
    }

    #[test]
    fn median_resists_outlier() {
        let v = [1.0, 1.0, 1.0, 100.0];
        assert_eq!(median(&v), Some(1.0));
    }

    #[test]
    fn push_finite_drops_nan_and_inf() {
        let mut out: Vec<[f64; 2]> = Vec::new();
        push_finite(&mut out, 0.0, 1.0_f32);
        push_finite(&mut out, 1.0, f32::NAN);
        push_finite(&mut out, 2.0, f32::INFINITY);
        push_finite(&mut out, 3.0, 4.5_f32);
        assert_eq!(out, vec![[0.0, 1.0], [3.0, 4.5]]);
    }
}
