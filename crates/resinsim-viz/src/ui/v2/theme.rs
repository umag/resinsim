//! v2 design-token block. Cool blue-grey tinted neutrals (low chroma
//! toward hue 240) per `DESIGN.md` §2; categorical Grafana-classic
//! palette for line series; threshold red/amber reserved exclusively
//! for fail/warn boundaries.
//!
//! These constants live in one place so a future `$impeccable document`
//! pass can lift them straight into a `DESIGN.md` token table without
//! grepping the pane bodies.

use bevy_egui::egui;

// ---------------------------------------------------------------------
// Surface scale (cool blue-grey, hue ≈ 240, low chroma).
// Resolved values for the OKLCH placeholders in DESIGN.md:
//   surface base dark   oklch(0.20 0.012 240)  → 20, 24, 31
//   surface low         oklch(0.15 0.010 240)  → 14, 17, 22
//   surface high        oklch(0.26 0.014 240)  → 28, 33, 41
//   ink                 oklch(0.92 0.010 240)  → 220, 225, 233
//   ink muted           oklch(0.64 0.008 240)  → 140, 150, 162
//   grid line           oklch(0.28 0.012 240)  → 40, 46, 56
// ---------------------------------------------------------------------

pub const SURFACE_BASE: egui::Color32 = egui::Color32::from_rgb(20, 24, 31);
pub const SURFACE_LOW: egui::Color32 = egui::Color32::from_rgb(14, 17, 22);
pub const SURFACE_HIGH: egui::Color32 = egui::Color32::from_rgb(28, 33, 41);
pub const INK: egui::Color32 = egui::Color32::from_rgb(220, 225, 233);
pub const INK_MUTED: egui::Color32 = egui::Color32::from_rgb(140, 150, 162);
pub const GRID_LINE: egui::Color32 = egui::Color32::from_rgb(40, 46, 56);

// ---------------------------------------------------------------------
// Grafana-classic categorical palette. For line series only — never
// used as chrome, surface, or emphasis. Two metrics on the same plot
// get different colours from this list; two runs of the same metric
// share a colour.
// ---------------------------------------------------------------------

pub const SERIES_GREEN: egui::Color32 = egui::Color32::from_rgb(126, 178, 109);
pub const SERIES_YELLOW: egui::Color32 = egui::Color32::from_rgb(234, 184, 57);
pub const SERIES_CYAN: egui::Color32 = egui::Color32::from_rgb(110, 208, 224);
pub const SERIES_ORANGE: egui::Color32 = egui::Color32::from_rgb(239, 132, 60);
pub const SERIES_PURPLE: egui::Color32 = egui::Color32::from_rgb(180, 132, 220);
pub const SERIES_BLUE: egui::Color32 = egui::Color32::from_rgb(118, 158, 230);

// ---------------------------------------------------------------------
// Threshold colours. Reserved exclusively for values that have crossed
// (THRESHOLD_RED) or are approaching (THRESHOLD_AMBER) a defined fail
// boundary. Forbidden as emphasis, brand, or delete-button colour.
// ---------------------------------------------------------------------

pub const THRESHOLD_RED: egui::Color32 = egui::Color32::from_rgb(190, 90, 90);
pub const THRESHOLD_AMBER: egui::Color32 = egui::Color32::from_rgb(190, 134, 60);

// ---------------------------------------------------------------------
// Cursor VLine ink. Light, neutral, never categorical or threshold.
// ---------------------------------------------------------------------

pub const CURSOR_INK: egui::Color32 = egui::Color32::from_rgb(180, 188, 200);

/// Apply the v2 dark theme to an egui context. Idempotent: safe to
/// call every frame, but the dashboard system uses a `Local<bool>`
/// guard so it only runs once.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    v.window_fill = SURFACE_BASE;
    v.panel_fill = SURFACE_BASE;
    v.extreme_bg_color = SURFACE_LOW;
    v.faint_bg_color = SURFACE_HIGH;

    v.widgets.noninteractive.bg_fill = SURFACE_BASE;
    v.widgets.inactive.bg_fill = SURFACE_HIGH;
    v.widgets.hovered.bg_fill = SURFACE_HIGH;
    v.widgets.active.bg_fill = SURFACE_HIGH;

    v.widgets.noninteractive.fg_stroke.color = INK_MUTED;
    v.widgets.inactive.fg_stroke.color = INK;
    v.widgets.hovered.fg_stroke.color = INK;
    v.widgets.active.fg_stroke.color = SERIES_GREEN;

    v.widgets.noninteractive.bg_stroke.color = GRID_LINE;
    v.widgets.inactive.bg_stroke.color = GRID_LINE;

    ctx.set_visuals(v);
}
