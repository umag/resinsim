//! Pure data-shape projection of `PrintSimulation` into the f64
//! line-series shape egui_plot consumes, plus the `render_plots`
//! and `render_layer_timeline` helpers that draw the time-axis
//! stack (right inspector, issue 04) and the layer-axis chart
//! (bottom panel, issue 05) respectively.
//!
//! Splitting `build_plot_data` / `build_layer_chart_data` (pure)
//! from the egui-touching render fns keeps the load-bearing math
//! testable on a plugin-less Bevy App per
//! `docs/patterns/bevy-app-test-seam.md`.

use bevy_egui::egui;
use egui_plot::{GridMark, Legend, Line, Plot, PlotPoint, PlotPoints, Text, VLine};
use resinsim_core::simulation::PrintSimulation;
use std::ops::RangeInclusive;

use crate::ui::state::BottomPanelState;

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

// ---------------------------------------------------------------------------
// Issue 05: layer-axis line chart (bottom panel)
// ---------------------------------------------------------------------------

/// One filtered (x, y) line for the bottom-panel chart. `points` carries
/// only finite samples — non-finite or domain-violating values (e.g.
/// `safety_factor = ∞` on zero-force layers) are dropped at projection
/// time, so the render path passes `points` straight to
/// `PlotPoints::from(...)` without re-filtering. Shorter than
/// `sim.layers().len()` when the source data has gaps.
///
/// `name` carries the unit suffix (e.g. "Peel force (N)") so the
/// legend and tooltip both surface it without a separate field.
#[derive(Debug, Clone)]
pub struct LayerSeries {
    pub name: &'static str,
    pub points: Vec<[f64; 2]>,
}

/// Three filtered series ready for the bottom-panel layer chart. The
/// `safety` series is in linear or log10 space depending on the
/// `log_safety` argument to `build_layer_chart_data`.
#[derive(Debug, Clone)]
pub struct LayerChartData {
    pub peel: LayerSeries,
    pub cure: LayerSeries,
    pub safety: LayerSeries,
}

/// Project a `PrintSimulation` into the three layer-axis series the
/// bottom panel consumes. Pure, plugin-less — the issue-05 sibling of
/// `build_plot_data`.
///
/// Filtering rules per series:
///   - `peel`: includes every layer whose `peel_force_n` is finite.
///   - `cure`: includes every layer whose `cure_depth_um` is finite.
///   - `safety` (linear): includes every layer whose `safety_factor`
///     is finite (drops `f32::INFINITY` from zero-force layers per
///     `safety-factor-zero-force.md`).
///   - `safety` (log10, when `log_safety = true`): additionally drops
///     non-positive values — log10 is undefined there. The y carried
///     in the points is `log10(sf)`, not `sf`.
pub fn build_layer_chart_data(sim: &PrintSimulation, log_safety: bool) -> LayerChartData {
    let layers = sim.layers();
    let n = layers.len();
    let mut peel: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut cure: Vec<[f64; 2]> = Vec::with_capacity(n);
    let mut safety: Vec<[f64; 2]> = Vec::with_capacity(n);

    for (i, layer) in layers.iter().enumerate() {
        let x = i as f64;
        let p = f64::from(layer.peel_force_n);
        if p.is_finite() {
            peel.push([x, p]);
        }
        let c = f64::from(layer.cure_depth_um);
        if c.is_finite() {
            cure.push([x, c]);
        }
        let sf = f64::from(layer.safety_factor);
        if sf.is_finite() {
            if log_safety {
                if sf > 0.0 {
                    safety.push([x, sf.log10()]);
                }
                // sf <= 0.0 is undefined under log10 — gap.
            } else {
                safety.push([x, sf]);
            }
        }
        // sf non-finite (e.g. INFINITY for zero-force layers) — gap.
    }

    LayerChartData {
        peel: LayerSeries {
            name: "Peel force (N)",
            points: peel,
        },
        cure: LayerSeries {
            name: "Cure depth (µm)",
            points: cure,
        },
        safety: LayerSeries {
            name: if log_safety {
                "Safety factor (log10)"
            } else {
                "Safety factor (×)"
            },
            points: safety,
        },
    }
}

