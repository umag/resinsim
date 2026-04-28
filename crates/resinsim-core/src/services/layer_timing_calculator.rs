use crate::entities::{PrinterProfile, Recipe, ReleaseMechanism};

/// Domain service: per-layer cumulative time from a Recipe + PrinterProfile
/// release mechanism. Stateless — all inputs via parameters.
///
/// The calculator branches on `PrinterProfile::release_mechanism()`:
///
/// - `Linear` (build plate lifts, vat stationary — Athena II, Mars 4, classic
///   MSLA): uses separate lift and retract speeds.
///
///   ```text
///   t_lift    = lift_distance_mm / lift_speed_mm_min    × 60
///   t_retract = lift_distance_mm / retract_speed_mm_min × 60
///   t_layer   = exposure
///             + wait_before_cure
///             + t_lift
///             + wait_before_release
///             + t_retract
///             + wait_after_release
///   ```
///
/// - `Tilt` (vat hinges, plate stationary — Mars 5 Ultra): Recipe's
///   `lift_distance_mm` and `lift_speed_mm_min` are CTB metadata only. The
///   calculator uses `lift_cycle_sec` as the canonical lumped release
///   duration, skipping the separate lift/retract decomposition and the
///   `wait_before_release_sec` (the tilt motion is one atomic release).
///
///   ```text
///   t_layer = exposure + wait_before_cure + lift_cycle_sec + wait_after_release
///   ```
///
/// Tilt-angular geometry refinement (`tilt_angle_deg` × `tilt_rate_deg_s`) is
/// deferred to a follow-on issue; see ADR-0007.
///
/// # Exposure phases
///
/// The exposure time per layer follows a 3-phase model:
///
/// - `layer < bottom_layer_count` → `bottom_exposure_sec`
/// - `layer < bottom_layer_count + transition_layers` → linear interpolation
///   between bottom and normal
/// - else → `normal_exposure_sec`
pub struct LayerTimingCalculator;

impl LayerTimingCalculator {
    /// Cumulative time from layer 0 up to and including layer `i`, for
    /// `i` in `0..total_layers`. The i-th element is the sum of per-layer
    /// times for layers `0..=i`. Useful for feeding `ThermalCalculator`'s
    /// time-series stage A input.
    pub fn cumulative_times_sec(
        recipe: &Recipe,
        printer: &PrinterProfile,
        total_layers: u32,
    ) -> Vec<f32> {
        let mut out = Vec::with_capacity(total_layers as usize);
        let mut total = 0.0_f32;
        for i in 0..total_layers {
            total += Self::layer_time_sec(recipe, printer, i);
            out.push(total);
        }
        out
    }

    /// Time for a single layer `layer` (0-based), branching on release mechanism.
    pub fn layer_time_sec(recipe: &Recipe, printer: &PrinterProfile, layer: u32) -> f32 {
        let exposure = Self::exposure_at_layer(recipe, layer);
        match printer.release_mechanism() {
            ReleaseMechanism::Linear => {
                let t_lift = recipe.lift_distance_mm() / recipe.lift_speed_mm_min() * 60.0;
                let t_retract = recipe.lift_distance_mm() / recipe.retract_speed_mm_min() * 60.0;
                exposure
                    + recipe.wait_before_cure_sec()
                    + t_lift
                    + recipe.wait_before_release_sec()
                    + t_retract
                    + recipe.wait_after_release_sec()
            }
            ReleaseMechanism::Tilt => {
                // Tilt release is one atomic motion; wait_before_release_sec does not apply.
                exposure
                    + recipe.wait_before_cure_sec()
                    + recipe.lift_cycle_sec()
                    + recipe.wait_after_release_sec()
            }
        }
    }

