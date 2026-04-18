use std::collections::HashMap;

use crate::entities::{PrinterProfile, ResinProfile};
use crate::io::{geometry, sliced::LayerInput, stl};
use crate::services::build_plate::PlateAdhesionProfile;
use crate::services::failure_predictor::{FailurePredictor, LayerOverrides, SupportConfig};
use crate::services::suction_detector::SuctionDetector;
use crate::simulation::PrintSimulation;
use crate::values::CrossSectionArea;

/// Application service: orchestrates a full simulation run.
/// Loads geometry, slices it, runs FailurePredictor per layer,
/// assembles the PrintSimulation aggregate.
pub struct SimulationRunner;

impl SimulationRunner {
    /// Run full simulation on an STL file.
    pub fn run_stl(
        stl_path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
    ) -> Result<PrintSimulation, String> {
        let triangles = stl::load_stl(stl_path)?;
        let bbox = stl::bounding_box(&triangles);
        let areas = geometry::slice_areas(&triangles, &bbox, printer.layer_height_um);

        Self::run_from_areas(&areas, resin, printer, supports, plate, ambient_c)
    }

    /// Run simulation from pre-computed per-layer areas.
    pub fn run_from_areas(
        areas: &[CrossSectionArea],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;

        // Pre-pass: detect suction risks and raft layers
        let suction_map = Self::build_suction_map(areas);
        let raft_end = Self::detect_raft_end(areas);

        let mut sim = PrintSimulation::new();
        let mut prev_area = CrossSectionArea::new(0.0).expect("zero is valid");

        for (i, &area) in areas.iter().enumerate() {
            let overrides = LayerOverrides {
                suction_force_n: suction_map.get(&(i as u32)).copied(),
                is_raft: (i as u32) < raft_end,
                ..Default::default()
            };
            let (result, failures) = FailurePredictor::predict_layer(
                i as u32, area, prev_area,
                &overrides, resin, printer, supports, plate, ambient_c,
            );
            sim.add_layer(result, failures);
            prev_area = area;
        }

        Ok(sim)
    }

    /// Run simulation from parsed LayerInputs (from CTB or other sliced files).
    /// Uses per-layer exposure and lift speed from the sliced file.
    pub fn run_from_layer_inputs(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;

        // Pre-pass: detect suction risks and raft layers
        let areas: Vec<CrossSectionArea> = layers.iter()
            .map(|li| CrossSectionArea::new(li.cross_section_area_mm2)
                .map_err(|e| format!("layer {}: {e}", li.index)))
            .collect::<Result<_, _>>()?;
        let suction_map = Self::build_suction_map(&areas);
        let raft_end = Self::detect_raft_end(&areas);

        let mut sim = PrintSimulation::new();
        let mut prev_area = CrossSectionArea::new(0.0).expect("zero is valid");

        for (li, area) in layers.iter().zip(areas.iter().copied()) {
            let overrides = LayerOverrides {
                exposure_sec: Some(li.exposure_sec),
                lift_speed_mm_min: Some(li.lift_speed_mm_min),
                suction_force_n: suction_map.get(&li.index).copied(),
                is_raft: li.index < raft_end,
            };
            let (result, failures) = FailurePredictor::predict_layer(
                li.index, area, prev_area,
                &overrides, resin, printer, supports, plate, ambient_c,
            );
            sim.add_layer(result, failures);
            prev_area = area;
        }

        Ok(sim)
    }

    /// Run SuctionDetector heuristic pre-pass and build a layer→force map.
    fn build_suction_map(areas: &[CrossSectionArea]) -> HashMap<u32, f32> {
        let risks = SuctionDetector::detect_from_areas(areas, None);
        risks.into_iter().map(|r| (r.layer, r.suction_force_n)).collect()
    }

    /// Detect raft layers: initial layers with large constant area followed
    /// by a >50% area drop. Everything before the drop is raft.
    /// Returns the index of the first non-raft layer (0 if no raft detected).
    fn detect_raft_end(areas: &[CrossSectionArea]) -> u32 {
        if areas.len() < 3 {
            return 0;
        }
        let first_area = areas[0].value();
        if first_area < 10.0 {
            return 0; // no significant raft
        }
        for (i, a) in areas.iter().enumerate().skip(1) {
            let ratio = a.value() / first_area;
            if ratio < 0.5 {
                return i as u32;
            }
            // Also break if area starts growing significantly (model started)
            if a.value() > first_area * 1.2 {
                return 0; // not a raft pattern
            }
        }
        0 // constant area throughout — not a raft
    }

