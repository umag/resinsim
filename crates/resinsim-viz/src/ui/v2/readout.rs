//! Per-layer stat readout — right rail showing every `LayerResult`
//! field for the cursor layer in mono-tabular type.
//!
//! Slice C in `spec/viz-v2-design-brief.md` §5: a ~280px-wide
//! rail mounted via `egui::SidePanel::right` outside the pane grid.
//! Number formatting follows brief §8: forces 2 decimals + N,
//! depths 1 decimal + µm, temperature 1 decimal + °C, area 1
//! decimal (delta signed) + mm², layer index zero-padded. Non-
//! finite values surface as "∞" / "-∞" / "NaN" rather than
//! gracelessly crashing the format string (safety_factor =
//! `f32::INFINITY` on zero-force layers per
//! `safety-factor-zero-force.md`).
//!
//! Collapsible behaviour is deferred — the brief calls the rail
//! "collapsible" but the simplest implementation (`SidePanel`'s
//! built-in resizable drag edge) already lets the user shrink it.
//! A discrete collapse-to-icon toggle lands when the dashboard
//! ships a settings surface.

use bevy_egui::egui;
use resinsim_core::simulation::PrintSimulation;

use super::theme;

pub const READOUT_WIDTH_DEFAULT: f32 = 280.0;
pub const READOUT_WIDTH_MIN: f32 = 220.0;

/// Render the per-layer stat readout into the parent `ui`.
///
/// When `sim` is `None`, paints a muted "(no run loaded)" message
/// and returns; this is the `NoRun` analogue for the rail. When the
/// cursor layer is out of range (shouldn't happen with the existing
/// `CurrentLayer.max` clamp, but defensive), it clamps to the last
/// valid layer.
pub fn render(ui: &mut egui::Ui, sim: Option<&PrintSimulation>, cursor_layer: u32) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Layer stats")
            .strong()
            .color(theme::INK)
            .size(14.0),
    );
    ui.separator();

    let Some(sim) = sim else {
        ui.label(
            egui::RichText::new("(no run loaded)")
                .monospace()
                .small()
                .color(theme::INK_MUTED),
        );
        return;
    };

    let layers = sim.layers();
    if layers.is_empty() {
        ui.label(
            egui::RichText::new("(no layers in sim)")
                .monospace()
                .small()
                .color(theme::INK_MUTED),
        );
        return;
    }

    let max = (layers.len() as u32).saturating_sub(1);
    let idx = cursor_layer.min(max);
    let layer = &layers[idx as usize];

    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(format_layer_header(idx, max))
            .monospace()
            .color(theme::INK),
    );
    ui.add_space(4.0);

    egui::Grid::new("v2-readout-grid")
        .num_columns(3)
        .striped(true)
        .min_col_width(0.0)
        .show(ui, |ui| {
            row(ui, "cure_depth", &fmt_float(layer.cure_depth_um, 1), "µm");
            row(
                ui,
                "worst_cure",
                &fmt_float(layer.worst_cure_depth_um, 1),
                "µm",
            );
            row(
                ui,
                "layer_height",
                &fmt_float(layer.effective_layer_height_um, 1),
                "µm",
            );
            row(ui, "peel_force", &fmt_float(layer.peel_force_n, 2), "N");
            row(ui, "suction", &fmt_float(layer.suction_force_n, 2), "N");
            row(ui, "total_force", &fmt_float(layer.total_force_n, 2), "N");
            row(
                ui,
                "support_cap",
                &fmt_float(layer.support_capacity_n, 2),
                "N",
            );
            row(ui, "safety", &fmt_float(layer.safety_factor, 2), "");
            row(
                ui,
                "area",
                &fmt_float(layer.cross_section_area_mm2, 1),
                "mm²",
            );
            row(ui, "Δarea", &fmt_signed(layer.area_delta_mm2, 2), "mm²");
            row(ui, "vat_temp", &fmt_float(layer.vat_temperature_c, 1), "°C");
            row(
                ui,
                "viscosity",
                &fmt_float(layer.viscosity_mpa_s, 1),
                "mPa·s",
            );
            row(ui, "z_defl", &fmt_float(layer.z_deflection_um, 1), "µm");
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: &str, unit: &str) {
    ui.label(egui::RichText::new(label).small().color(theme::INK_MUTED));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(egui::RichText::new(value).monospace().color(theme::INK));
    });
    ui.label(egui::RichText::new(unit).weak().monospace());
    ui.end_row();
}

