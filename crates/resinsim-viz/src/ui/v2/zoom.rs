//! Trackpad-pinch → egui_plot zoom bridge.
//!
//! `bevy_egui` 0.39 forwards mouse wheels but does not forward
//! Bevy 0.18's native `PinchGesture` events from the macOS trackpad.
//! `draw_v2_dashboard` reads them via `MessageReader<PinchGesture>`
//! (Bevy 0.18 renamed the trait from `Event` to `Message`),
//! sums the per-frame delta, and threads it through `PaneCtx` to
//! every pane. The pane that's hovered consumes the delta inside
//! its plot closure via [`apply_pinch_zoom`]; X-axis bounds propagate
//! across every pane in the `link_axis` group, Y-axis bounds stay
//! local to the hovered pane.
//!
//! Per-axis policy is locked at:
//!   - X shared (group "v2-shared-cursor"; pinch on any pane narrows
//!     the time window for all of them).
//!   - Y per-pane (units differ across panes; sharing Y zoom would
//!     be incoherent).
//!
//! Reference: `docs/patterns/mac-trackpad-panorbit-config.md`. The
//! 3D panorbit camera takes the same gesture stream via
//! `bevy_panorbit_camera`'s built-in handler; this module is the
//! egui_plot equivalent for the 2D dashboard.

use egui_plot::{PlotBounds, PlotUi};

/// Pinch sensitivity: pinch delta multiplied by this gives the
/// per-frame fractional zoom-in. Tuned for the macOS trackpad's
/// per-frame delta magnitudes (typically 0.01–0.05 per frame at
/// 60 Hz). Higher = faster zoom.
const PINCH_SENSITIVITY: f32 = 1.0;

/// Convert a per-frame pinch delta into a multiplicative zoom
/// factor for plot bounds.
///
/// Positive `delta` (fingers spreading apart) zooms IN by shrinking
/// the visible range — factor < 1.0. Negative `delta` (fingers
/// pinching together) zooms OUT — factor > 1.0. The factor is
/// clamped to [0.1, 10.0] so a single frame's gesture can't
/// collapse the plot to a point or blow it up to absurd ranges.
pub fn pinch_delta_to_zoom_factor(delta: f32, sensitivity: f32) -> f32 {
    let raw = 1.0 - delta * sensitivity;
    raw.clamp(0.1, 10.0)
}

/// Scale a 1-D range `[min, max]` around an `anchor` by `factor`.
/// Used by `apply_pinch_zoom` for both axes.
///
/// The anchor stays fixed; the surrounding range expands or
/// contracts proportionally. Anchoring on the cursor coordinate is
/// the standard direct-manipulation idiom — the point under the
/// fingers stays under the fingers as the surrounding range
/// changes.
pub fn zoom_around(min: f64, max: f64, anchor: f64, factor: f64) -> (f64, f64) {
    let new_min = anchor + (min - anchor) * factor;
    let new_max = anchor + (max - anchor) * factor;
    (new_min, new_max)
}

/// Apply a pinch-zoom to the plot at `plot_ui`. Called from inside
/// each pane's `Plot::show` closure, but only when the closure is
/// for the hovered pane (so exactly one pane consumes the gesture
/// per frame).
///
/// Both axes scale around the pointer's plot-coordinate. X bounds
/// propagate across the link group; Y bounds remain local.
///
/// No-op when `delta` is zero, when the pointer isn't over the
/// plot (no anchor available), or when the resulting bounds would
/// be degenerate.
pub fn apply_pinch_zoom(plot_ui: &mut PlotUi<'_>, delta: f32) {
    if delta.abs() < 1e-6 {
        return;
    }
    let Some(anchor) = plot_ui.pointer_coordinate() else {
        return;
    };
    let factor = f64::from(pinch_delta_to_zoom_factor(delta, PINCH_SENSITIVITY));
    let bounds = plot_ui.plot_bounds();
    let new_x = zoom_around(bounds.min()[0], bounds.max()[0], anchor.x, factor);
    let new_y = zoom_around(bounds.min()[1], bounds.max()[1], anchor.y, factor);
    if (new_x.1 - new_x.0).abs() < f64::EPSILON || (new_y.1 - new_y.0).abs() < f64::EPSILON {
        return;
    }
    plot_ui.set_plot_bounds(PlotBounds::from_min_max(
        [new_x.0, new_y.0],
        [new_x.1, new_y.1],
    ));
}

