use std::collections::HashMap;

use crate::entities::{PrinterProfile, ResinProfile};
use crate::io::{geometry, sliced::LayerInput, stl};
use crate::services::build_plate::PlateAdhesionProfile;
use crate::services::failure_predictor::{FailurePredictor, LayerOverrides, SupportConfig};
use crate::services::pairing_validator;
use crate::services::suction_detector::SuctionDetector;
use crate::simulation::PrintSimulation;
use crate::values::{CrossSectionArea, InitialLedTemperature, LayerMask, LayerPhase};

/// Application service: orchestrates a full simulation run.
/// Loads geometry, slices it, runs FailurePredictor per layer,
/// assembles the PrintSimulation aggregate.
pub struct SimulationRunner;

impl SimulationRunner {
    /// Run full simulation on an STL file.
    ///
    /// Ordering (ADR-0005 Consequences):
    ///   1. `resin.validate()` + `printer.validate()`
    ///   2. `pairing_validator::validate_pairing(printer, recipe)` — fail fast with ALL
    ///      violations BEFORE any geometry is sliced.
    ///   3. `slice_layers(..., recipe.layer_height_um, printer.voxel_size_mm)` — uses
    ///      the recipe + printer's configured voxel resolution.
    ///   4. Run FailurePredictor with mask-based SuctionDetector pre-pass.
    #[allow(clippy::too_many_arguments)]
    pub fn run_stl(
        stl_path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;
        let recipe = resin.recipe();

        let triangles = stl::load_stl(stl_path)?;
        let bbox = stl::bounding_box(&triangles);
        let geometries = geometry::slice_layers(
            &triangles,
            &bbox,
            recipe.layer_height_um(),
            printer.voxel_size_mm(),
        );
        let areas: Vec<CrossSectionArea> = geometries.iter().map(|g| g.area).collect();
        let masks: Vec<LayerMask> = geometries.into_iter().map(|g| g.mask).collect();
        Self::run_inner(
            &areas,
            &masks,
            None,
            resin,
            printer,
            supports,
            plate,
            ambient_c,
            initial_led_temp,
        )
    }

    /// Run simulation from pre-computed per-layer areas (area-only entry point).
    ///
    /// Mask-synthesising adapter (Phase B, Step 7, suction-detector-raft-false-positive):
    /// each area is represented as a fully-solid 1×1 LayerMask at the printer's
    /// voxel resolution. Fully-solid masks produce zero cavity events — correct
    /// for test fixtures whose areas represent solid cross-sections (e.g.
    /// `cube_areas`, `sphere_areas`). Callers that want to exercise cavity
    /// detection use [`run_from_layer_inputs`] with a bespoke LayerMask stack.
    ///
    /// Revalidation here is defence-in-depth per ADR-0005 §5.
    #[allow(clippy::too_many_arguments)]
    pub fn run_from_areas(
        areas: &[CrossSectionArea],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;

        let masks: Vec<LayerMask> = (0..areas.len())
            .map(|_| {
                LayerMask::new_all_solid(1, 1, printer.voxel_size_mm())
                    .expect("1×1 all-solid mask at validated positive voxel_size_mm constructs")
            })
            .collect();
        Self::run_inner(
            areas,
            &masks,
            None,
            resin,
            printer,
            supports,
            plate,
            ambient_c,
            initial_led_temp,
        )
    }

