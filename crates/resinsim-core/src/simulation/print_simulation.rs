use serde::{Deserialize, Serialize};

use crate::entities::{FailureEvent, LayerResult, Severity};

/// Aggregate root: a complete simulation run for one geometry + resin + printer.
/// Layers and failures are only mutated through the root's methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintSimulation {
    layers: Vec<LayerResult>,
    failures: Vec<FailureEvent>,
}

/// Summary statistics for a completed simulation.
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
}

impl Default for PrintSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl PrintSimulation {
    pub fn new() -> Self {
        Self {
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

    /// Compute summary statistics. Requires at least one layer.
    pub fn summary(&self) -> SimSummary {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::LayerResult;

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
        let mut sim = PrintSimulation::new();
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]);
        sim.add_layer(make_layer(1, 6.0, 2.5, 22.5), vec![]);
        assert_eq!(sim.layers().len(), 2);
    }

    #[test]
    #[should_panic(expected = "layers must be sequential")]
    fn non_sequential_layer_panics() {
        let mut sim = PrintSimulation::new();
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]);
        sim.add_layer(make_layer(5, 6.0, 2.5, 22.5), vec![]); // skip
    }

    #[test]
    fn summary_finds_extremes() {
        let mut sim = PrintSimulation::new();
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
}
