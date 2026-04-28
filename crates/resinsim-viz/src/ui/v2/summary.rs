//! Summary strip — top-band chips showing the dashboard-level
//! signals that decide "is this run worth opening": max-force
//! layer, min-safety layer, total time, total-layer count.
//!
//! Slice B in `spec/viz-v2-design-brief.md` §5. Per brief §7,
//! `max-force layer N` and `min-safety layer N` chips are
//! click-to-jump: clicking returns the layer index so the parent
//! system writes it into `CurrentLayer.index`. The remaining
//! chips (run-tag + total time + total layer count) are read-only.
//!
//! Run-tag default (per brief §8) is the filename stem of the
//! loaded sim.json. That requires plumbing the load path through
//! `LoadedSimulation`; for now we show a placeholder `(unnamed)`
//! when no obvious tag is available.

use std::path::Path;

use bevy_egui::egui;
use resinsim_core::entities::LayerResult;
use resinsim_core::simulation::PrintSimulation;

use super::theme;

pub const SUMMARY_HEIGHT_PX: f32 = 48.0;

/// Render the summary strip. Returns `Some(layer)` if the user
/// clicked a layer-jump chip (max-force / min-safety).
///
/// `source_path` is the filesystem path of the loaded sim.json, used
/// to render the run-tag chip per brief §8 ("filename stem of the
/// sim.json"). `None` falls back to a muted placeholder.
pub fn render(
    ui: &mut egui::Ui,
    sim: Option<&PrintSimulation>,
    source_path: Option<&Path>,
) -> Option<u32> {
    let mut jump_to: Option<u32> = None;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(run_tag(sim, source_path))
                .monospace()
                .color(theme::INK),
        );
        separator(ui);
        ui.label(
            egui::RichText::new(format!("total {}", format_total_time(total_time_seconds(sim))))
                .monospace()
                .color(theme::INK_MUTED),
        );
        separator(ui);
        if let Some((layer, force)) = max_force_layer(sim) {
            if chip(
                ui,
                &format!("max-force layer {layer:0>4} · {force:.1} N"),
            ) {
                jump_to = Some(layer);
            }
        }
        separator(ui);
        if let Some((layer, sf)) = min_safety_layer(sim) {
            if chip(
                ui,
                &format!("min-safety layer {layer:0>4} · {sf:.2}"),
            ) {
                jump_to = Some(layer);
            }
        }
        // Right-aligned: total layer count.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            if let Some(s) = sim {
                ui.label(
                    egui::RichText::new(format!("{} layers", s.layers().len()))
                        .monospace()
                        .color(theme::INK_MUTED),
                );
            } else {
                ui.label(
                    egui::RichText::new("(no run loaded)")
                        .monospace()
                        .color(theme::INK_MUTED),
                );
            }
        });
    });

    jump_to
}

fn chip(ui: &mut egui::Ui, label: &str) -> bool {
    let resp = ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .monospace()
                .color(theme::INK),
        )
        .fill(theme::SURFACE_HIGH)
        .stroke(egui::Stroke::new(1.0_f32, theme::GRID_LINE)),
    );
    resp.clicked()
}

fn separator(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("·").color(theme::INK_MUTED));
}

/// Run-tag chip text. Per brief §8: "Run-tag default: filename
/// stem of the sim.json. User can rename inline." Inline rename
/// lives behind a future settings surface; for now we derive
/// directly from the path. `None` sim falls back to a muted
/// placeholder; `None` path while a sim is loaded falls back to
/// `[ run ]` (this can happen for a sim constructed in-process,
/// not via load_from_path — none exist today, but the case is
/// total so we cover it).
fn run_tag(sim: Option<&PrintSimulation>, source_path: Option<&Path>) -> String {
    match (sim, source_path) {
        (None, _) => "[ — ]".to_string(),
        (Some(_), None) => "[ run ]".to_string(),
        (Some(_), Some(p)) => format!("[ {} ]", sim_filename_stem(p)),
    }
}

