//! Layer scrubber — full-width bottom strip showing the cursor
//! position over the layer range. Click anywhere to jump; drag to
//! follow the cursor continuously.
//!
//! Slice D in `spec/viz-v2-design-brief.md` §5. Failure ticks
//! (red/amber inline marks at FailureEvent layers) belong to slice
//! B and land alongside the failures rail; for now the scrubber
//! renders only the cursor line + the axis tick labels.
//!
//! Keyboard parity with the scrubber (↑/↓, Shift+↑/↓, Home/End,
//! PgUp/PgDn per brief §7) lives in `crate::handle_layer_keys` —
//! adjusting that system in main.rs keeps the keyboard handling
//! consistent across the v1 and v2 surfaces.

use bevy_egui::egui;
use resinsim_core::entities::{FailureEvent, Severity};

use super::theme;

/// Default height of the scrubber strip per brief §5.
pub const SCRUBBER_HEIGHT_PX: f32 = 64.0;

/// Render the scrubber and return `Some(new_layer)` if the user's
/// click or drag should move the cursor. The caller writes the
/// returned index into `CurrentLayer.index`.
///
/// `current` is the active layer (drawn as a cursor VLine);
/// `max` is the last valid index (`layers.len() - 1`). When `max`
/// is 0 the scrubber renders the chrome but ignores input — there
/// is no range to scrub.
///
/// `failures` paints inline severity ticks across the axis at each
/// failure's layer position. The cursor line is painted after the
/// ticks so the active cursor is always visible even when it sits
/// on a failure layer.
pub fn render(
    ui: &mut egui::Ui,
    current: u32,
    max: u32,
    failures: &[FailureEvent],
) -> Option<u32> {
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(avail.x, avail.y),
        egui::Sense::click_and_drag(),
    );

    paint_chrome(ui, rect);

    if max == 0 {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "(no layers loaded)",
            egui::FontId::monospace(11.0),
            theme::INK_MUTED,
        );
        return None;
    }

    let axis_rect = axis_subrect(rect);
    paint_axis_ticks(ui, axis_rect, max);
    paint_failure_ticks(ui, axis_rect, failures, max);
    paint_cursor(ui, axis_rect, current, max);

    // Click anywhere on the strip (including the label band) jumps
    // the cursor; drag follows. Pointer coords are screen-space.
    if resp.clicked() || resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            return Some(screen_x_to_layer(pos.x, axis_rect.min.x, axis_rect.width(), max));
        }
    }

    None
}

/// Background + top/bottom edges. Subtle separation from the rest
/// of the dashboard via the same `GRID_LINE` colour used by cell
/// borders, so the scrubber reads as part of the same surface
/// family as the panes above it.
fn paint_chrome(ui: &mut egui::Ui, rect: egui::Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::SURFACE_BASE);
    painter.line_segment(
        [
            egui::pos2(rect.min.x, rect.min.y),
            egui::pos2(rect.max.x, rect.min.y),
        ],
        egui::Stroke::new(1.0_f32, theme::GRID_LINE),
    );
}

/// Vertical inset for the axis (cursor + ticks). Leaves a band at
/// the bottom for tick labels and 6px of breathing room at the
/// top.
fn axis_subrect(rect: egui::Rect) -> egui::Rect {
    let label_band = 14.0;
    egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 8.0, rect.min.y + 6.0),
        egui::pos2(rect.max.x - 8.0, rect.max.y - label_band),
    )
}

/// Cursor VLine + tick mark. The line spans the axis subrect's
/// height; a 2px width keeps it readable at retina density without
/// dominating the strip.
fn paint_cursor(ui: &mut egui::Ui, axis_rect: egui::Rect, current: u32, max: u32) {
    let x = layer_to_screen_x(current, axis_rect.min.x, axis_rect.width(), max);
    let painter = ui.painter_at(axis_rect);
    painter.line_segment(
        [
            egui::pos2(x, axis_rect.min.y),
            egui::pos2(x, axis_rect.max.y),
        ],
        egui::Stroke::new(2.0_f32, theme::CURSOR_INK),
    );
}

