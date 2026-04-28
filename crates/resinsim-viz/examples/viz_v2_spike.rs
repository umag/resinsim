//! Throwaway spike for the v2 redesign (see ADR-0014 +
//! spec/viz-v2-design-brief.md).
//!
//! Validates that bevy_egui 0.39 carries the three density-critical
//! widgets we need before committing the full redesign:
//!
//!   1. A 2x2 resizable pane grid with three draggable splitters
//!      (one between the two columns, one inside each column).
//!   2. A mono-tabular stats column rendering every LayerResult
//!      field for a cursor layer that auto-advances and is also
//!      slider-controllable.
//!   3. Real time-series plots (egui_plot) inside each pane, with
//!      a shared cursor VLine that moves in lockstep with the
//!      scrubber slider, plus threshold lines (HLine) where the
//!      physics defines a fail boundary.
//!
//! Acceptance is a 5-minute manual exercise. Drag every splitter,
//! scrub through layers (with auto-advance on or via the slider),
//! and verify that:
//!
//! - The cursor VLine in every pane jumps in sync with the scrubber.
//! - The stats column on the right does NOT jiggle horizontally
//!   when values change. If it does, mono-tabular is broken
//!   (likely a fallback-font swap).
//! - Threshold lines on Forces (support_capacity), Cure depth
//!   (effective_layer_height), and Safety factor (1.0) are visible
//!   in muted amber/red, not the same colour as a series.
//!
//! Run with:
//!
//!   cargo run -p resinsim-viz --example viz_v2_spike
//!
//! Not registered as a binary. Safe to delete after the spike clears.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use egui_plot::{HLine, Legend, Line, Plot, PlotPoints, VLine};

const TOTAL_LAYERS: u32 = 4500;
const TICK_INTERVAL_SECS: f32 = 0.25;

// Grafana-classic categorical palette (per DESIGN.md "Series palette").
// Used only for line series, never for chrome.
const SERIES_GREEN: egui::Color32 = egui::Color32::from_rgb(126, 178, 109);
const SERIES_YELLOW: egui::Color32 = egui::Color32::from_rgb(234, 184, 57);
const SERIES_CYAN: egui::Color32 = egui::Color32::from_rgb(110, 208, 224);
const SERIES_ORANGE: egui::Color32 = egui::Color32::from_rgb(239, 132, 60);

// Threshold colours — reserved exclusively for fail/warn boundaries.
const THRESHOLD_RED: egui::Color32 = egui::Color32::from_rgb(190, 90, 90);
const THRESHOLD_AMBER: egui::Color32 = egui::Color32::from_rgb(190, 134, 60);

// Cursor VLine colour — light ink, not categorical, not threshold.
const CURSOR_INK: egui::Color32 = egui::Color32::from_rgb(180, 188, 200);

#[derive(Resource)]
struct Cursor {
    layer: u32,
    auto_advance: bool,
    last_tick_secs: f32,
}

impl Default for Cursor {
    fn default() -> Self {
        Self { layer: 42, auto_advance: true, last_tick_secs: 0.0 }
    }
}

/// Per-layer synthetic time-series. Computed once at startup; every
/// frame just borrows or clones the precomputed Vec into PlotPoints.
/// Values keyed to layer index via deterministic sin/cos so visual
/// regressions surface immediately.
#[derive(Resource)]
struct Series {
    peel_force: Vec<[f64; 2]>,
    suction_force: Vec<[f64; 2]>,
    total_force: Vec<[f64; 2]>,
    support_capacity_n: f64,
    cure_depth: Vec<[f64; 2]>,
    effective_layer_height_um: f64,
    safety_factor: Vec<[f64; 2]>,
    vat_temperature: Vec<[f64; 2]>,
}