    /// Run simulation from parsed LayerInputs (from CTB or other sliced files).
    ///
    /// Uses per-layer exposure and lift speed from the sliced file; baseline recipe
    /// values (layer_height_um, bottom_exposure_sec, bottom_layer_count) come from
    /// `resin.recipe()`. Each `LayerInput` should carry a populated `mask` for
    /// cavity detection; inputs without a mask get a synthesised fully-solid 1×1
    /// mask at `printer.voxel_size_mm()` (no cavity events emitted for those layers).
    #[allow(clippy::too_many_arguments)]
    pub fn run_from_layer_inputs(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;

        let areas: Vec<CrossSectionArea> = layers
            .iter()
            .map(|li| {
                CrossSectionArea::new(li.cross_section_area_mm2)
                    .map_err(|e| format!("layer {}: {e}", li.index))
            })
            .collect::<Result<_, _>>()?;

        // Collect masks from LayerInputs, synthesising a fully-solid fallback
        // for any layer that doesn't carry one. Fallback voxel resolution must
        // match the mask-carrying layers to satisfy CavityDetector's consistency
        // precondition; pick it from the first carrying layer, or from
        // printer.voxel_size_mm() if none.
        let printer_voxel = printer.voxel_size_mm();
        let carrying_voxel = layers
            .iter()
            .find_map(|li| li.mask.as_ref().map(|m| m.voxel_size_mm()))
            .unwrap_or(printer_voxel);
        let carrying_dims = layers
            .iter()
            .find_map(|li| li.mask.as_ref().map(|m| (m.width_cells(), m.height_cells())))
            .unwrap_or((1, 1));
        let masks: Vec<LayerMask> = layers
            .iter()
            .map(|li| match &li.mask {
                Some(m) => m.clone(),
                None => LayerMask::new_all_solid(carrying_dims.0, carrying_dims.1, carrying_voxel)
                    .expect("consistent dims + positive voxel_size yields valid all-solid mask"),
            })
            .collect();

        let per_layer_overrides: Vec<(f32, f32)> = layers
            .iter()
            .map(|li| (li.exposure_sec, li.lift_speed_mm_min))
            .collect();
        Self::run_inner(
            &areas,
            &masks,
            Some(&per_layer_overrides),
            resin,
            printer,
            supports,
            plate,
            ambient_c,
            initial_led_temp,
        )
    }

    /// Internal: run the simulation given resolved areas + masks. Every public
    /// entry point converges here.
    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        areas: &[CrossSectionArea],
        masks: &[LayerMask],
        per_layer_overrides: Option<&[(f32, f32)]>,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        let recipe = resin.recipe();
        let suction_map = Self::build_suction_map(masks)?;
        let phases = LayerPhase::classify_sequence(areas, recipe);

        let mut sim = PrintSimulation::new();
        let mut prev_area = CrossSectionArea::new(0.0).expect("zero is valid");

        for (i, &area) in areas.iter().enumerate() {
            let (exposure_override, lift_speed_override) = per_layer_overrides
                .and_then(|pl| pl.get(i).copied())
                .map(|(e, l)| (Some(e), Some(l)))
                .unwrap_or((None, None));
            let overrides = LayerOverrides {
                exposure_sec: exposure_override,
                lift_speed_mm_min: lift_speed_override,
                suction_force_n: suction_map.get(&(i as u32)).copied(),
                is_raft: matches!(phases.get(i), Some(LayerPhase::Raft)),
                initial_led_temp,
            };
            let (result, failures) = FailurePredictor::predict_layer(
                i as u32, area, prev_area, &overrides, resin, printer, recipe, supports, plate,
                ambient_c,
            );
            sim.add_layer(result, failures);
            prev_area = area;
        }