/// Percentile-clamped Y-axis bounds. Use this in preference to
/// [`data_y_bounds`] when the series can produce extreme outliers
/// that would otherwise collapse the default view (e.g. the
/// elegoo-grey 30 µm lilith run has finite `safety_factor` values
/// crossing 1 e6 on near-zero-force layers; min/max would render
/// the steady-state 1–20 band as a flat line at the bottom).
///
/// `lo_pct` / `hi_pct` are clamped to `[0.0, 1.0]`; pass `0.0`
/// for "use the actual minimum" and `1.0` for "use the actual
/// maximum". Typical values: `(0.02, 0.98)` drops 2 % from each
/// tail — enough to absorb a handful of outliers without losing
/// real boundary information.
///
/// Returns `None` when every input is non-finite.
pub fn percentile_bounds(
    values: &[f64],
    lo_pct: f64,
    hi_pct: f64,
    padding_frac: f64,
) -> Option<(f64, f64)> {
    let mut finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = finite.len() as f64 - 1.0;
    let lo_idx = (n * lo_pct.clamp(0.0, 1.0)).round() as usize;
    let hi_idx = (n * hi_pct.clamp(0.0, 1.0)).round() as usize;
    let max_idx = finite.len() - 1;
    let lo = finite[lo_idx.min(max_idx)];
    let hi = finite[hi_idx.min(max_idx)];
    let range = (hi - lo).max(1e-9);
    let pad = range * padding_frac.max(0.0);
    Some((lo - pad, hi + pad))
}