impl Series {
    fn synthesize(n: u32) -> Self {
        let mut peel_force = Vec::with_capacity(n as usize);
        let mut suction_force = Vec::with_capacity(n as usize);
        let mut total_force = Vec::with_capacity(n as usize);
        let mut cure_depth = Vec::with_capacity(n as usize);
        let mut safety_factor = Vec::with_capacity(n as usize);
        let mut vat_temperature = Vec::with_capacity(n as usize);

        let support_capacity_n: f64 = 25.0;
        let effective_layer_height_um: f64 = 50.0;

        for i in 0..n {
            let lf = i as f64;
            let x = i as f64;

            // Bottom-layer peel spike (first 6 layers ~ 3x steady-state),
            // then steady-state with a slow drift and a high-frequency wobble.
            let bottom_factor = if i < 6 { 3.0 - (i as f64 * 0.3) } else { 1.0 };
            let peel = bottom_factor * (12.34 + 3.5 * (lf * 0.021).cos());
            let suction = 0.78 + 0.4 * (lf * 0.05).sin();
            let total = peel + suction;
            let cure = 142.3 + 8.0 * (lf * 0.013).sin();
            let safety = support_capacity_n / total.max(0.01);
            let vat = 27.4 + 0.8 * (lf * 0.003).sin() + 0.05 * (lf * 0.001);

            peel_force.push([x, peel]);
            suction_force.push([x, suction]);
            total_force.push([x, total]);
            cure_depth.push([x, cure]);
            safety_factor.push([x, safety]);
            vat_temperature.push([x, vat]);
        }

        Self {
            peel_force,
            suction_force,
            total_force,
            support_capacity_n,
            cure_depth,
            effective_layer_height_um,
            safety_factor,
            vat_temperature,
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "resinsim-viz v2 spike".into(),
                resolution: (1400u32, 900u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(Cursor::default())
        .insert_resource(Series::synthesize(TOTAL_LAYERS))
        .add_systems(Startup, setup_camera)
        .add_systems(Update, auto_advance_cursor)
        .add_systems(EguiPrimaryContextPass, draw_dashboard)
        .run();
}

/// Egui auto-attaches its render output to the first camera in the
/// world; without one the screen stays grey and no UI appears.
/// 2D camera is enough for the spike (no Bevy meshes).
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Applies the DESIGN.md dark-default theme: cool blue-grey surfaces,
/// faint grid lines, no pure-black or pure-white. Production will move
/// these into typed tokens; the spike inlines them. Called once from
/// the first egui pass because the egui context isn't constructed
/// until after Startup runs.
fn apply_dark_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    let surface_base = egui::Color32::from_rgb(20, 24, 31);
    let surface_low = egui::Color32::from_rgb(14, 17, 22);
    let surface_high = egui::Color32::from_rgb(28, 33, 41);
    let ink = egui::Color32::from_rgb(220, 225, 233);
    let ink_muted = egui::Color32::from_rgb(140, 150, 162);
    let grid_line = egui::Color32::from_rgb(40, 46, 56);

    v.window_fill = surface_base;
    v.panel_fill = surface_base;
    v.extreme_bg_color = surface_low;
    v.faint_bg_color = surface_high;

    v.widgets.noninteractive.bg_fill = surface_base;
    v.widgets.inactive.bg_fill = surface_high;
    v.widgets.hovered.bg_fill = surface_high;
    v.widgets.active.bg_fill = surface_high;

    v.widgets.noninteractive.fg_stroke.color = ink_muted;
    v.widgets.inactive.fg_stroke.color = ink;
    v.widgets.hovered.fg_stroke.color = ink;
    v.widgets.active.fg_stroke.color = SERIES_GREEN;

    v.widgets.noninteractive.bg_stroke.color = grid_line;
    v.widgets.inactive.bg_stroke.color = grid_line;

    ctx.set_visuals(v);
}

fn auto_advance_cursor(time: Res<Time>, mut cursor: ResMut<Cursor>) {
    if !cursor.auto_advance {
        return;
    }
    let now = time.elapsed().as_secs_f32();
    if now - cursor.last_tick_secs >= TICK_INTERVAL_SECS {
        cursor.last_tick_secs = now;
        cursor.layer = (cursor.layer + 1) % TOTAL_LAYERS;
    }
}

fn draw_dashboard(
    mut contexts: EguiContexts,
    mut cursor: ResMut<Cursor>,
    series: Res<Series>,
    mut themed: Local<bool>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if !*themed {
        apply_dark_theme(ctx);
        *themed = true;
    }

    egui::TopBottomPanel::top("summary")
        .resizable(false)
        .default_height(48.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("[ run-A ]").monospace());
                ui.separator();
                ui.label(egui::RichText::new("total 4h 12m 33s").monospace());
                ui.separator();
                ui.label(egui::RichText::new("max-force layer 3247").monospace());
                ui.separator();
                ui.label(egui::RichText::new("min-safety layer 0412").monospace());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("v2 spike").weak());
                });
            });
        });

    egui::TopBottomPanel::bottom("scrubber")
        .resizable(false)
        .default_height(64.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Layer");
                let total_minus_one = TOTAL_LAYERS.saturating_sub(1);
                let mut layer_i = cursor.layer as i64;
                let resp = ui.add(
                    egui::Slider::new(&mut layer_i, 0..=(total_minus_one as i64))
                        .show_value(false),
                );
                if resp.changed() {
                    cursor.layer = layer_i.max(0) as u32;
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{:>04} of {:>04}",
                        cursor.layer, total_minus_one
                    ))
                    .monospace(),
                );
                ui.separator();
                ui.checkbox(&mut cursor.auto_advance, "auto-advance");
            });
        });

    egui::SidePanel::left("failures")
        .resizable(true)
        .default_width(220.0)
        .min_width(160.0)
        .show(ctx, |ui| {
            ui.heading("Failures");
            ui.separator();
            ui.label(
                egui::RichText::new("(0 failures, 0 warnings)")
                    .monospace()
                    .weak(),
            );
        });

    egui::SidePanel::right("readout")
        .resizable(true)
        .default_width(280.0)
        .min_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Layer stats");
            ui.separator();
            stats_table(ui, cursor.layer);
        });

    egui::CentralPanel::default().show(ctx, |ui| {
        let half = ui.available_width() * 0.5;
        egui::SidePanel::left("col1")
            .resizable(true)
            .default_width(half)
            .show_inside(ui, |ui| {
                let half_h = ui.available_height() * 0.5;
                egui::TopBottomPanel::top("col1_top")
                    .resizable(true)
                    .default_height(half_h)
                    .show_inside(ui, |ui| {
                        forces_pane(ui, &series, cursor.layer);
                    });
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    cure_depth_pane(ui, &series, cursor.layer);
                });
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let half_h = ui.available_height() * 0.5;
            egui::TopBottomPanel::top("col2_top")
                .resizable(true)
                .default_height(half_h)
                .show_inside(ui, |ui| {
                    safety_factor_pane(ui, &series, cursor.layer);
                });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                vat_temperature_pane(ui, &series, cursor.layer);
            });
        });
    });
}