        Ok(sim)
    }

    /// Run SuctionDetector mask-based pre-pass and build a layer→force map.
    ///
    /// Propagates `CavityError` as a human-readable string — callers of the
    /// public `run_*` entry points already return `Result<_, String>`.
    fn build_suction_map(masks: &[LayerMask]) -> Result<HashMap<u32, f32>, String> {
        let risks = SuctionDetector::detect_from_masks(masks)
            .map_err(|e| format!("suction detection: {e}"))?;
        Ok(risks
            .into_iter()
            .map(|r| (r.layer, r.suction_force_n))
            .collect())
    }

    /// Auto-detect format from file extension and run simulation.
    #[allow(clippy::too_many_arguments)]
    pub fn run_auto(
        path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient_c: f32,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        let format = crate::io::sliced::detect_format(path)
            .ok_or_else(|| format!("unknown file format: {}", path.display()))?;

        match format {
            "STL" => Self::run_stl(
                path,
                resin,
                printer,
                supports,
                plate,
                ambient_c,
                initial_led_temp,
            ),
            "CTB" => {
                let (_info, layers) = crate::io::ctb::parse_ctb(path)?;
                Self::run_from_layer_inputs(
                    &layers,
                    resin,
                    printer,
                    supports,
                    plate,
                    ambient_c,
                    initial_led_temp,
                )
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
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        let first_force = layers[10].total_force_n;
        let last_force = layers[99].total_force_n;
        assert!(
            (first_force - last_force).abs() < 0.1,
            "force should be ~constant: first={first_force}, last={last_force}"
        );
    }

    #[test]
    fn sphere_force_peaks_at_equator() {
        let areas = sphere_areas(200, 10.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 30 }, &default_plate(), 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let summary = sim.summary();
        assert!(
            summary.max_force_layer > 80 && summary.max_force_layer < 120,
            "max force should be near equator, got layer {}",
            summary.max_force_layer
        );
    }

    #[test]
    fn cube_no_critical_failures_with_adequate_supports() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert_eq!(
            sim.summary().critical_failures,
            0,
            "small cube should have no failures"
        );
    }

    #[test]
    fn large_area_with_plate_adhesion_may_survive() {
        // 5000 mm² cross section, peel = 65 N
        // 5 supports = 21.99 N, but interlayer bond = 50 × 5000 × 0.001 = 250 N
        // Total = 271.99 N >> 65 N → passes with interlayer bond
        let areas = cube_areas(50, 5000.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 5 }, &default_plate(), 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let overload_count = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SupportOverload)
            .count();
        assert_eq!(
            overload_count, 0,
            "interlayer bond should prevent support overload"
        );
    }

    #[test]
    fn no_supports_no_plate_fails() {
        // Remove both plate adhesion and supports → guaranteed failure
        let areas = cube_areas(50, 500.0);
        let no_plate = PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0,
            bottom_layer_count: 0,
            interlayer_bond_kpa: 0.0,
        };
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.0, n_supports: 0 }, &no_plate, 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert!(
            sim.summary().critical_failures > 0,
            "no supports + no plate should fail"
        );
    }

    /// Construct a closed-cup LayerInput stack with a bespoke LayerMask:
    /// `base_layers` solid layers (build-plate floor) → `wall_layers` ring-wall
    /// layers (trapped void interior) → `cap_layers` solid layers (FEP-side
    /// closure). Uses 7×7 voxel grid at 1mm voxel → 5×5 interior = 25 mm²
    /// sealed area (above the 1 N downstream threshold).
    ///
    /// Rewritten in Phase B Step 7 (suction-detector-raft-false-positive):
    /// previously this test used area-only sequences, which no longer exercise
    /// cavity detection under the mask-based path.
    fn closed_cup_layer_inputs(
        base_layers: usize,
        wall_layers: usize,
        cap_layers: usize,
        exposure_sec: f32,
        layer_height_um: f32,
        lift_speed_mm_min: f32,
    ) -> Vec<LayerInput> {
        let w = 7u32;
        let h = 7u32;
        let voxel = 1.0_f32;

        let solid_mask = LayerMask::new_all_solid(w, h, voxel)
            .expect("7×7 @ 1mm mask constructs");
        let ring_mask = {
            let mut m = LayerMask::new_all_solid(w, h, voxel)
                .expect("7×7 @ 1mm mask constructs");
            for x in 1..w - 1 {
                for y in 1..h - 1 {
                    m.clear(x, y).expect("interior cell in bounds");
                }
            }
            m
        };

        let solid_area = (w as f64) * (h as f64) * (voxel as f64).powi(2); // 49 mm²
        let ring_area = solid_area - 25.0; // 24 mm² (wall ring)

        let mut layers = Vec::new();
        let mut idx: u32 = 0;
        let mut z_mm = 0.0_f32;
        let layer_height_mm = layer_height_um / 1000.0;
        for _ in 0..base_layers {
            layers.push(
                LayerInput::new(idx, solid_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                    .expect("valid LayerInput")
                    .with_mask(solid_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        for _ in 0..wall_layers {
            layers.push(
                LayerInput::new(idx, ring_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                    .expect("valid LayerInput")
                    .with_mask(ring_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        for _ in 0..cap_layers {
            layers.push(
                LayerInput::new(idx, solid_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                    .expect("valid LayerInput")
                    .with_mask(solid_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        layers
    }

    #[test]
    fn closed_cup_triggers_suction_warning() {
        // 5 solid base + 10 ring walls + 1 cap (layer 15 is the closure).
        // Interior = 5×5 = 25 mm² at 1mm voxel. Force = 50 kPa × 25 × 1e-3 = 1.25 N
        // — above FailurePredictor's 1 N emission gate.
        let layers = closed_cup_layer_inputs(5, 10, 1, 2.5, 50.0, 60.0);
        let sim = SimulationRunner::run_from_layer_inputs(
            &layers,
            &ResinProfile::generic_standard(),
            &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 10 },
            &default_plate(),
            22.0,
            None,
        )
        .expect("test fixture: validated profiles satisfy run_from_layer_inputs preconditions");
        let suction_events: Vec<_> = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .collect();
        assert!(
            !suction_events.is_empty(),
            "closed cup should trigger suction warning, got: {:?}",
            sim.failures()
        );
        // Event at the closure layer (15 = 5 base + 10 walls).
        assert_eq!(suction_events[0].layer, 15);
    }

    #[test]
    fn solid_cube_no_suction() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0, None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let suction_count = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .count();
        assert_eq!(suction_count, 0, "solid cube should have no suction");
    }

    #[test]
    fn suction_adds_to_total_force() {
        // Same fixture as closed_cup_triggers_suction_warning; check that the
        // closure layer's total_force exceeds its peel_force.
        let layers = closed_cup_layer_inputs(5, 10, 1, 2.5, 50.0, 60.0);
        let sim = SimulationRunner::run_from_layer_inputs(
            &layers,
            &ResinProfile::generic_standard(),
            &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 10 },
            &default_plate(),
            22.0,
            None,
        )
        .expect("test fixture: validated profiles satisfy run_from_layer_inputs preconditions");
        let closure_layer = &sim.layers()[15];
        assert!(
            closure_layer.suction_force_n > 0.0,
            "closure layer should have suction force, got {}",
            closure_layer.suction_force_n
        );
        assert!(
            closure_layer.total_force_n > closure_layer.peel_force_n,
            "total should exceed peel when suction present"
        );
    }

    #[test]
    fn temperature_rises_over_long_print() {
        let areas = cube_areas(500, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), 22.0, None,
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
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            22.0,
            None,
        );
        assert!(
            result.is_err(),
            "invalid profile should be rejected at entry point"
        );
    }

    // --- ADR-0005: pairing runs before slicing. Locks the ordering invariant that
    // a recipe outside the printer envelope fails fast at simulation entry, not
    // after geometry has been sliced into layer areas. ---

    #[test]
    fn pairing_violation_returns_err_before_slice_areas() {
        // Narrow printer envelope that excludes the resin's recipe layer height.
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.layer_height_range_um = crate::values::FloatRange::new(100.0, 150.0)
            .expect("test fixture: 100..150 µm range is valid");
        let areas = cube_areas(5, 100.0);
        // generic_standard recipe has layer_height_um = 50.0 → outside the narrowed range.
        let err = SimulationRunner::run_from_areas(
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            22.0,
            None,
        )
        .expect_err("pairing violation must fail simulation entry");
        assert!(
            err.starts_with("pairing:"),
            "err must identify pairing stage: {err}"
        );
        assert!(
            err.contains("layer_height_um"),
            "err must name the offending recipe field: {err}"
        );
    }

    #[test]
    fn pairing_reports_all_violations_at_once() {
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.layer_height_range_um = crate::values::FloatRange::new(100.0, 150.0)
            .expect("test fixture: 100..150 µm range is valid");
        printer.exposure_range_sec = crate::values::FloatRange::new(10.0, 60.0)
            .expect("test fixture: 10..60 sec range is valid");
        let areas = cube_areas(5, 100.0);
        let err = SimulationRunner::run_from_areas(
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            22.0,
            None,
        )
        .expect_err("multiple pairing violations must fail");
        // Both layer_height AND exposure violate; violations are joined with "; ".
        assert!(err.contains("layer_height_um"));
        assert!(err.contains("normal_exposure_sec"));
    }
}