    /// Exposure duration at a given 0-based layer index, per the 3-phase model
    /// (bottom / transition / normal). Interpolation in the transition window
    /// is linear between `bottom_exposure_sec` and `normal_exposure_sec`.
    pub fn exposure_at_layer(recipe: &Recipe, layer: u32) -> f32 {
        let bottom_n = recipe.bottom_layer_count();
        let transition_n = recipe.transition_layers();
        if layer < bottom_n {
            recipe.bottom_exposure_sec()
        } else if layer < bottom_n + transition_n {
            // Ramp from bottom at the last bottom-layer to normal at the first post-transition.
            // With transition_n layers, step = (normal - bottom) / (transition_n + 1).
            let step = (recipe.normal_exposure_sec() - recipe.bottom_exposure_sec())
                / (transition_n as f32 + 1.0);
            recipe.bottom_exposure_sec() + step * (layer - bottom_n + 1) as f32
        } else {
            recipe.normal_exposure_sec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{PrinterProfile, Recipe};

    fn linear_printer() -> PrinterProfile {
        PrinterProfile::generic_msla_4k() // ReleaseMechanism::Linear by factory default
    }

    fn tilt_printer() -> PrinterProfile {
        PrinterProfile::elegoo_mars5_ultra() // ReleaseMechanism::Tilt
    }

    // --- exposure_at_layer phases ---

    #[test]
    fn exposure_bottom_phase() {
        let r = Recipe::generic_standard(); // bottom_count=6, bottom=25s, normal=2.5s
        for i in 0..6 {
            assert_eq!(
                LayerTimingCalculator::exposure_at_layer(&r, i),
                25.0,
                "bottom-phase layer {i} must use bottom_exposure_sec",
            );
        }
    }

    #[test]
    fn exposure_transition_phase_interpolates() {
        let r = Recipe::generic_standard(); // transition_layers=3, bottom=25, normal=2.5
                                            // step = (2.5 - 25) / 4 = -5.625. After bottom (layer 6): 25 + step*1 = 19.375.
                                            // Transition layers: 6, 7, 8 → 19.375, 13.75, 8.125
        let e6 = LayerTimingCalculator::exposure_at_layer(&r, 6);
        let e7 = LayerTimingCalculator::exposure_at_layer(&r, 7);
        let e8 = LayerTimingCalculator::exposure_at_layer(&r, 8);
        assert!((e6 - 19.375).abs() < 1e-4, "layer 6 interp: got {e6}");
        assert!((e7 - 13.75).abs() < 1e-4, "layer 7 interp: got {e7}");
        assert!((e8 - 8.125).abs() < 1e-4, "layer 8 interp: got {e8}");
        // Monotonically decreasing through the ramp.
        assert!(e6 > e7 && e7 > e8);
    }

    #[test]
    fn exposure_normal_phase_after_transition() {
        let r = Recipe::generic_standard();
        // First post-transition layer = bottom_count + transition_layers = 9.
        for i in [9u32, 50, 500] {
            assert_eq!(
                LayerTimingCalculator::exposure_at_layer(&r, i),
                2.5,
                "post-transition layer {i} must use normal_exposure_sec",
            );
        }
    }

    // --- layer_time_sec Linear mechanism ---

    #[test]
    fn linear_layer_time_uses_both_lift_and_retract() {
        // Generic standard: lift_distance=5.0, lift_speed=60, retract=None (falls back to 60).
        // So t_lift = t_retract = 5/60*60 = 5.0 sec.
        // normal layer (1000): exposure=2.5, wait_before_cure=0.5, wait_before_release=1.0,
        //                      wait_after_release=0.0.
        // t_layer = 2.5 + 0.5 + 5.0 + 1.0 + 5.0 + 0.0 = 14.0 sec.
        let r = Recipe::generic_standard();
        let p = linear_printer();
        let t = LayerTimingCalculator::layer_time_sec(&r, &p, 1000);
        assert!((t - 14.0).abs() < 1e-4, "normal layer time: got {t}");
    }

    #[test]
    fn linear_retract_asymmetry_changes_layer_time() {
        // Explicit retract_speed = 180 mm/min (3× lift); same distance 5 mm.
        // t_lift = 5/60*60 = 5.0, t_retract = 5/180*60 = 1.667.
        // normal: 2.5 + 0.5 + 5.0 + 1.0 + 1.667 + 0.0 = 10.667
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(180.0),
        )
        .expect("explicit retract 180 mm/min is valid");
        let p = linear_printer();
        let t = LayerTimingCalculator::layer_time_sec(&r, &p, 1000);
        assert!(
            (t - 10.667).abs() < 1e-3,
            "asymmetric linear layer time: got {t}",
        );
        // With fallback (retract = lift), the same layer would be 14.0 — asymmetry saves time.
        let r_symmetric = Recipe::generic_standard();
        let t_sym = LayerTimingCalculator::layer_time_sec(&r_symmetric, &p, 1000);
        assert!(t < t_sym, "faster retract must reduce per-layer time");
    }

    #[test]
    fn linear_bottom_layer_takes_longer_than_normal() {
        let r = Recipe::generic_standard();
        let p = linear_printer();
        let t_bottom = LayerTimingCalculator::layer_time_sec(&r, &p, 0);
        let t_normal = LayerTimingCalculator::layer_time_sec(&r, &p, 1000);
        // Bottom exposure 25 vs normal 2.5 → 22.5 sec extra.
        assert!(
            t_bottom > t_normal + 20.0,
            "bottom layer should take > 20 sec longer than normal: bottom={t_bottom}, normal={t_normal}",
        );
    }

    // --- layer_time_sec Tilt mechanism ---

    #[test]
    fn tilt_layer_time_uses_lift_cycle_not_lift_retract() {
        // Tilt: exposure + wait_before_cure + lift_cycle_sec + wait_after_release.
        // Generic standard on Tilt printer:
        //   2.5 + 0.5 + 7.5 + 0.0 = 10.5.
        // Does NOT include wait_before_release (1.0) — tilt is atomic.
        // Does NOT use lift_distance/lift_speed/retract_speed.
        let r = Recipe::generic_standard();
        let p = tilt_printer();
        let t = LayerTimingCalculator::layer_time_sec(&r, &p, 1000);
        assert!((t - 10.5).abs() < 1e-4, "tilt normal layer time: got {t}");
    }

    #[test]
    fn tilt_ignores_retract_speed() {
        // Changing retract_speed must NOT affect Tilt layer time.
        let r_none = Recipe::generic_standard();
        let r_fast = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(500.0),
        )
        .expect("retract 500 mm/min is valid");
        let p = tilt_printer();
        let t_none = LayerTimingCalculator::layer_time_sec(&r_none, &p, 1000);
        let t_fast = LayerTimingCalculator::layer_time_sec(&r_fast, &p, 1000);
        assert_eq!(
            t_none, t_fast,
            "Tilt mechanism must ignore retract_speed_mm_min",
        );
    }