/// Filename stem with the brief's `.sim.json` compound extension
/// stripped — so `/tmp/lilith-torso.sim.json` reads as
/// `lilith-torso`, not `lilith-torso.sim`. Falls back to
/// `file_stem()` for non-compound extensions, and to `"(unnamed)"`
/// for paths with no filename. Pure helper for unit tests.
pub fn sim_filename_stem(path: &Path) -> String {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return "(unnamed)".to_string();
    };
    let lower = name.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix(".sim.json") {
        // Preserve original case in the returned stem.
        name[..stripped.len()].to_string()
    } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        stem.to_string()
    } else {
        name.to_string()
    }
}

/// `sim.cumulative_times_sec()` reads the per-layer time stack.
/// The last value is the total time. Returns 0.0 for missing /
/// empty sim.
fn total_time_seconds(sim: Option<&PrintSimulation>) -> f64 {
    sim.map(|s| {
        s.cumulative_times_sec()
            .last()
            .copied()
            .map(f64::from)
            .unwrap_or(0.0)
    })
    .unwrap_or(0.0)
}

/// Find the layer with the largest `total_force_n`. Pure helper
/// for unit tests. Returns `(layer_index, force_value)` or `None`
/// for an empty / non-finite-only sim.
pub fn max_force_layer(sim: Option<&PrintSimulation>) -> Option<(u32, f32)> {
    let s = sim?;
    layer_max(s.layers(), |l| l.total_force_n)
}

/// Find the layer with the smallest finite `safety_factor`. Pure
/// helper for unit tests. Drops non-finite values (e.g.
/// `f32::INFINITY` on zero-force layers per
/// `safety-factor-zero-force.md`).
pub fn min_safety_layer(sim: Option<&PrintSimulation>) -> Option<(u32, f32)> {
    let s = sim?;
    layer_min_finite(s.layers(), |l| l.safety_factor)
}

fn layer_max<F: Fn(&LayerResult) -> f32>(
    layers: &[LayerResult],
    field: F,
) -> Option<(u32, f32)> {
    let mut best: Option<(u32, f32)> = None;
    for (i, l) in layers.iter().enumerate() {
        let v = field(l);
        if !v.is_finite() {
            continue;
        }
        match best {
            None => best = Some((i as u32, v)),
            Some((_, prev)) if v > prev => best = Some((i as u32, v)),
            _ => {}
        }
    }
    best
}

fn layer_min_finite<F: Fn(&LayerResult) -> f32>(
    layers: &[LayerResult],
    field: F,
) -> Option<(u32, f32)> {
    let mut best: Option<(u32, f32)> = None;
    for (i, l) in layers.iter().enumerate() {
        let v = field(l);
        if !v.is_finite() {
            continue;
        }
        match best {
            None => best = Some((i as u32, v)),
            Some((_, prev)) if v < prev => best = Some((i as u32, v)),
            _ => {}
        }
    }
    best
}