/// Paint inline severity ticks: 1px-wide vertical strokes at each
/// failure layer's screen-x position, coloured by severity.
/// Critical = red, Warning = amber, Info = muted ink (rendered
/// half-height so the muted dots don't crowd the cursor line).
///
/// Order is deliberate: muted Info first, then Warning, then
/// Critical, so the most severe ticks paint on top.
fn paint_failure_ticks(
    ui: &mut egui::Ui,
    axis_rect: egui::Rect,
    failures: &[FailureEvent],
    max: u32,
) {
    if failures.is_empty() {
        return;
    }
    let painter = ui.painter_at(axis_rect);
    for ordered in [Severity::Info, Severity::Warning, Severity::Critical] {
        for f in failures.iter().filter(|f| f.severity == ordered) {
            if f.layer > max {
                continue;
            }
            let x = layer_to_screen_x(f.layer, axis_rect.min.x, axis_rect.width(), max);
            let (color, half_height) = match ordered {
                Severity::Critical => (theme::THRESHOLD_RED, false),
                Severity::Warning => (theme::THRESHOLD_AMBER, false),
                Severity::Info => (theme::INK_MUTED, true),
            };
            let (top, bottom) = if half_height {
                (axis_rect.center().y, axis_rect.max.y)
            } else {
                (axis_rect.min.y, axis_rect.max.y)
            };
            painter.line_segment(
                [egui::pos2(x, top), egui::pos2(x, bottom)],
                egui::Stroke::new(1.0_f32, color),
            );
        }
    }
}

/// Tick marks + numeric labels at sensible intervals. Aims for
/// ~5–8 ticks across the visible range, snapping to round
/// multiples so the labels are easy to read.
fn paint_axis_ticks(ui: &mut egui::Ui, axis_rect: egui::Rect, max: u32) {
    let step = tick_step_for(max);
    let painter = ui.painter();

    let mut tick = 0_u32;
    while tick <= max {
        let x = layer_to_screen_x(tick, axis_rect.min.x, axis_rect.width(), max);
        // Tick mark (short, at the axis bottom).
        painter.line_segment(
            [
                egui::pos2(x, axis_rect.max.y - 4.0),
                egui::pos2(x, axis_rect.max.y),
            ],
            egui::Stroke::new(1.0_f32, theme::GRID_LINE),
        );
        // Label below the tick.
        painter.text(
            egui::pos2(x, axis_rect.max.y + 2.0),
            egui::Align2::CENTER_TOP,
            tick.to_string(),
            egui::FontId::monospace(10.0),
            theme::INK_MUTED,
        );
        tick = match tick.checked_add(step) {
            Some(next) => next,
            None => break,
        };
    }
}

/// Map a screen-space x coordinate to a layer index, clamped to
/// `[0, max]`. Pure helper for unit tests.
///
/// `origin_x` and `width` define the axis subrect; `x` is the
/// pointer's screen-space x. Returns `0` for degenerate inputs
/// (max=0 or width≤0) rather than panicking — the caller is
/// expected to skip the cursor update in those cases.
pub fn screen_x_to_layer(x: f32, origin_x: f32, width: f32, max: u32) -> u32 {
    if max == 0 || width <= 0.0 {
        return 0;
    }
    let frac = ((x - origin_x) / width).clamp(0.0, 1.0) as f64;
    (frac * max as f64).round() as u32
}

/// Inverse of `screen_x_to_layer`: layer index → screen-space x in
/// the axis subrect. Used for cursor + tick rendering.
pub fn layer_to_screen_x(layer: u32, origin_x: f32, width: f32, max: u32) -> f32 {
    if max == 0 || width <= 0.0 {
        return origin_x;
    }
    let frac = (layer as f64 / max as f64).clamp(0.0, 1.0) as f32;
    origin_x + frac * width
}