/// Format the rail's layer header: "Layer 0042 of 4491". Zero-pads
/// both numbers to the width of `max` so the digits line up across
/// cursor moves (the brief's tabular-mono rule applies to this
/// header just as much as the value column below it).
pub fn format_layer_header(cursor: u32, max: u32) -> String {
    // Width = digit count of `max` (the largest value either column
    // ever displays). Using `max + 1` here would over-pad when max
    // is e.g. 99999 — the digit jump from 99999 → 100000 widens the
    // pad, but no value in the column ever needs that extra digit.
    let width = max.to_string().len();
    format!(
        "Layer {cursor:0>width$} of {max:0>width$}",
        cursor = cursor,
        max = max,
        width = width
    )
}

/// Format a finite floating-point value with `decimals` precision.
/// Non-finite inputs surface as `∞` / `-∞` / `NaN` rather than the
/// Rust default `inf` / `-inf` / `NaN` so the column matches the
/// brief's tone (instrumentation-grade, not debug-print).
pub fn fmt_float<F: Into<f64>>(v: F, decimals: usize) -> String {
    let v: f64 = v.into();
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "∞".to_string()
        } else {
            "-∞".to_string()
        };
    }
    format!("{v:.decimals$}")
}

/// Like [`fmt_float`], but always carries an explicit sign (`+`/`-`)
/// for finite values. Used for `area_delta_mm2` where the sign is
/// the load-bearing piece of information (positive = layer widened,
/// negative = narrowed).
pub fn fmt_signed<F: Into<f64>>(v: F, decimals: usize) -> String {
    let v: f64 = v.into();
    if !v.is_finite() {
        return fmt_float(v, decimals);
    }
    format!("{v:+.decimals$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- format_layer_header ----

    #[test]
    fn header_zero_pads_to_max_width() {
        assert_eq!(format_layer_header(42, 4491), "Layer 0042 of 4491");
    }

    #[test]
    fn header_single_digit_unpadded_when_max_is_single_digit() {
        assert_eq!(format_layer_header(3, 7), "Layer 3 of 7");
    }

    #[test]
    fn header_zero_max_renders_zero_of_zero() {
        assert_eq!(format_layer_header(0, 0), "Layer 0 of 0");
    }

    #[test]
    fn header_max_at_99999_pads_to_5() {
        assert_eq!(format_layer_header(7, 99999), "Layer 00007 of 99999");
    }

    // ---- fmt_float ----

    #[test]
    fn fmt_float_zero_one_decimal() {
        assert_eq!(fmt_float(0.0_f32, 1), "0.0");
    }

    #[test]
    fn fmt_float_two_decimals_rounds_half_to_even_or_up() {
        // Rust's f64 → format uses round-to-nearest-even.
        // 12.345 rounds to 12.35 or 12.34 depending on f64 rep;
        // assert it's at least within one ULP of either.
        let s = fmt_float(12.345_f64, 2);
        assert!(s == "12.34" || s == "12.35", "got {s}");
    }

    #[test]
    fn fmt_float_carries_negative_sign() {
        assert_eq!(fmt_float(-7.2_f32, 1), "-7.2");
    }

    #[test]
    fn fmt_float_nan_surfaces_as_nan() {
        assert_eq!(fmt_float(f32::NAN, 2), "NaN");
    }

    #[test]
    fn fmt_float_positive_infinity_is_glyph() {
        assert_eq!(fmt_float(f32::INFINITY, 2), "∞");
    }

    #[test]
    fn fmt_float_negative_infinity_is_glyph_with_minus() {
        assert_eq!(fmt_float(f32::NEG_INFINITY, 2), "-∞");
    }

    #[test]
    fn fmt_float_accepts_f64() {
        assert_eq!(fmt_float(320.5_f64, 1), "320.5");
    }

    // ---- fmt_signed ----

    #[test]
    fn fmt_signed_positive_carries_plus() {
        assert_eq!(fmt_signed(5.2_f32, 2), "+5.20");
    }

    #[test]
    fn fmt_signed_negative_carries_minus() {
        assert_eq!(fmt_signed(-3.1_f32, 2), "-3.10");
    }

    #[test]
    fn fmt_signed_zero_renders_plus_zero() {
        // Tabular columns are easier to scan when zero has the
        // same width as a signed value either side of it.
        assert_eq!(fmt_signed(0.0_f32, 2), "+0.00");
    }

    #[test]
    fn fmt_signed_non_finite_passes_through() {
        assert_eq!(fmt_signed(f32::INFINITY, 2), "∞");
        assert_eq!(fmt_signed(f32::NAN, 2), "NaN");
    }
}