fn cursor_vline(layer: u32) -> VLine {
    VLine::new("cursor", layer as f64)
        .color(CURSOR_INK)
        .width(1.0_f32)
}

/// Shared id used by every pane's `link_axis` + `link_cursor`. Panning,
/// zooming, or hovering any pane mirrors to all the others.
fn layer_link() -> egui::Id {
    egui::Id::new("v2-spike-layer-axis")
}

fn forces_pane(ui: &mut egui::Ui, series: &Series, cursor_layer: u32) {
    ui.heading("Forces");
    ui.separator();
    Plot::new("forces_plot")
        .x_axis_label("Layer")
        .y_axis_label("N")
        .legend(Legend::default())
        .link_axis(layer_link(), [true, false])
        .link_cursor(layer_link(), [true, false])
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("peel", PlotPoints::from(series.peel_force.clone()))
                    .color(SERIES_GREEN),
            );
            plot_ui.line(
                Line::new("suction", PlotPoints::from(series.suction_force.clone()))
                    .color(SERIES_CYAN),
            );
            plot_ui.line(
                Line::new("total", PlotPoints::from(series.total_force.clone()))
                    .color(SERIES_YELLOW),
            );
            plot_ui.hline(
                HLine::new("support_capacity", series.support_capacity_n)
                    .color(THRESHOLD_AMBER)
                    .width(1.0_f32),
            );
            plot_ui.vline(cursor_vline(cursor_layer));
        });
}

fn cure_depth_pane(ui: &mut egui::Ui, series: &Series, cursor_layer: u32) {
    ui.heading("Cure depth");
    ui.separator();
    Plot::new("cure_depth_plot")
        .x_axis_label("Layer")
        .y_axis_label("µm")
        .legend(Legend::default())
        .link_axis(layer_link(), [true, false])
        .link_cursor(layer_link(), [true, false])
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("cure_depth", PlotPoints::from(series.cure_depth.clone()))
                    .color(SERIES_GREEN),
            );
            plot_ui.hline(
                HLine::new("layer_height", series.effective_layer_height_um)
                    .color(THRESHOLD_AMBER)
                    .width(1.0_f32),
            );
            plot_ui.vline(cursor_vline(cursor_layer));
        });
}

