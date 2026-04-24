use serde::{Deserialize, Serialize};

use crate::entities::{FailureEvent, LayerResult, PrinterProfile, Recipe, Severity};
use crate::services::LayerTimingCalculator;

/// Aggregate root: a complete simulation run for one geometry + resin + printer.
///
/// The aggregate OWNS the `Recipe` and `PrinterProfile` it was constructed
/// with. Projections (`summary()`, future force/temperature stats) read
/// those owned domain entities internally — callers don't re-thread them.
/// Layers and failures are only mutated through the root's methods.
///
/// # Deserialize note
///
/// `#[derive(Deserialize)]` reconstructs the aggregate directly from fields
/// — it does NOT call `Recipe::validate()` or `PrinterProfile::validate()`
/// on the child entities. Current code has zero external deserializers of
/// `PrintSimulation`; if a future consumer deserializes one, wrap the
/// output with an explicit validate() pass on the child entities before
/// treating the aggregate as trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintSimulation {
    /// Domain-validated `Recipe` (Clone-owned) used by `summary()` phase-time
    /// projection. Immutable for the aggregate's lifetime — no public
    /// mutator exposed.
    recipe: Recipe,
    /// Domain-validated `PrinterProfile` (Clone-owned) used by `summary()`
    /// phase-time projection. Immutable for the aggregate's lifetime.
    printer: PrinterProfile,
    layers: Vec<LayerResult>,
    failures: Vec<FailureEvent>,
}

/// Summary statistics for a completed simulation.
///
/// Time fields (`total_time_sec`, `bottom_time_sec`, `transition_time_sec`,
/// `normal_time_sec`) are filled from `LayerTimingCalculator::cumulative_times_sec`
/// against the aggregate's owned `Recipe + PrinterProfile`. Per-phase fields
/// sum to `total_time_sec` within f32 tolerance; all are zero when
/// `total_layers == 0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimSummary {
    pub total_layers: u32,
    pub critical_failures: usize,
    pub warnings: usize,
    pub max_peel_force_n: f32,
    pub max_force_layer: u32,
    pub min_safety_factor: f32,
    pub min_safety_layer: u32,
    pub max_temperature_c: f32,
    pub max_z_deflection_um: f32,
    pub total_time_sec: f32,
    pub bottom_time_sec: f32,
    pub transition_time_sec: f32,
    pub normal_time_sec: f32,
}

impl PrintSimulation {
    /// Construct an empty aggregate pinned to a `Recipe + PrinterProfile`
    /// pair. Both entities are Clone-owned; callers pass by value (typically
    /// via `.clone()` from refs they already hold).
    pub fn new(recipe: Recipe, printer: PrinterProfile) -> Self {
        Self {
            recipe,
            printer,
            layers: Vec::new(),
            failures: Vec::new(),
        }
    }

    /// Add a layer result and its associated failures.
    /// Enforces invariant: layers must be added sequentially.
    pub fn add_layer(&mut self, result: LayerResult, mut layer_failures: Vec<FailureEvent>) {
        let expected = self.layers.len() as u32;
        assert_eq!(
            result.index, expected,
            "layers must be sequential: expected {expected}, got {}",
            result.index
        );
        self.layers.push(result);
        self.failures.append(&mut layer_failures);
    }

    pub fn layers(&self) -> &[LayerResult] {
        &self.layers
    }

    pub fn failures(&self) -> &[FailureEvent] {
        &self.failures
    }

    pub fn critical_failures(&self) -> Vec<&FailureEvent> {
        self.failures
            .iter()
            .filter(|f| f.severity == Severity::Critical)
            .collect()
    }