/// Format a duration in seconds as `Xh Ym Zs`. Per brief §8:
/// "1h 23m 47s in summary". Zero-padding is omitted on the leading
/// component but compact on the rest (matches the wall-clock
/// reading style for prints that span hours).
///
/// Pure helper for unit tests. Negative / non-finite inputs
/// collapse to "0s" rather than panicking — the formatter is
/// total over its input domain.
pub fn format_total_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0s".to_string();
    }
    let total = seconds.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_total_time_zero() {
        assert_eq!(format_total_time(0.0), "0s");
        assert_eq!(format_total_time(-5.0), "0s");
        assert_eq!(format_total_time(f64::NAN), "0s");
    }

    #[test]
    fn format_total_time_under_a_minute() {
        assert_eq!(format_total_time(47.0), "47s");
        assert_eq!(format_total_time(59.0), "59s");
    }

    #[test]
    fn format_total_time_under_an_hour() {
        assert_eq!(format_total_time(60.0), "1m 0s");
        assert_eq!(format_total_time(120.0), "2m 0s");
        assert_eq!(format_total_time(3599.0), "59m 59s");
    }

    #[test]
    fn format_total_time_multi_hour() {
        assert_eq!(format_total_time(3600.0), "1h 0m 0s");
        assert_eq!(format_total_time(3661.0), "1h 1m 1s");
        assert_eq!(format_total_time(5648.0), "1h 34m 8s");
    }

    #[test]
    fn format_total_time_rounds_subseconds() {
        assert_eq!(format_total_time(59.6), "1m 0s");
        assert_eq!(format_total_time(59.4), "59s");
    }

    // Layer-max / min tests use a tiny synthetic LayerResult set
    // rather than building a full PrintSimulation; that keeps the
    // test self-contained and exercises the pure helper logic.

    fn mk_layer(index: u32, total_force_n: f32, safety_factor: f32) -> LayerResult {
        LayerResult {
            index,
            cure_depth_um: 100.0,
            peel_force_n: total_force_n,
            suction_force_n: 0.0,
            total_force_n,
            support_capacity_n: 100.0,
            safety_factor,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: 22.0,
            viscosity_mpa_s: 200.0,
            z_deflection_um: 1.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: 100.0,
        }
    }

    #[test]
    fn layer_max_returns_max_index() {
        let layers = vec![
            mk_layer(0, 5.0, 3.0),
            mk_layer(1, 20.0, 2.0),
            mk_layer(2, 8.0, 4.0),
        ];
        let r = layer_max(&layers, |l| l.total_force_n);
        assert_eq!(r, Some((1, 20.0)));
    }

    #[test]
    fn layer_max_ignores_non_finite() {
        let layers = vec![
            mk_layer(0, 5.0, 3.0),
            mk_layer(1, f32::NAN, 2.0),
            mk_layer(2, f32::INFINITY, 4.0),
            mk_layer(3, 8.0, 4.0),
        ];
        let r = layer_max(&layers, |l| l.total_force_n);
        assert_eq!(r, Some((3, 8.0)));
    }

    #[test]
    fn layer_max_empty_is_none() {
        let layers: Vec<LayerResult> = vec![];
        assert_eq!(layer_max(&layers, |l| l.total_force_n), None);
    }

    #[test]
    fn layer_min_finite_drops_infinity() {
        let layers = vec![
            mk_layer(0, 5.0, 3.0),
            mk_layer(1, 0.0, f32::INFINITY), // zero-force layer
            mk_layer(2, 8.0, 1.5),
        ];
        let r = layer_min_finite(&layers, |l| l.safety_factor);
        assert_eq!(r, Some((2, 1.5)));
    }

    // ---- sim_filename_stem ----

    #[test]
    fn filename_stem_strips_sim_json_compound_extension() {
        assert_eq!(
            sim_filename_stem(Path::new("/tmp/lilith-torso.sim.json")),
            "lilith-torso"
        );
    }

    #[test]
    fn filename_stem_case_insensitive_match_preserves_case() {
        assert_eq!(
            sim_filename_stem(Path::new("/x/Lilith-Torso.Sim.Json")),
            "Lilith-Torso"
        );
    }

    #[test]
    fn filename_stem_falls_back_for_non_sim_json_extension() {
        assert_eq!(
            sim_filename_stem(Path::new("/x/foo.json")),
            "foo"
        );
        assert_eq!(
            sim_filename_stem(Path::new("/x/bar.ctb")),
            "bar"
        );
    }

    #[test]
    fn filename_stem_no_extension() {
        assert_eq!(sim_filename_stem(Path::new("/x/plain")), "plain");
    }

    #[test]
    fn filename_stem_empty_or_directory_returns_unnamed() {
        // `Path::file_name` returns None for "/" and "."
        assert_eq!(sim_filename_stem(Path::new("/")), "(unnamed)");
    }

    #[test]
    fn filename_stem_handles_relative_path() {
        assert_eq!(
            sim_filename_stem(Path::new("fixtures/lilith-torso.sim.json")),
            "lilith-torso"
        );
    }

    #[test]
    fn layer_min_finite_ties_pick_first() {
        let layers = vec![
            mk_layer(0, 5.0, 2.0),
            mk_layer(1, 5.0, 2.0),
            mk_layer(2, 5.0, 3.0),
        ];
        let r = layer_min_finite(&layers, |l| l.safety_factor);
        // Strictly-less-than comparison means index 0 wins on tie.
        assert_eq!(r, Some((0, 2.0)));
    }
}