fn safety_factor_pane(ui: &mut egui::Ui, series: &Series, cursor_layer: u32) {
    ui.heading("Safety factor");
    ui.separator();
    Plot::new("safety_plot")
        .x_axis_label("Layer")
        .y_axis_label("ratio")
        .legend(Legend::default())
        .link_axis(layer_link(), [true, false])
        .link_cursor(layer_link(), [true, false])
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("safety_factor", PlotPoints::from(series.safety_factor.clone()))
                    .color(SERIES_GREEN),
            );
            plot_ui.hline(
                HLine::new("threshold", 1.0_f64)
                    .color(THRESHOLD_RED)
                    .width(1.0_f32),
            );
            plot_ui.vline(cursor_vline(cursor_layer));
        });
}

fn vat_temperature_pane(ui: &mut egui::Ui, series: &Series, cursor_layer: u32) {
    ui.heading("Vat temperature");
    ui.separator();
    Plot::new("vat_temp_plot")
        .x_axis_label("Layer")
        .y_axis_label("°C")
        .legend(Legend::default())
        .link_axis(layer_link(), [true, false])
        .link_cursor(layer_link(), [true, false])
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("vat_temp", PlotPoints::from(series.vat_temperature.clone()))
                    .color(SERIES_ORANGE),
            );
            plot_ui.vline(cursor_vline(cursor_layer));
        });
}

/// PRIMARY VALIDATION TARGET (2): mono-tabular stats column.
///
/// Synthesised values are designed to vary digit counts across rows
/// and across cursor moves so any column-alignment regression surfaces
/// visibly in the running spike.
fn stats_table(ui: &mut egui::Ui, layer: u32) {
    let lf = layer as f32;
    let cure_depth_um = 142.3 + 8.0 * (lf * 0.013).sin();
    let worst_cure_depth_um = cure_depth_um - 6.5;
    let peel_force_n = 12.34 + 3.5 * (lf * 0.021).cos();
    let suction_force_n = 0.78 + 0.4 * (lf * 0.05).sin();
    let total_force_n = peel_force_n + suction_force_n;
    let support_capacity_n = 25.0_f32;
    let safety_factor = support_capacity_n / total_force_n.max(0.01);
    let area_mm2 = 320.5 + 90.0 * (lf * 0.007).sin();
    let area_delta_mm2 = 12.5 * (lf * 0.011).cos();
    let vat_temp_c = 27.4 + 0.8 * (lf * 0.003).sin();
    let viscosity_mpa_s = 215.0 + 18.0 * (lf * 0.006).cos();
    let z_deflection_um = 4.5 + 1.5 * (lf * 0.017).sin();
    let effective_layer_um = 50.0_f32;

    egui::Grid::new("stats_grid")
        .num_columns(3)
        .striped(true)
        .min_col_width(0.0)
        .show(ui, |ui| {
            row(ui, "index", &format!("{layer:>04}"), "");
            row(ui, "cure_depth", &format!("{cure_depth_um:>8.1}"), "µm");
            row(ui, "worst_cure_depth", &format!("{worst_cure_depth_um:>8.1}"), "µm");
            row(ui, "effective_layer", &format!("{effective_layer_um:>8.1}"), "µm");
            row(ui, "peel_force", &format!("{peel_force_n:>8.2}"), "N");
            row(ui, "suction_force", &format!("{suction_force_n:>8.2}"), "N");
            row(ui, "total_force", &format!("{total_force_n:>8.2}"), "N");
            row(ui, "support_capacity", &format!("{support_capacity_n:>8.2}"), "N");
            row(ui, "safety_factor", &format!("{safety_factor:>8.2}"), "");
            row(ui, "cross_section_area", &format!("{area_mm2:>8.1}"), "mm²");
            row(ui, "area_delta", &format!("{area_delta_mm2:>+8.2}"), "mm²");
            row(ui, "vat_temperature", &format!("{vat_temp_c:>8.1}"), "°C");
            row(ui, "viscosity", &format!("{viscosity_mpa_s:>8.1}"), "mPa·s");
            row(ui, "z_deflection", &format!("{z_deflection_um:>8.1}"), "µm");
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: &str, unit: &str) {
    ui.label(label);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(egui::RichText::new(value).monospace());
    });
    ui.label(egui::RichText::new(unit).weak().monospace());
    ui.end_row();
}