    /// Compute summary statistics, including per-phase print duration.
    ///
    /// Reads `self.recipe + self.printer` (set at construction) to project
    /// phase times via `LayerTimingCalculator`. Short-print clamp semantics
    /// cover every layer-count regime (see module tests): when
    /// `total_layers` is zero, all time fields are zero; when it falls
    /// inside the bottom phase, transition + normal are zero; etc.
    pub fn summary(&self) -> SimSummary {
        let total_layers = self.layers.len() as u32;
        let (total_time_sec, bottom_time_sec, transition_time_sec, normal_time_sec) =
            self.phase_times(total_layers);

        if self.layers.is_empty() {
            return SimSummary {
                total_layers: 0,
                critical_failures: 0,
                warnings: 0,
                max_peel_force_n: 0.0,
                max_force_layer: 0,
                min_safety_factor: f32::INFINITY,
                min_safety_layer: 0,
                max_temperature_c: 0.0,
                max_z_deflection_um: 0.0,
                total_time_sec,
                bottom_time_sec,
                transition_time_sec,
                normal_time_sec,
            };
        }

        let mut max_force = f32::NEG_INFINITY;
        let mut max_force_layer = 0u32;
        let mut min_sf = f32::INFINITY;
        let mut min_sf_layer = 0u32;
        let mut max_temp = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;

        for lr in &self.layers {
            if lr.total_force_n > max_force {
                max_force = lr.total_force_n;
                max_force_layer = lr.index;
            }
            if lr.safety_factor < min_sf {
                min_sf = lr.safety_factor;
                min_sf_layer = lr.index;
            }
            if lr.vat_temperature_c > max_temp {
                max_temp = lr.vat_temperature_c;
            }
            if lr.z_deflection_um > max_z {
                max_z = lr.z_deflection_um;
            }
        }

        SimSummary {
            total_layers: self.layers.len() as u32,
            critical_failures: self
                .failures
                .iter()
                .filter(|f| f.severity == Severity::Critical)
                .count(),
            warnings: self
                .failures
                .iter()
                .filter(|f| f.severity == Severity::Warning)
                .count(),
            max_peel_force_n: max_force,
            max_force_layer,
            min_safety_factor: min_sf,
            min_safety_layer: min_sf_layer,
            max_temperature_c: max_temp,
            max_z_deflection_um: max_z,
            total_time_sec,
            bottom_time_sec,
            transition_time_sec,
            normal_time_sec,
        }
    }