/// Compute (x, severity) pairs for every failure within the
/// scrubber range. Pure helper for unit tests; the rendering loop
/// calls `layer_to_screen_x` directly (different iteration order
/// for paint depth), but the math is identical so this exercises
/// the same code path.
#[cfg(test)]
pub fn failure_tick_positions(
    failures: &[FailureEvent],
    origin_x: f32,
    width: f32,
    max: u32,
) -> Vec<(f32, Severity)> {
    failures
        .iter()
        .filter(|f| f.layer <= max)
        .map(|f| {
            (
                layer_to_screen_x(f.layer, origin_x, width, max),
                f.severity,
            )
        })
        .collect()
}

/// Pick a tick step that yields ~5–10 labels across the range.
/// Snaps to round multiples (1, 5, 10, 50, 100, 500, 1000, …) so
/// the label values read naturally.
///
/// Pure helper for unit tests.
pub fn tick_step_for(max: u32) -> u32 {
    if max == 0 {
        return 1;
    }
    // Target ~8 ticks; the step is `max / 8`, rounded up to the next
    // nice number.
    let raw = (max as f64 / 8.0).max(1.0);
    let exp = raw.log10().floor() as i32;
    let scale = 10f64.powi(exp);
    let mantissa = raw / scale;
    let nice = if mantissa <= 1.0 {
        1.0
    } else if mantissa <= 2.0 {
        2.0
    } else if mantissa <= 5.0 {
        5.0
    } else {
        10.0
    };
    ((nice * scale).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use resinsim_core::entities::FailureType;

    fn mk_failure(layer: u32, severity: Severity) -> FailureEvent {
        FailureEvent {
            layer,
            failure_type: FailureType::SupportOverload,
            severity,
            message: String::new(),
        }
    }

    // ---- screen_x_to_layer ----

    #[test]
    fn screen_x_at_origin_is_zero() {
        assert_eq!(screen_x_to_layer(0.0, 0.0, 100.0, 4491), 0);
    }

    #[test]
    fn screen_x_at_right_edge_is_max() {
        assert_eq!(screen_x_to_layer(100.0, 0.0, 100.0, 4491), 4491);
    }

    #[test]
    fn screen_x_mid_range_is_mid_layer() {
        let layer = screen_x_to_layer(50.0, 0.0, 100.0, 4490);
        assert_eq!(layer, 2245);
    }

    #[test]
    fn screen_x_below_origin_clamps_to_zero() {
        assert_eq!(screen_x_to_layer(-50.0, 0.0, 100.0, 4491), 0);
    }

    #[test]
    fn screen_x_past_right_clamps_to_max() {
        assert_eq!(screen_x_to_layer(150.0, 0.0, 100.0, 4491), 4491);
    }

    #[test]
    fn screen_x_zero_max_returns_zero() {
        // Defensive: no range to scrub → no cursor movement.
        assert_eq!(screen_x_to_layer(50.0, 0.0, 100.0, 0), 0);
    }

    #[test]
    fn screen_x_zero_width_returns_zero() {
        assert_eq!(screen_x_to_layer(50.0, 0.0, 0.0, 4491), 0);
    }

    #[test]
    fn screen_x_offset_origin_respects_origin() {
        // Pointer at x=200 with origin=100, width=100 → frac=1 → max.
        assert_eq!(screen_x_to_layer(200.0, 100.0, 100.0, 4491), 4491);
        // Pointer at x=150 → mid.
        let mid = screen_x_to_layer(150.0, 100.0, 100.0, 4490);
        assert_eq!(mid, 2245);
    }

    // ---- layer_to_screen_x ----

    #[test]
    fn layer_at_zero_is_origin() {
        assert_eq!(layer_to_screen_x(0, 100.0, 200.0, 4491), 100.0);
    }

    #[test]
    fn layer_at_max_is_right_edge() {
        assert_eq!(layer_to_screen_x(4491, 100.0, 200.0, 4491), 300.0);
    }

    #[test]
    fn layer_round_trip_with_screen_x() {
        // Forward + reverse should agree at integer layer values
        // for reasonable axis widths.
        let max = 4491_u32;
        let origin = 0.0_f32;
        let width = 1000.0_f32;
        for layer in [0_u32, 100, 1000, 2245, 4490, 4491] {
            let x = layer_to_screen_x(layer, origin, width, max);
            let back = screen_x_to_layer(x, origin, width, max);
            assert_eq!(back, layer, "round-trip mismatch at layer {layer}");
        }
    }

    // ---- failure_tick_positions ----

    #[test]
    fn failure_ticks_empty_input_yields_empty() {
        let ticks = failure_tick_positions(&[], 0.0, 100.0, 4491);
        assert!(ticks.is_empty());
    }

    #[test]
    fn failure_ticks_map_layer_to_screen_x() {
        let xs = vec![
            mk_failure(0, Severity::Critical),
            mk_failure(2245, Severity::Warning),
            mk_failure(4490, Severity::Info),
        ];
        let ticks = failure_tick_positions(&xs, 0.0, 100.0, 4490);
        assert_eq!(ticks.len(), 3);
        assert!((ticks[0].0 - 0.0).abs() < 0.5);
        assert!((ticks[1].0 - 50.0).abs() < 0.5);
        assert!((ticks[2].0 - 100.0).abs() < 0.5);
        assert_eq!(ticks[0].1, Severity::Critical);
        assert_eq!(ticks[1].1, Severity::Warning);
        assert_eq!(ticks[2].1, Severity::Info);
    }

    #[test]
    fn failure_ticks_filter_out_of_range_layers() {
        // A failure at layer 9999 against a max of 4490 must be
        // dropped — it'd render off the right edge of the scrubber.
        let xs = vec![
            mk_failure(100, Severity::Critical),
            mk_failure(9999, Severity::Critical),
        ];
        let ticks = failure_tick_positions(&xs, 0.0, 100.0, 4490);
        assert_eq!(ticks.len(), 1);
    }

    #[test]
    fn failure_ticks_respect_axis_origin() {
        let xs = vec![mk_failure(0, Severity::Critical)];
        let ticks = failure_tick_positions(&xs, 200.0, 500.0, 4490);
        // Layer 0 → frac 0 → origin_x = 200.
        assert!((ticks[0].0 - 200.0).abs() < 0.5);
    }

    // ---- tick_step_for ----

    #[test]
    fn tick_step_zero_max_is_one() {
        assert_eq!(tick_step_for(0), 1);
    }

    #[test]
    fn tick_step_small_max() {
        // max = 10 → raw = 1.25, mantissa=1.25 → nice=2, scale=1 → step=2
        assert_eq!(tick_step_for(10), 2);
    }

    #[test]
    fn tick_step_lilith_scale() {
        // max = 4491 → raw ≈ 561, exp=2 (scale=100), mantissa≈5.61 → nice=10 → step=1000
        let step = tick_step_for(4491);
        // Should produce ~4-5 ticks across the range, which we can
        // verify by checking the step is in the expected "nice"
        // multiple set and in the right order of magnitude.
        assert!(step >= 500 && step <= 1000, "got {step}");
    }

    #[test]
    fn tick_step_returns_round_numbers() {
        // For a series of ranges, the chosen step should be 1, 2,
        // 5, or 10× a power of 10.
        let allowed_mantissas = [1u32, 2, 5];
        for max in [1, 5, 10, 50, 100, 500, 1000, 5000, 10_000] {
            let step = tick_step_for(max);
            let mut s = step;
            while s % 10 == 0 && s > 9 {
                s /= 10;
            }
            assert!(
                allowed_mantissas.contains(&s),
                "tick step {step} for max={max} reduces to mantissa {s}, expected 1/2/5"
            );
        }
    }
}
