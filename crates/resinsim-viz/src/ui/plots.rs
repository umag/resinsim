//! Pure data-shape projection of `PrintSimulation` into the f64
//! line-series shape egui_plot consumes, plus the `render_plots`
//! helper that draws the three time-series stacks (Print progress,
//! Forces, Temperature) into a `egui::Ui`.
//!
//! Splitting `build_plot_data` (pure) from `render_plots` (egui)
//! keeps the load-bearing math testable on a plugin-less Bevy App
//! per `docs/patterns/bevy-app-test-seam.md`.

use bevy_egui::egui;
use egui_plot::{GridMark, Legend, Line, Plot, PlotPoint, PlotPoints};
use resinsim_core::simulation::PrintSimulation;
use std::ops::RangeInclusive;

/// Per-layer time-series projection, parallel-indexed with
/// `sim.layers()`. All series have length `sim.layers().len()`.
pub struct PlotData {
    /// Cumulative wall-clock time at the end of each layer (s).
    pub times_sec: Vec<f64>,
    /// Cumulative z height at the end of each layer (mm).
    pub heights_mm: Vec<f64>,
    /// Per-layer peel force (N).
    pub peel_n: Vec<f64>,
    /// Per-layer suction force (N).
    pub suction_n: Vec<f64>,
    /// Per-layer total force (peel + suction + …, N).
    pub total_n: Vec<f64>,
    /// Per-layer vat temperature (°C).
    pub vat_c: Vec<f64>,
    /// Per-layer viscosity (mPa·s).
    pub viscosity_mpa_s: Vec<f64>,
}

/// Project a simulation aggregate into f64 series suitable for
/// egui_plot. Pure, plugin-less — the load-bearing test seam.
pub fn build_plot_data(sim: &PrintSimulation) -> PlotData {
    let times_sec: Vec<f64> = sim
        .cumulative_times_sec()
        .into_iter()
        .map(f64::from)
        .collect();

    let layers = sim.layers();
    let n = layers.len();
    let mut heights_mm: Vec<f64> = Vec::with_capacity(n);
    let mut peel_n: Vec<f64> = Vec::with_capacity(n);
    let mut suction_n: Vec<f64> = Vec::with_capacity(n);
    let mut total_n: Vec<f64> = Vec::with_capacity(n);
    let mut vat_c: Vec<f64> = Vec::with_capacity(n);
    let mut viscosity_mpa_s: Vec<f64> = Vec::with_capacity(n);

    let mut z_running_mm: f64 = 0.0;
    for layer in layers {
        z_running_mm += f64::from(layer.effective_layer_height_um) / 1000.0;
        heights_mm.push(z_running_mm);
        peel_n.push(f64::from(layer.peel_force_n));
        suction_n.push(f64::from(layer.suction_force_n));
        total_n.push(f64::from(layer.total_force_n));
        vat_c.push(f64::from(layer.vat_temperature_c));
        viscosity_mpa_s.push(f64::from(layer.viscosity_mpa_s));
    }

    PlotData {
        times_sec,
        heights_mm,
        peel_n,
        suction_n,
        total_n,
        vat_c,
        viscosity_mpa_s,
    }
}

/// Zip two parallel series into the `Vec<[f64; 2]>` shape egui_plot
/// expects.
fn zip_xy(xs: &[f64], ys: &[f64]) -> Vec<[f64; 2]> {
    xs.iter().zip(ys.iter()).map(|(&x, &y)| [x, y]).collect()
}

/// Approximate `q`-quantile (0.0–1.0) of a slice via sort+pick.
/// Used to compute a meaningful default y-bound for the Force plot
/// when the bottom-layer peel spike (typically 5–20× the steady-state
/// peel) would otherwise compress the steady-state signal into a flat
/// line. Returns `None` for empty inputs (caller falls back to
/// auto-bounds).
fn quantile(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().filter(|v| v.is_finite()).copied().collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

/// True when every value in the series is exactly zero — used to
/// surface a "(no events for this model)" annotation on the Suction
/// line so a flat-zero plot reads as "physics says no" instead of
/// "is this broken?".
fn is_all_zero(values: &[f64]) -> bool {
    !values.is_empty() && values.iter().all(|&v| v == 0.0)
}

/// Format a non-negative duration in seconds as `H:MM:SS`. Used by
/// the time-axis formatter so a 56468 s print reads `15:41:08`
/// instead of `56468`. Negative or non-finite inputs fall back to
/// `{:.0} s` so the formatter is total — egui calls it for every
/// gridline including off-screen ones.
pub(crate) fn format_hms(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return format!("{seconds:.0} s");
    }
    let total = seconds.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h}:{m:02}:{s:02}")
}