    #[test]
    fn tilt_ignores_lift_distance_and_lift_speed() {
        let r_slow = Recipe::generic_standard(); // lift_speed=60, lift_distance=5
        let r_fast = Recipe::new(50.0, 6, 3, 2.5, 25.0, 0.5, 1.0, 0.0, 200.0, 7.5, 50.0, None)
            .expect("larger lift_distance and lift_speed is valid");
        let p = tilt_printer();
        assert_eq!(
            LayerTimingCalculator::layer_time_sec(&r_slow, &p, 1000),
            LayerTimingCalculator::layer_time_sec(&r_fast, &p, 1000),
            "Tilt must ignore lift_distance_mm and lift_speed_mm_min",
        );
    }

    // --- cumulative_times_sec ---

    #[test]
    fn cumulative_time_is_monotonic() {
        let r = Recipe::generic_standard();
        let p = linear_printer();
        let cum = LayerTimingCalculator::cumulative_times_sec(&r, &p, 100);
        for i in 1..cum.len() {
            assert!(
                cum[i] >= cum[i - 1],
                "cumulative time must be non-decreasing at index {i}: {} vs {}",
                cum[i - 1],
                cum[i],
            );
        }
    }

    #[test]
    fn cumulative_time_bottom_phase_grows_faster() {
        // First 6 layers (bottom) should add more time than next 6 (post-transition normals).
        let r = Recipe::generic_standard();
        let p = linear_printer();
        let cum = LayerTimingCalculator::cumulative_times_sec(&r, &p, 100);
        let bottom_6 = cum[5]; // cumulative through layer 5 (inclusive)
        let normals_6 = cum[14] - cum[8]; // layers 9..=14 inclusive, 6 normal layers
        assert!(
            bottom_6 > normals_6,
            "6 bottom layers must take more time than 6 normal layers: {bottom_6} vs {normals_6}",
        );
    }

    #[test]
    fn cumulative_time_length_matches_total_layers() {
        let r = Recipe::generic_standard();
        let p = linear_printer();
        assert_eq!(
            LayerTimingCalculator::cumulative_times_sec(&r, &p, 500).len(),
            500,
        );
        assert_eq!(
            LayerTimingCalculator::cumulative_times_sec(&r, &p, 0).len(),
            0,
        );
    }
}