/// Snap a continuous plot x-coordinate to a discrete layer index,
/// clamping out-of-range values to the bounds. Pure helper — the
/// load-bearing test seam for click-to-seek per
/// `docs/patterns/bevy-app-test-seam.md` (egui closures aren't
/// unit-testable; the math is).
///
/// Returns `None` only when there are no layers (no valid index
/// exists). For any non-empty `layer_count`:
///   - `x ≤ -0.5` → `Some(0)`
///   - `x ≥ layer_count - 0.5` → `Some(layer_count - 1)`
///   - in between: `Some(round(x))`
///
/// Round-to-nearest is intentional: a click halfway between two layers
/// snaps to the nearer one, matching user expectation. Distinct from
/// arrow-key step semantics (saturating ±1 from current) — different
/// mental models for continuous click vs discrete keypress, see
/// ADR-0014.
pub fn snap_plot_x_to_layer(x: f64, layer_count: u32) -> Option<u32> {
    if layer_count == 0 {
        return None;
    }
    let max_idx = (layer_count - 1) as f64;
    let clamped = x.clamp(0.0, max_idx);
    Some(clamped.round() as u32)
}

/// Compute a sensible top-y for the cursor-label position, derived
/// directly from the projected enabled-series finite y values.
///
/// Why not `plot_ui.plot_bounds().max()[1]`? On the first paint after
/// a Run, egui_plot's auto-bounds aren't fully resolved inside the
/// closure until lines are added — querying `plot_bounds()` before
/// `plot_ui.line(...)` calls returns stale defaults. Deriving from
/// the data is deterministic and frame-1 correct.
///
/// Fallback: `1.0` when no series is enabled or all are empty (label
/// renders at y = 1.0; visually fine since the plot will auto-fit
/// around any subsequent data and the label is decorative).
fn cursor_label_top_y(data: &LayerChartData, state: &BottomPanelState) -> f64 {
    let mut max_y = f64::NEG_INFINITY;
    let mut consider = |s: &LayerSeries| {
        for p in &s.points {
            if p[1].is_finite() && p[1] > max_y {
                max_y = p[1];
            }
        }
    };
    if state.show_peel {
        consider(&data.peel);
    }
    if state.show_cure {
        consider(&data.cure);
    }
    if state.show_safety {
        consider(&data.safety);
    }
    if max_y.is_finite() {
        max_y
    } else {
        1.0
    }
}