/// Render the three stacked time-series plots into `ui`. When
/// `data.is_none()`, renders a single placeholder label.
///
/// Plots share an x-axis link group (`"resinsim-viz-time-axis"`) so
/// zoom/pan on one mirrors to the others. Axis labels and legend
/// wording are locked here per ADR-0011 — the view contract for
/// 05/06/07.
pub fn render_plots(ui: &mut egui::Ui, data: Option<&PlotData>) {
    let Some(d) = data else {
        ui.label("Run simulation to see plots");
        return;
    };

    let link_group = ui.id().with("resinsim-viz-time-axis");
    let time_fmt = |mark: GridMark, _range: &RangeInclusive<f64>| format_hms(mark.value);

    Plot::new("plot-print-progress")
        .height(150.0)
        .x_axis_label("Time (h:mm:ss)")
        .y_axis_label("Z height (mm)")
        .legend(Legend::default())
        .link_axis(link_group, [true, false])
        .link_cursor(link_group, [true, false])
        .x_axis_formatter(time_fmt)
        .label_formatter(|name, value: &PlotPoint| {
            format!("{name}\nt = {}\nz = {:.2} mm", format_hms(value.x), value.y)
        })
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(
                "Z height",
                PlotPoints::from(zip_xy(&d.times_sec, &d.heights_mm)),
            ));
        });

    // Force y-bounds: clip to 95th-percentile × 1.5 so the bottom-layer
    // peel spike (often 5–20× steady-state) doesn't flatten the rest
    // of the signal. User can scroll-zoom out to see the full range.
    let force_y_max = quantile(&d.total_n, 0.95)
        .map(|p| (p * 1.5).max(1.0))
        .unwrap_or(20.0);
    Plot::new("plot-forces")
        .height(150.0)
        .x_axis_label("Time (h:mm:ss)")
        .y_axis_label("Force (N)")
        .legend(Legend::default())
        .link_axis(link_group, [true, false])
        .link_cursor(link_group, [true, false])
        .default_y_bounds(0.0, force_y_max)
        .x_axis_formatter(time_fmt)
        .label_formatter(|name, value: &PlotPoint| {
            format!(
                "{name}\nt = {}\n{name} = {:.2} N",
                format_hms(value.x),
                value.y
            )
        })
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(
                "Peel",
                PlotPoints::from(zip_xy(&d.times_sec, &d.peel_n)),
            ));
            plot_ui.line(Line::new(
                "Suction",
                PlotPoints::from(zip_xy(&d.times_sec, &d.suction_n)),
            ));
            plot_ui.line(Line::new(
                "Total",
                PlotPoints::from(zip_xy(&d.times_sec, &d.total_n)),
            ));
        });
    if is_all_zero(&d.suction_n) {
        ui.label(
            egui::RichText::new("Suction: no enclosed-cavity events for this model.")
                .small()
                .color(egui::Color32::GRAY),
        );
    }

    // Vat temperature and viscosity get their own plots — different
    // scales (typically 30 °C vs 200–450 mPa·s) make a shared y-axis
    // useless. Both still link the x-axis with the others.
    Plot::new("plot-vat-temp")
        .height(150.0)
        .x_axis_label("Time (h:mm:ss)")
        .y_axis_label("Vat temp (°C)")
        .legend(Legend::default())
        .link_axis(link_group, [true, false])
        .link_cursor(link_group, [true, false])
        .x_axis_formatter(time_fmt)
        .label_formatter(|name, value: &PlotPoint| {
            format!(
                "{name}\nt = {}\n{name} = {:.2} °C",
                format_hms(value.x),
                value.y
            )
        })
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(
                "Vat temp",
                PlotPoints::from(zip_xy(&d.times_sec, &d.vat_c)),
            ));
        });

    Plot::new("plot-viscosity")
        .height(150.0)
        .x_axis_label("Time (h:mm:ss)")
        .y_axis_label("Viscosity (mPa·s)")
        .legend(Legend::default())
        .link_axis(link_group, [true, false])
        .link_cursor(link_group, [true, false])
        .x_axis_formatter(time_fmt)
        .label_formatter(|name, value: &PlotPoint| {
            format!(
                "{name}\nt = {}\n{name} = {:.1} mPa·s",
                format_hms(value.x),
                value.y
            )
        })
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(
                "Viscosity",
                PlotPoints::from(zip_xy(&d.times_sec, &d.viscosity_mpa_s)),
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_repos::ProfileRepos;
    use resinsim_core::app::{build_simulation_from_layers, RunRequest};
    use resinsim_core::io::sliced::LayerInput;
    use resinsim_core::simulation::PrintSimulation;
    use resinsim_core::values::LayerMask;
    use std::path::PathBuf;

    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    fn shipped_repos() -> ProfileRepos {
        ProfileRepos::new(&workspace_data_dir())
    }

    /// Synthesise a 100-layer cube-like sim — same shape as
    /// `sim::tests::cube_layer_inputs` but local (the helper is
    /// private to that module's test scope).
    fn cube_sim(n_layers: u32) -> PrintSimulation {
        let layer_height_um = 50.0_f32;
        let exposure_sec = 2.5_f32;
        let lift_speed_mm_min = 60.0_f32;
        let voxel_size_mm = 0.05_f32;
        let layers: Vec<LayerInput> = (0..n_layers)
            .map(|i| {
                let z_mm = (i as f32 + 1.0) * (layer_height_um / 1000.0);
                let mask = LayerMask::new_all_solid(1, 1, voxel_size_mm)
                    .expect("test fixture: 1×1 mask at validated voxel size constructs");
                LayerInput::new(
                    i,
                    100.0,
                    exposure_sec,
                    lift_speed_mm_min,
                    layer_height_um,
                    z_mm,
                )
                .expect(
                    "test fixture: positive exposure + non-negative area satisfy LayerInput::new",
                )
                .with_mask(mask)
            })
            .collect();
        let req = RunRequest::new_with_v1_defaults("generic_standard", "generic_msla_4k", None);
        build_simulation_from_layers(&req, &layers, &shipped_repos().0).expect(
            "test fixture: shipped profiles + cube-like inputs satisfy run_from_layer_inputs",
        )
    }

    #[test]
    fn series_lengths_match_layer_count() {
        let sim = cube_sim(100);
        let d = build_plot_data(&sim);
        assert_eq!(d.times_sec.len(), 100);
        assert_eq!(d.heights_mm.len(), 100);
        assert_eq!(d.peel_n.len(), 100);
        assert_eq!(d.suction_n.len(), 100);
        assert_eq!(d.total_n.len(), 100);
        assert_eq!(d.vat_c.len(), 100);
        assert_eq!(d.viscosity_mpa_s.len(), 100);
    }

    #[test]
    fn times_sec_is_monotonic_non_decreasing() {
        let sim = cube_sim(100);
        let d = build_plot_data(&sim);
        for i in 1..d.times_sec.len() {
            assert!(
                d.times_sec[i] >= d.times_sec[i - 1],
                "times must be non-decreasing at i={i}: {} vs {}",
                d.times_sec[i - 1],
                d.times_sec[i]
            );
        }
    }

    #[test]
    fn heights_mm_is_monotonic_non_decreasing() {
        let sim = cube_sim(100);
        let d = build_plot_data(&sim);
        for i in 1..d.heights_mm.len() {
            assert!(
                d.heights_mm[i] >= d.heights_mm[i - 1],
                "heights must be non-decreasing at i={i}: {} vs {}",
                d.heights_mm[i - 1],
                d.heights_mm[i]
            );
        }
    }

    /// `total_force = peel + suction + …` per
    /// `failure_predictor.rs:275-277` (peel_force_n, suction_force_n,
    /// total_force_n all written from the same predict_layer pass;
    /// total = peel + suction + interlayer, all non-negative).
    #[test]
    fn force_total_at_least_peel() {
        let sim = cube_sim(100);
        let d = build_plot_data(&sim);
        for i in 0..d.peel_n.len() {
            assert!(
                d.total_n[i] >= d.peel_n[i],
                "total >= peel at i={i}: total={}, peel={}",
                d.total_n[i],
                d.peel_n[i]
            );
            assert!(d.total_n[i].is_finite() && d.total_n[i] >= 0.0);
        }
    }

    #[test]
    fn quantile_basic_invariants() {
        // Empty input.
        assert!(super::quantile(&[], 0.5).is_none());
        // Single element.
        assert_eq!(super::quantile(&[3.0], 0.5), Some(3.0));
        // Sorted-input sanity.
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        assert_eq!(super::quantile(&xs, 0.0), Some(0.0));
        assert_eq!(super::quantile(&xs, 1.0), Some(9.0));
        let p50 = super::quantile(&xs, 0.5).expect("non-empty input yields Some");
        assert!((4.0..=5.0).contains(&p50), "p50 ≈ median, got {p50}");
        // Outliers don't blow up p95.
        let with_spike = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 100.0];
        let p95 = super::quantile(&with_spike, 0.95).expect("non-empty input yields Some");
        // p95 of {1×9, 100} index = round(9 * 0.95) = 9 → max element.
        // Important: not the median, not the mean — outlier still
        // surfaces at p95 by construction.
        assert!(p95 >= 1.0, "p95 must be ≥ steady-state value: {p95}");
    }

    #[test]
    fn quantile_filters_non_finite() {
        let xs = [1.0, 2.0, f64::NAN, 3.0, f64::INFINITY];
        let p50 = super::quantile(&xs, 0.5).expect("filter keeps 3 finite values");
        assert!((1.0..=3.0).contains(&p50));
    }

    #[test]
    fn format_hms_basic_cases() {
        assert_eq!(super::format_hms(0.0), "0:00:00");
        assert_eq!(super::format_hms(59.0), "0:00:59");
        assert_eq!(super::format_hms(60.0), "0:01:00");
        assert_eq!(super::format_hms(3661.0), "1:01:01");
        // 56468 s ≈ 15h 41m 8s — the lilith-torso run total time.
        assert_eq!(super::format_hms(56468.0), "15:41:08");
        // Round-trip on fractional seconds — formatter rounds.
        assert_eq!(super::format_hms(60.4), "0:01:00");
        assert_eq!(super::format_hms(60.6), "0:01:01");
    }

    #[test]
    fn format_hms_handles_non_finite_and_negative() {
        // Total formatter — egui calls it for off-screen marks too.
        assert!(super::format_hms(f64::NAN).contains("NaN"));
        assert!(super::format_hms(-5.0).starts_with('-'));
        assert!(super::format_hms(f64::INFINITY).contains("inf"));
    }

    #[test]
    fn is_all_zero_distinguishes_from_empty_and_nonzero() {
        assert!(!super::is_all_zero(&[]), "empty is not 'all zero'");
        assert!(super::is_all_zero(&[0.0, 0.0, 0.0]));
        assert!(!super::is_all_zero(&[0.0, 0.0001, 0.0]));
        assert!(!super::is_all_zero(&[1.0]));
    }

    #[test]
    fn empty_simulation_yields_empty_series() {
        // Construct an empty PrintSimulation directly — running
        // `SimulationRunner` with zero layers fails earlier in the
        // suction-detector pre-pass ("no masks provided"), which
        // isn't the path build_plot_data is exercising.
        let repos = shipped_repos();
        let resin = repos
            .resin
            .load("generic_standard")
            .expect("test fixture: shipped resin");
        let printer = repos
            .printer
            .load("generic_msla_4k")
            .expect("test fixture: shipped printer");
        let sim = PrintSimulation::new(resin.recipe().clone(), printer);
        let d = build_plot_data(&sim);
        assert!(d.times_sec.is_empty());
        assert!(d.heights_mm.is_empty());
        assert!(d.peel_n.is_empty());
        assert!(d.suction_n.is_empty());
        assert!(d.total_n.is_empty());
        assert!(d.vat_c.is_empty());
        assert!(d.viscosity_mpa_s.is_empty());
    }
}