    // (total, bottom, transition, normal) in seconds.
    // Explicit clamps cover every length regime: n=0 ⇒ all zero;
    // n within bottom phase ⇒ transition+normal zero; n in transition ⇒ normal zero.
    fn phase_times(&self, total_layers: u32) -> (f32, f32, f32, f32) {
        if total_layers == 0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let cumulative =
            LayerTimingCalculator::cumulative_times_sec(&self.recipe, &self.printer, total_layers);
        let bottom_count = self.recipe.bottom_layer_count();
        let transition_count = self.recipe.transition_layers();
        let bottom_n = bottom_count.min(total_layers) as usize;
        let trans_end = (bottom_count.saturating_add(transition_count)).min(total_layers) as usize;
        let n = total_layers as usize;

        let bottom_t = if bottom_n == 0 {
            0.0
        } else {
            cumulative[bottom_n - 1]
        };
        let transition_t = if trans_end <= bottom_n {
            0.0
        } else {
            cumulative[trans_end - 1] - bottom_t
        };
        let normal_t = if n <= trans_end {
            0.0
        } else {
            cumulative[n - 1] - cumulative[trans_end - 1]
        };
        let total_t = cumulative[n - 1];
        (total_t, bottom_t, transition_t, normal_t)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::entities::{LayerResult, PrinterProfile, Recipe, ResinProfile};

    /// Shared fixture: factory-default Recipe (generic_standard resin).
    /// `pub(crate)` so report_generator.rs test mod can reuse.
    pub(crate) fn default_recipe() -> Recipe {
        ResinProfile::generic_standard().recipe().clone()
    }

    /// Shared fixture: factory-default Linear PrinterProfile (generic_msla_4k).
    /// `pub(crate)` so report_generator.rs test mod can reuse.
    pub(crate) fn linear_printer() -> PrinterProfile {
        PrinterProfile::generic_msla_4k()
    }

    fn make_layer(index: u32, force: f32, sf: f32, temp: f32) -> LayerResult {
        LayerResult {
            index,
            cure_depth_um: 100.0,
            peel_force_n: force,
            suction_force_n: 0.0,
            total_force_n: force,
            support_capacity_n: force * sf,
            safety_factor: sf,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: temp,
            viscosity_mpa_s: 200.0,
            z_deflection_um: force / 0.46, // k=460
            effective_layer_height_um: 50.0 - force / 0.46,
            worst_cure_depth_um: 100.0,
        }
    }

    #[test]
    fn sequential_layers_accepted() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]);
        sim.add_layer(make_layer(1, 6.0, 2.5, 22.5), vec![]);
        assert_eq!(sim.layers().len(), 2);
    }

    #[test]
    #[should_panic(expected = "layers must be sequential")]
    fn non_sequential_layer_panics() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]);
        sim.add_layer(make_layer(5, 6.0, 2.5, 22.5), vec![]); // skip
    }

    #[test]
    fn summary_finds_extremes() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]);
        sim.add_layer(
            make_layer(1, 20.0, 0.8, 25.0),
            vec![FailureEvent {
                layer: 1,
                failure_type: crate::entities::FailureType::SupportOverload,
                severity: Severity::Critical,
                message: "test".into(),
            }],
        );
        sim.add_layer(make_layer(2, 10.0, 2.0, 24.0), vec![]);

        let s = sim.summary();
        assert_eq!(s.total_layers, 3);
        assert_eq!(s.max_force_layer, 1);
        assert!((s.max_peel_force_n - 20.0).abs() < 1e-6);
        assert_eq!(s.min_safety_layer, 1);
        assert!((s.min_safety_factor - 0.8).abs() < 1e-6);
        assert!((s.max_temperature_c - 25.0).abs() < 1e-6);
        assert_eq!(s.critical_failures, 1);
    }

    // SimSummary per-phase time field tests — ported from the orphan
    // (jj:rnvrvtprzxmt `feat(print-time-report)`). In v4 the aggregate owns
    // recipe+printer, so summary() is arg-less — tests construct
    // PrintSimulation::new(recipe, printer) up-front.

    #[test]
    fn summary_includes_total_time() {
        // 100-layer Linear + generic_standard run: expected total equals
        // LayerTimingCalculator::cumulative_times_sec(...).last().
        let recipe = default_recipe();
        let printer = linear_printer();
        let mut sim = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..100 {
            sim.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![]);
        }
        let s = sim.summary();
        let expected = *LayerTimingCalculator::cumulative_times_sec(&recipe, &printer, 100)
            .last()
            .expect("100 layers produces a non-empty cumulative vector");
        assert!(
            (s.total_time_sec - expected).abs() < 1e-4,
            "total_time_sec should match calculator.last(): got {}, expected {expected}",
            s.total_time_sec,
        );
        assert!(
            s.total_time_sec > 0.0,
            "non-empty run must have positive total time"
        );
    }

    #[test]
    fn summary_per_phase_adds_to_total() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        for i in 0..100 {
            sim.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![]);
        }
        let s = sim.summary();
        let sum = s.bottom_time_sec + s.transition_time_sec + s.normal_time_sec;
        let tol = (s.total_time_sec.abs() * 1e-3).max(1e-6);
        assert!(
            (sum - s.total_time_sec).abs() < tol,
            "phase sum {sum} should equal total {} within {tol}",
            s.total_time_sec,
        );
    }

    #[test]
    fn summary_single_bottom_layer() {
        // n=1 with default recipe (bottom_count=6) — firmly in the bottom phase.
        // Exercises cumulative[0] indexing + the bottom_n=min(6,1)=1 clamp branch.
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 1.0, 3.0, 22.0), vec![]);
        let s = sim.summary();
        assert!(s.bottom_time_sec > 0.0);
        assert_eq!(s.transition_time_sec, 0.0);
        assert_eq!(s.normal_time_sec, 0.0);
        assert!((s.bottom_time_sec - s.total_time_sec).abs() < 1e-4);
    }

    #[test]
    fn summary_short_print_clamps() {
        let recipe = default_recipe();
        let printer = linear_printer();

        // total_layers == 0: all time fields zero, no panic.
        let empty = PrintSimulation::new(recipe.clone(), printer.clone()).summary();
        assert_eq!(empty.total_layers, 0);
        assert_eq!(empty.total_time_sec, 0.0);
        assert_eq!(empty.bottom_time_sec, 0.0);
        assert_eq!(empty.transition_time_sec, 0.0);
        assert_eq!(empty.normal_time_sec, 0.0);

        // total_layers == bottom_count - 1: only bottom_time_sec non-zero.
        let bottom_only_n = recipe.bottom_layer_count() - 1;
        let mut sim_b = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..bottom_only_n {
            sim_b.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![]);
        }
        let b = sim_b.summary();
        assert!(b.bottom_time_sec > 0.0);
        assert_eq!(b.transition_time_sec, 0.0);
        assert_eq!(b.normal_time_sec, 0.0);
        assert!((b.bottom_time_sec - b.total_time_sec).abs() < 1e-4);

        // total_layers == bottom_count + transition_layers - 1: normal stays zero.
        let mid_n = recipe.bottom_layer_count() + recipe.transition_layers() - 1;
        let mut sim_t = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..mid_n {
            sim_t.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![]);
        }
        let t = sim_t.summary();
        assert!(t.bottom_time_sec > 0.0);
        assert!(t.transition_time_sec > 0.0);
        assert_eq!(t.normal_time_sec, 0.0);
    }
}