    /// Auto-detect format from file extension and run simulation.
    pub fn run_auto(
        path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
    ) -> Result<PrintSimulation, String> {
        let format = crate::io::sliced::detect_format(path)
            .ok_or_else(|| format!("unknown file format: {}", path.display()))?;

        match format {
            "STL" => Self::run_stl(path, resin, printer, supports, plate, ambient_c),
            "CTB" => {
                let (_info, layers) = crate::io::ctb::parse_ctb(path)?;
                Self::run_from_layer_inputs(&layers, resin, printer, supports, plate, ambient_c)
            }
            other => Err(format!("format {other} not yet supported for simulation")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_plate() -> PlateAdhesionProfile {
        PlateAdhesionProfile::default_textured()
    }

    fn cube_areas(n_layers: usize, area: f64) -> Vec<CrossSectionArea> {
        vec![CrossSectionArea::new(area).expect("test area is non-negative"); n_layers]
    }

    fn sphere_areas(n_layers: usize, radius_mm: f64) -> Vec<CrossSectionArea> {
        let layer_height = 2.0 * radius_mm / n_layers as f64;
        (0..n_layers)
            .map(|i| {
                let h = (i as f64 + 0.5) * layer_height;
                let d = (h - radius_mm).abs();
                let a = std::f64::consts::PI * (radius_mm * radius_mm - d * d);
                CrossSectionArea::new(a.max(0.0)).expect("max(0.0) guarantees non-negative")
            })
            .collect()
    }

    #[test]
    fn cube_constant_force_across_layers() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        let first_force = layers[10].total_force_n;
        let last_force = layers[99].total_force_n;
        assert!((first_force - last_force).abs() < 0.1,
            "force should be ~constant: first={first_force}, last={last_force}");
    }

    #[test]
    fn sphere_force_peaks_at_equator() {
        let areas = sphere_areas(200, 10.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 30 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let summary = sim.summary();
        assert!(summary.max_force_layer > 80 && summary.max_force_layer < 120,
            "max force should be near equator, got layer {}", summary.max_force_layer);
    }

    #[test]
    fn cube_no_critical_failures_with_adequate_supports() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert_eq!(sim.summary().critical_failures, 0, "small cube should have no failures");
    }

    #[test]
    fn large_area_with_plate_adhesion_may_survive() {
        // 5000 mm² cross section, peel = 65 N
        // 5 supports = 21.99 N, but interlayer bond = 50 × 5000 × 0.001 = 250 N
        // Total = 271.99 N >> 65 N → passes with interlayer bond
        let areas = cube_areas(50, 5000.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 5 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let overload_count = sim.failures().iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SupportOverload)
            .count();
        assert_eq!(overload_count, 0, "interlayer bond should prevent support overload");
    }

    #[test]
    fn no_supports_no_plate_fails() {
        // Remove both plate adhesion and supports → guaranteed failure
        let areas = cube_areas(50, 500.0);
        let no_plate = PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0, bottom_layer_count: 0, interlayer_bond_kpa: 0.0,
        };
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.0, n_supports: 0 }, &no_plate, 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert!(sim.summary().critical_failures > 0, "no supports + no plate should fail");
    }

    /// Simulate a hollow cup: solid base transitions to thin ring walls.
    /// SuctionDetector should flag the transition layer.
    fn hollow_cup_areas(n_layers: usize, base_layers: usize, base_area: f64, wall_area: f64) -> Vec<CrossSectionArea> {
        (0..n_layers)
            .map(|i| {
                if i < base_layers {
                    CrossSectionArea::new(base_area).expect("test area is non-negative")
                } else {
                    CrossSectionArea::new(wall_area).expect("test area is non-negative")
                }
            })
            .collect()
    }

    #[test]
    fn hollow_cup_triggers_suction_warning() {
        // Solid base (314 mm²) transitions to thin ring (60 mm²) → 80% area drop
        // SuctionDetector heuristic flags this as suction risk
        let areas = hollow_cup_areas(20, 5, 314.0, 60.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 10 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let suction_events: Vec<_> = sim.failures().iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .collect();
        assert!(!suction_events.is_empty(),
            "hollow cup should trigger suction warning, got: {:?}", sim.failures());
        // Suction should be at transition layer (5)
        assert_eq!(suction_events[0].layer, 5);
    }

    #[test]
    fn solid_cube_no_suction() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let suction_count = sim.failures().iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .count();
        assert_eq!(suction_count, 0, "solid cube should have no suction");
    }

    #[test]
    fn suction_adds_to_total_force() {
        // Check that layers with suction have higher total force
        let areas = hollow_cup_areas(20, 5, 314.0, 60.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 10 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        // Layer 5 has suction, layer 6 has same wall area but no new suction trigger
        // Both have same peel from adhesion, but layer 5 should have suction force added
        let layer_5 = &layers[5];
        assert!(layer_5.suction_force_n > 0.0,
            "layer 5 should have suction force, got {}", layer_5.suction_force_n);
        assert!(layer_5.total_force_n > layer_5.peel_force_n,
            "total should exceed peel when suction present");
    }

    #[test]
    fn temperature_rises_over_long_print() {
        let areas = cube_areas(500, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        assert!(layers[490].vat_temperature_c > layers[10].vat_temperature_c + 3.0);
        assert!(layers[490].viscosity_mpa_s < layers[10].viscosity_mpa_s);
    }

    // --- Step 12: profile fixture invariant tests ---

    #[test]
    fn generic_msla_4k_passes_validate() {
        PrinterProfile::generic_msla_4k()
            .validate()
            .expect("PrinterProfile::generic_msla_4k() factory must satisfy validate()");
    }

    #[test]
    fn generic_standard_resin_passes_validate() {
        ResinProfile::generic_standard()
            .validate()
            .expect("ResinProfile::generic_standard() factory must satisfy validate()");
    }

    #[test]
    fn invalid_printer_profile_returns_err() {
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.lcd_uniformity_variation = 2.0; // outside [0, 1]
        let areas = cube_areas(5, 100.0);
        let result = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &printer,
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 10 }, &default_plate(), 22.0,
        );
        assert!(result.is_err(), "invalid profile should be rejected at entry point");
    }
}