/// Render the layer-axis chart into `ui` and return `Some(layer)` when
/// the user clicks inside the plot (caller writes it into
/// `CurrentLayer.index`).
///
/// `current` is the in-frame layer index (used for the VLine cursor +
/// label). `max` is `CurrentLayer.max` (i.e. `layers.len() - 1`); when
/// `data.peel.points + cure.points + safety.points` is all empty (no
/// run yet), the function still mounts a placeholder Plot so the
/// layout doesn't jump on first Run.
///
/// On every frame, the function detects whether the visibility flags
/// changed since the last paint via `state.prev_visibility`; on
/// change, it asks egui_plot to re-fit Y so toggling a series doesn't
/// leave bounds stale. (egui_plot caches plot bounds across frames
/// keyed on Plot ID — see ADR-0014.)
#[allow(clippy::too_many_arguments)]
pub fn render_layer_timeline(
    ui: &mut egui::Ui,
    data: &LayerChartData,
    current: u32,
    max: u32,
    state: &mut BottomPanelState,
) -> Option<u32> {
    // --- Pre-plot row: visibility + log-scale toggles ---
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_peel, "Peel force (N)");
        ui.checkbox(&mut state.show_cure, "Cure depth (µm)");
        ui.checkbox(&mut state.show_safety, "Safety factor");
        // Log-scale toggle is a sub-option of Safety — only meaningful
        // when the safety series is on. Disabling Safety also resets
        // the log toggle so re-enabling Safety later starts in linear
        // mode (less surprising than silently remembering log).
        if state.show_safety {
            ui.checkbox(&mut state.safety_log_scale, "log10");
        } else if state.safety_log_scale {
            state.safety_log_scale = false;
        }
    });

    // --- Detect visibility change to force Y re-fit ---
    let cur_vis = (
        state.show_peel,
        state.show_cure,
        state.show_safety,
        state.safety_log_scale,
    );
    let force_refit = state.prev_visibility != cur_vis;
    state.prev_visibility = cur_vis;

    let label_top_y = cursor_label_top_y(data, state);
    let layer_count = max.saturating_add(1);

    // --- Plot body ---
    let response = Plot::new("plot-layer-timeline")
        .x_axis_label("Layer")
        .y_axis_label("Value (mixed units — see series)")
        .legend(Legend::default())
        .label_formatter(|name, value: &PlotPoint| {
            // Tooltip carries unit per series via the leading `name`
            // (egui_plot uses the series name we passed to `Line::new`).
            format!("{name}\nlayer {}\n{:.3}", value.x.round() as i64, value.y)
        })
        .show(ui, |plot_ui| {
            if force_refit {
                plot_ui.set_auto_bounds([true, true]);
            }

            // Series first so the cursor + label overlay them.
            if state.show_peel {
                plot_ui.line(Line::new(
                    data.peel.name,
                    PlotPoints::from(data.peel.points.clone()),
                ));
            }
            if state.show_cure {
                plot_ui.line(Line::new(
                    data.cure.name,
                    PlotPoints::from(data.cure.points.clone()),
                ));
            }
            if state.show_safety {
                plot_ui.line(Line::new(
                    data.safety.name,
                    PlotPoints::from(data.safety.points.clone()),
                ));
            }

            // Cursor + label only meaningful when we have at least one layer.
            if layer_count > 0 {
                let cur_x = current as f64;
                plot_ui.vline(VLine::new("layer-cursor", cur_x));
                plot_ui.text(Text::new(
                    "layer-cursor-label",
                    PlotPoint::new(cur_x, label_top_y),
                    egui::RichText::new(format!("Layer {}", current.saturating_add(1))).small(),
                ));
            }

            // Click handling: closure return value carries the snapped
            // layer up through PlotResponse<R>::inner.
            if plot_ui.response().clicked()
                && let Some(p) = plot_ui.pointer_coordinate()
            {
                snap_plot_x_to_layer(p.x, layer_count)
            } else {
                None
            }
        });

    response.inner
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

    // ---- Issue 05: build_layer_chart_data ----

    #[test]
    fn build_layer_chart_data_lengths_match_layers_when_all_finite() {
        let sim = cube_sim(100);
        let d = build_layer_chart_data(&sim, false);
        // Cube sim produces non-zero peel + finite cure + finite SF
        // for every layer — all three series should be parallel-indexed.
        assert_eq!(d.peel.points.len(), 100);
        assert_eq!(d.cure.points.len(), 100);
        assert_eq!(d.safety.points.len(), 100);
        // X coordinates monotonic 0..100.
        for (i, p) in d.peel.points.iter().enumerate() {
            assert_eq!(p[0], i as f64, "peel x must equal layer index at i={i}");
        }
        for (i, p) in d.cure.points.iter().enumerate() {
            assert_eq!(p[0], i as f64);
        }
        for (i, p) in d.safety.points.iter().enumerate() {
            assert_eq!(p[0], i as f64);
        }
    }

    /// Construct a synthetic 3-layer sim with a zero-force middle
    /// layer (safety_factor = ∞) to exercise the projection's
    /// non-finite filter. Direct LayerResult construction bypasses
    /// the failure_predictor; that's intentional — we're testing the
    /// projection's filter, not the predictor.
    fn three_layer_sim_with_inf_safety() -> PrintSimulation {
        use resinsim_core::entities::LayerResult;
        let repos = shipped_repos();
        let resin = repos
            .resin
            .load("generic_standard")
            .expect("test fixture: shipped resin");
        let printer = repos
            .printer
            .load("generic_msla_4k")
            .expect("test fixture: shipped printer");
        let mut sim = PrintSimulation::new(resin.recipe().clone(), printer);
        let mk = |idx: u32, peel: f32, sf: f32| LayerResult {
            index: idx,
            cure_depth_um: 100.0,
            peel_force_n: peel,
            suction_force_n: 0.0,
            base_force_n: 0.0,
            peel_shape_factor: None,
            total_force_n: peel,
            support_capacity_n: peel * sf.max(1.0),
            safety_factor: sf,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: 22.0,
            viscosity_mpa_s: 200.0,
            z_deflection_um: 1.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: 100.0,
            strain_magnitude_max: None,
            stress_von_mises_max_mpa: None,
            strain_gradient_max_frac: None,
            voxel_yield_fraction: None,
        };
        sim.add_layer(mk(0, 5.0, 3.0), vec![])
            .expect("test fixture: index 0 matches empty layer count");
        sim.add_layer(mk(1, 0.0, f32::INFINITY), vec![])
            .expect("test fixture: index 1 matches layer count 1");
        sim.add_layer(mk(2, 5.0, 3.0), vec![])
            .expect("test fixture: index 2 matches layer count 2");
        sim
    }

    #[test]
    fn build_layer_chart_data_safety_filters_inf() {
        let sim = three_layer_sim_with_inf_safety();
        let d = build_layer_chart_data(&sim, false);
        // Peel + cure unaffected — every layer has a finite reading.
        assert_eq!(d.peel.points.len(), 3);
        assert_eq!(d.cure.points.len(), 3);
        // Safety drops the ∞-SF middle layer.
        assert_eq!(d.safety.points.len(), 2);
        let xs: Vec<f64> = d.safety.points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.0, 2.0], "safety x must skip the ∞ layer");
        for p in &d.safety.points {
            assert!(p[1].is_finite(), "all surviving samples must be finite");
        }
    }

    #[test]
    fn build_layer_chart_data_log_safety_omits_non_positive_and_non_finite() {
        use resinsim_core::entities::LayerResult;
        let repos = shipped_repos();
        let resin = repos
            .resin
            .load("generic_standard")
            .expect("test fixture: shipped resin");
        let printer = repos
            .printer
            .load("generic_msla_4k")
            .expect("test fixture: shipped printer");
        let mut sim = PrintSimulation::new(resin.recipe().clone(), printer);
        let mk = |idx: u32, sf: f32| LayerResult {
            index: idx,
            cure_depth_um: 100.0,
            peel_force_n: 5.0,
            suction_force_n: 0.0,
            base_force_n: 0.0,
            peel_shape_factor: None,
            total_force_n: 5.0,
            support_capacity_n: 5.0 * sf.max(1.0),
            safety_factor: sf,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: 22.0,
            viscosity_mpa_s: 200.0,
            z_deflection_um: 1.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: 100.0,
            strain_magnitude_max: None,
            stress_von_mises_max_mpa: None,
            strain_gradient_max_frac: None,
            voxel_yield_fraction: None,
        };
        // Mix of: positive, zero, negative, infinite.
        for (idx, layer) in [
            mk(0, 10.0),
            mk(1, 0.0),
            mk(2, -1.0),
            mk(3, f32::INFINITY),
            mk(4, 100.0),
        ]
        .into_iter()
        .enumerate()
        {
            sim.add_layer(layer, vec![])
                .unwrap_or_else(|e| panic!("test fixture: index {idx} sequential add: {e}"));
        }

        let d = build_layer_chart_data(&sim, true);
        // Only positive + finite SFs survive log mode.
        let surviving: Vec<(f64, f64)> = d.safety.points.iter().map(|p| (p[0], p[1])).collect();
        assert_eq!(surviving.len(), 2, "got {surviving:?}");
        assert_eq!(surviving[0].0, 0.0);
        assert!((surviving[0].1 - 1.0).abs() < 1e-9, "log10(10) ≈ 1");
        assert_eq!(surviving[1].0, 4.0);
        assert!((surviving[1].1 - 2.0).abs() < 1e-9, "log10(100) = 2");
    }

    #[test]
    fn build_layer_chart_data_empty_simulation() {
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
        let d = build_layer_chart_data(&sim, false);
        assert!(d.peel.points.is_empty());
        assert!(d.cure.points.is_empty());
        assert!(d.safety.points.is_empty());
    }

    #[test]
    fn build_layer_chart_data_safety_name_changes_with_log_mode() {
        let sim = cube_sim(2);
        let linear = build_layer_chart_data(&sim, false);
        let log = build_layer_chart_data(&sim, true);
        assert!(
            linear.safety.name.contains("(×)"),
            "linear name carries unit"
        );
        assert!(log.safety.name.contains("log10"), "log name carries log10");
    }

    // ---- Issue 05: snap_plot_x_to_layer ----

    #[test]
    fn snap_plot_x_to_layer_empty_count_is_none() {
        assert_eq!(snap_plot_x_to_layer(0.0, 0), None);
        assert_eq!(snap_plot_x_to_layer(50.0, 0), None);
    }

    #[test]
    fn snap_plot_x_to_layer_clamps_below_zero() {
        assert_eq!(snap_plot_x_to_layer(-10.0, 100), Some(0));
        assert_eq!(snap_plot_x_to_layer(-0.4, 100), Some(0));
    }

    #[test]
    fn snap_plot_x_to_layer_clamps_above_max() {
        assert_eq!(snap_plot_x_to_layer(200.0, 100), Some(99));
        assert_eq!(snap_plot_x_to_layer(99.5, 100), Some(99));
    }

    #[test]
    fn snap_plot_x_to_layer_rounds_to_nearest() {
        assert_eq!(snap_plot_x_to_layer(3.6, 100), Some(4));
        assert_eq!(snap_plot_x_to_layer(3.4, 100), Some(3));
        assert_eq!(snap_plot_x_to_layer(3.5, 100), Some(4)); // round half-up
    }

    #[test]
    fn snap_plot_x_to_layer_in_range_round_trip() {
        for i in 0..100u32 {
            assert_eq!(snap_plot_x_to_layer(f64::from(i), 100), Some(i));
        }
    }

    // ---- Issue 05: cursor_label_top_y (private helper) ----

    #[test]
    fn cursor_label_top_y_uses_max_of_enabled_series() {
        use crate::ui::state::BottomPanelState;
        let data = LayerChartData {
            peel: LayerSeries {
                name: "p",
                points: vec![[0.0, 1.0], [1.0, 5.0]],
            },
            cure: LayerSeries {
                name: "c",
                points: vec![[0.0, 100.0], [1.0, 150.0]],
            },
            safety: LayerSeries {
                name: "s",
                points: vec![[0.0, 3.0]],
            },
        };
        // Default: peel only.
        let state = BottomPanelState::default();
        assert_eq!(cursor_label_top_y(&data, &state), 5.0);
        // Cure on (large): bumps top_y.
        let state = BottomPanelState {
            show_peel: true,
            show_cure: true,
            show_safety: false,
            safety_log_scale: false,
            prev_visibility: (true, true, false, false),
        };
        assert_eq!(cursor_label_top_y(&data, &state), 150.0);
        // No series enabled → fallback 1.0.
        let state = BottomPanelState {
            show_peel: false,
            show_cure: false,
            show_safety: false,
            safety_log_scale: false,
            prev_visibility: (false, false, false, false),
        };
        assert_eq!(cursor_label_top_y(&data, &state), 1.0);
    }
}