/// Compute initial Y-axis bounds that fit the data range, with
/// `padding_frac` headroom on each side (e.g. 0.05 = 5%). Returns
/// `None` only when every input is non-finite. Pure helper for
/// the v2 panes' `default_y_bounds` so the steady-state signal
/// reads at default zoom even when a series spikes at the
/// bottom-layer band — the user can still pinch out to see
/// outliers; we just don't let outliers compress the default view.
///
/// Tight data-driven bounds also keep threshold HLines well above
/// the visible band when they're set in real engineering units
/// (e.g. `support_capacity` ~100 N) while the data sits one order
/// of magnitude below (~10 N). The user pinches out to see the
/// threshold; the default view shows variation.
pub fn data_y_bounds(values: &[f64], padding_frac: f64) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in values {
        if !v.is_finite() {
            continue;
        }
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    let range = (hi - lo).max(1e-9);
    let pad = range * padding_frac.max(0.0);
    Some((lo - pad, hi + pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_factor_zero_delta_is_identity() {
        assert!((pinch_delta_to_zoom_factor(0.0, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pinch_factor_positive_zooms_in() {
        let factor = pinch_delta_to_zoom_factor(0.1, 1.0);
        assert!(factor < 1.0 && factor > 0.0);
    }

    #[test]
    fn pinch_factor_negative_zooms_out() {
        let factor = pinch_delta_to_zoom_factor(-0.1, 1.0);
        assert!(factor > 1.0);
    }

    #[test]
    fn pinch_factor_clamps_extreme_zoom_in() {
        // Hugely positive delta would otherwise drive factor below
        // zero; clamp catches it.
        let factor = pinch_delta_to_zoom_factor(100.0, 1.0);
        assert!(factor >= 0.1);
    }

    #[test]
    fn pinch_factor_clamps_extreme_zoom_out() {
        let factor = pinch_delta_to_zoom_factor(-100.0, 1.0);
        assert!(factor <= 10.0);
    }

    #[test]
    fn pinch_factor_respects_sensitivity() {
        // Higher sensitivity = bigger effect per delta unit.
        let low = pinch_delta_to_zoom_factor(0.05, 0.5);
        let high = pinch_delta_to_zoom_factor(0.05, 2.0);
        // Both zoom in (factor < 1), but high should be smaller.
        assert!(high < low);
    }

    #[test]
    fn zoom_around_preserves_anchor() {
        // The anchor point itself should map to itself after scaling.
        let (a, b) = zoom_around(0.0, 100.0, 50.0, 0.5);
        assert!((a - 25.0).abs() < 1e-9);
        assert!((b - 75.0).abs() < 1e-9);
        // Anchor itself: (50 - 50) * 0.5 + 50 = 50. Check via
        // mid-range proxy.
        assert!((50.0 - (a + b) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_zoom_in_shrinks_range() {
        let (a, b) = zoom_around(-10.0, 10.0, 0.0, 0.5);
        assert!((a - -5.0).abs() < 1e-9);
        assert!((b - 5.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_zoom_out_grows_range() {
        let (a, b) = zoom_around(-10.0, 10.0, 0.0, 2.0);
        assert!((a - -20.0).abs() < 1e-9);
        assert!((b - 20.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_off_centre_anchor() {
        // Anchor near the right edge — left side moves more than right.
        let (a, b) = zoom_around(0.0, 100.0, 90.0, 0.5);
        // a = 90 + (0 - 90) * 0.5 = 45
        // b = 90 + (100 - 90) * 0.5 = 95
        assert!((a - 45.0).abs() < 1e-9);
        assert!((b - 95.0).abs() < 1e-9);
    }

    // ---- percentile_bounds ----

    #[test]
    fn percentile_bounds_clamps_steady_state_against_outlier() {
        // Steady state at 5.0 (200 samples) plus a single 1e6 spike.
        let mut xs = vec![5.0; 200];
        xs.push(1_000_000.0);
        let (lo, hi) = percentile_bounds(&xs, 0.0, 0.98, 0.0).expect("non-empty after filter");
        // p98 of 201 sorted values: idx = round(200 * 0.98) = 196.
        // Index 196 of [5.0×200, 1e6] is still 5.0 (the spike sits
        // at index 200). So hi = 5.0, no spike pulled in.
        assert!((lo - 5.0).abs() < 1e-9);
        assert!((hi - 5.0).abs() < 1e-9);
    }

    #[test]
    fn percentile_bounds_keeps_full_range_at_0_1() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let (lo, hi) = percentile_bounds(&xs, 0.0, 1.0, 0.0).expect("non-empty");
        assert!((lo - 1.0).abs() < 1e-9);
        assert!((hi - 5.0).abs() < 1e-9);
    }

    #[test]
    fn percentile_bounds_drops_both_tails() {
        // 100 values 1..=100. With n = 99 (size - 1):
        // lo_idx = round(99 * 0.10) = round(9.9) = 10 → finite[10] = 11.0
        // hi_idx = round(99 * 0.90) = round(89.1) = 89 → finite[89] = 90.0
        let xs: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let (lo, hi) = percentile_bounds(&xs, 0.1, 0.9, 0.0).expect("non-empty");
        assert!((lo - 11.0).abs() < 1e-6, "lo = {lo}");
        assert!((hi - 90.0).abs() < 1e-6, "hi = {hi}");
    }

    #[test]
    fn percentile_bounds_filters_non_finite() {
        let xs = [1.0, f64::INFINITY, 5.0, f64::NAN];
        let (lo, hi) = percentile_bounds(&xs, 0.0, 1.0, 0.0).expect("filtered down to {1.0, 5.0}");
        assert!((lo - 1.0).abs() < 1e-9);
        assert!((hi - 5.0).abs() < 1e-9);
    }

    #[test]
    fn percentile_bounds_empty_and_all_non_finite_return_none() {
        assert_eq!(percentile_bounds(&[], 0.02, 0.98, 0.05), None);
        assert_eq!(
            percentile_bounds(&[f64::NAN, f64::INFINITY], 0.02, 0.98, 0.05),
            None
        );
    }

    #[test]
    fn percentile_bounds_applies_padding() {
        let xs = [1.0, 5.0];
        let (lo, hi) = percentile_bounds(&xs, 0.0, 1.0, 0.1).expect("non-empty");
        // range = 4, pad = 0.4 → (0.6, 5.4)
        assert!((lo - 0.6).abs() < 1e-9);
        assert!((hi - 5.4).abs() < 1e-9);
    }

    // ---- data_y_bounds ----

    #[test]
    fn data_y_bounds_basic_range_with_padding() {
        let (lo, hi) = data_y_bounds(&[1.0, 5.0, 3.0], 0.1).expect("non-empty");
        // range = 4, pad = 0.4 → bounds = (0.6, 5.4)
        assert!((lo - 0.6).abs() < 1e-9);
        assert!((hi - 5.4).abs() < 1e-9);
    }

    #[test]
    fn data_y_bounds_handles_constant_values() {
        let (lo, hi) = data_y_bounds(&[5.0, 5.0, 5.0], 0.1).expect("non-empty");
        // Range is 0, but we floor it at 1e-9 → pad ~0. Bounds essentially (5, 5).
        // Caller is responsible for further padding if they want a visible plot.
        assert!((hi - lo).abs() < 1.0);
    }

    #[test]
    fn data_y_bounds_filters_non_finite() {
        let xs = [1.0, f64::NAN, 5.0, f64::INFINITY, 3.0];
        let (lo, hi) = data_y_bounds(&xs, 0.0).expect("finite values exist");
        // 1.0 — 5.0, no padding
        assert!((lo - 1.0).abs() < 1e-9);
        assert!((hi - 5.0).abs() < 1e-9);
    }

    #[test]
    fn data_y_bounds_all_non_finite_returns_none() {
        assert_eq!(data_y_bounds(&[f64::NAN, f64::INFINITY], 0.05), None);
        assert_eq!(data_y_bounds(&[], 0.05), None);
    }

    #[test]
    fn data_y_bounds_negative_padding_clamps_to_zero() {
        // Don't let a bad caller shrink the range below the data.
        let (lo, hi) = data_y_bounds(&[1.0, 5.0], -0.5).expect("non-empty");
        assert!((lo - 1.0).abs() < 1e-9);
        assert!((hi - 5.0).abs() < 1e-9);
    }
}
