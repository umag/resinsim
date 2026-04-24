use crate::entities::{
    FailureEvent, FailureType, LayerResult, PrinterProfile, Recipe, ResinProfile, Severity,
};
use crate::services::build_plate::PlateAdhesionProfile;
use crate::services::uniformity_calculator::UniformityProfile;
use crate::services::{
    CureCalculator, PeelForceCalculator, SupportAnalyzer, ThermalCalculator, UniformityCalculator,
    ZAxisCompensator,
};
use crate::values::{
    AmbientTemperature, CrossSectionArea, Energy, InitialLedTemperature, PeelForce,
    PenetrationDepth,
};

/// Domain service: orchestrates all physics checks for a single layer.
/// Produces a LayerResult and any FailureEvents.
///
/// Composes: CureCalculator, PeelForceCalculator, ThermalCalculator, ZAxisCompensator.
pub struct FailurePredictor;

/// Configuration for support geometry (simplified for Tier 1).
#[derive(Debug, Clone)]
pub struct SupportConfig {
    /// Tip contact radius in mm.
    pub tip_radius_mm: f32,
    /// Number of supports at each layer (simplified: constant).
    pub n_supports: u32,
}

/// Print-wide thermal context — constants that apply to every layer of a run.
///
/// Constructed once at the SimulationRunner entry point and threaded by
/// reference through `predict_layer` (sibling aggregate to `SupportConfig` +
/// `PlateAdhesionProfile`). Addresses the step-10 code-review LOW: these
/// values are load-bearing for every layer's thermal calculation but are NOT
/// per-layer overrides from sliced-file data or pre-pass analysis — keeping
/// them out of `LayerOverrides` clarifies intent.
#[derive(Debug, Clone, Copy)]
pub struct ThermalContext {
    /// Room ambient temperature. User-supplied, not profile-sourced. Typed
    /// via `AmbientTemperature` (finite, above absolute zero) so unphysical
    /// values fail at construction in the caller.
    pub ambient: AmbientTemperature,
    /// Initial LED case temperature at print start (ADR-0007 / KB-152).
    /// When `None`, falls back to `ambient` — legacy single-stage behaviour
    /// where the LED is assumed to start at ambient. Typed via
    /// `InitialLedTemperature` so unphysical values fail at construction in
    /// the caller, not as a panic mid-simulation.
    pub initial_led_temp: Option<InitialLedTemperature>,
}

/// Threshold for rapid area increase warning.
const AREA_DELTA_WARN_MM2: f64 = 100.0;

/// Per-layer overrides from sliced file data and pre-pass analysis.
/// When None, falls back to printer profile defaults.
#[derive(Debug, Clone, Default)]
pub struct LayerOverrides {
    pub exposure_sec: Option<f32>,
    pub lift_speed_mm_min: Option<f32>,
    /// Suction force in Newtons from SuctionDetector pre-pass.
    pub suction_force_n: Option<f32>,
    /// Whether this layer is part of the raft/support base.
    /// Z deflection on raft layers is downgraded to Info (not Critical).
    pub is_raft: bool,
}

impl FailurePredictor {
    /// Simulate a single layer and return result + failures.
    // The 10 arguments each name a distinct physical input; bundling them into a
    // `PredictLayerInputs` struct would move the collection point one frame up
    // without reducing the caller's parameter count. Documented as accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn predict_layer(
        layer: u32,
        area: CrossSectionArea,
        prev_area: CrossSectionArea,
        overrides: &LayerOverrides,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        recipe: &Recipe,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        thermal: &ThermalContext,
    ) -> (LayerResult, Vec<FailureEvent>) {
        let mut failures = Vec::new();

        // --- Thermal (ADR-0007 two-stage: LED → vat via coupling) ---
        let vat_temp = ThermalCalculator::vat_temperature_at_layer_v2(
            recipe,
            printer,
            thermal.ambient.value(),
            thermal.initial_led_temp.map(|t| t.value()),
            layer,
        );
        let viscosity = ThermalCalculator::viscosity_at_temperature(
            resin.viscosity_mpa_s,
            resin.reference_temp_c,
            vat_temp.value(),
            resin.activation_energy_kj_mol,
        );

        if resin.is_degradation_risk(vat_temp) {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::ThermalDegradation,
                severity: Severity::Warning,
                message: format!(
                    "Vat temperature {:.1}°C exceeds {:.1}°C degradation threshold",
                    vat_temp.value(),
                    resin.degradation_temp_c,
                ),
            });
        }

        // --- Cure depth ---
        let exposure_sec =
            overrides
                .exposure_sec
                .unwrap_or(if layer < recipe.bottom_layer_count() {
                    recipe.bottom_exposure_sec()
                } else {
                    recipe.normal_exposure_sec()
                });
        let energy = Energy::from_exposure(printer.led_power_mw_cm2, exposure_sec)
            .expect("PrinterProfile::validate() guarantees led_power_mw_cm2 > 0; exposure_sec from recipe or override is positive");
        let dp = PenetrationDepth::new(resin.penetration_depth_um)
            .expect("ResinProfile::validate() guarantees penetration_depth_um > 0");
        let ec_ref = Energy::new(resin.critical_energy_mj_cm2)
            .expect("ResinProfile::validate() guarantees critical_energy_mj_cm2 > 0");
        // KB-153: Ec(T) Arrhenius correction on Ec_ref at the current vat temperature.
        // Default Ea_cure = 30 kJ/mol (literature midpoint estimate) when the resin
        // TOML omits `cure_kinetics_ea_kj_mol`; callers rendering user output warn.
        let ea_cure_kj_mol = resin.effective_cure_kinetics_ea_kj_mol();
        let cure_depth = CureCalculator::cure_depth_at_temp(
            dp,
            energy,
            ec_ref,
            resin.reference_temp_c(),
            vat_temp,
            ea_cure_kj_mol,
        );

        if !cure_depth.is_sufficient(recipe.layer_height_um()) {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::InsufficientCure,
                severity: Severity::Critical,
                message: format!(
                    "Cure depth {:.1} µm < layer height {} µm",
                    cure_depth.value(),
                    recipe.layer_height_um()
                ),
            });
        }

        // --- LCD uniformity worst-case cure depth (KB-120) ---
        let worst_cure_depth = if printer.lcd_uniformity_variation > 0.0 {
            // Worst case = corner of build plate: factor = 1 - variation/2
            let corner_factor = 1.0 - printer.lcd_uniformity_variation / 2.0;
            let corner_energy = energy.scale(corner_factor);
            let cd = CureCalculator::cure_depth_at_temp(
                dp,
                corner_energy,
                ec_ref,
                resin.reference_temp_c(),
                vat_temp,
                ea_cure_kj_mol,
            );
            // Warn if center is sufficient but corner is not
            if cure_depth.is_sufficient(recipe.layer_height_um())
                && !cd.is_sufficient(recipe.layer_height_um())
            {
                failures.push(FailureEvent {
                    layer,
                    failure_type: FailureType::NonUniformCure,
                    severity: Severity::Warning,
                    message: format!(
                        "Center cure {:.1} µm OK but edge cure {:.1} µm < {} µm layer height ({:.0}% LCD variation)",
                        cure_depth.value(), cd.value(), recipe.layer_height_um(),
                        printer.lcd_uniformity_variation * 100.0,
                    ),
                });
            }
            cd.value()
        } else {
            cure_depth.value()
        };

        // --- Peel force ---
        let lift_speed = overrides
            .lift_speed_mm_min
            .unwrap_or(recipe.lift_speed_mm_min());
        let speed_factor =
            PeelForceCalculator::lift_speed_factor(lift_speed, resin.ref_lift_speed_mm_min);
        let peel = PeelForceCalculator::peel_force(resin.peel_adhesion_kpa, area, speed_factor);
        let suction_n = overrides.suction_force_n.unwrap_or(0.0);
        let suction = PeelForce::new(suction_n).expect(
            "suction_force_n from SuctionDetector is non-negative; default 0.0 is non-negative",
        );
        let total_force = PeelForceCalculator::total_force(peel, suction);

        // --- Holding capacity + safety assessment (plan v3 §6) ---
        let crate::services::SupportAssessment {
            overload,
            total_capacity,
            safety_factor,
            ..
        } = SupportAnalyzer::assess(layer, area, total_force, resin, supports, plate);
        if let Some(event) = overload {
            failures.push(event);
        }

        // --- Suction warning ---
        if suction_n > 1.0 {
            let severity = if suction_n > peel.value() * 2.0 {
                Severity::Critical
            } else {
                Severity::Warning
            };
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::SuctionCup,
                severity,
                message: format!(
                    "Sealed cavity suction {:.1} N (peel adhesion {:.1} N, total {:.1} N)",
                    suction_n,
                    peel.value(),
                    total_force.value()
                ),
            });
        }

        // --- Z-axis deflection ---
        let z_deflection =
            ZAxisCompensator::deflection_um(total_force, printer.z_stiffness_n_per_mm);
        let effective_height =
            ZAxisCompensator::effective_layer_height_um(recipe.layer_height_um(), z_deflection);

        if effective_height <= 0.0 {
            let (severity, note) = if overrides.is_raft {
                (Severity::Info, " (raft layer — does not affect model)")
            } else {
                (Severity::Critical, "")
            };
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::ZAxisCatastrophic,
                severity,
                message: format!(
                    "Z deflection {:.1} µm exceeds layer height {} µm{note}",
                    z_deflection,
                    recipe.layer_height_um()
                ),
            });
        }

        // --- Area delta ---
        let delta = area.value() - prev_area.value();
        if delta > AREA_DELTA_WARN_MM2 {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::RapidAreaIncrease,
                severity: Severity::Warning,
                message: format!(
                    "Area increased by {:.1} mm² (>{:.0} threshold)",
                    delta, AREA_DELTA_WARN_MM2
                ),
            });
        }

        let result = LayerResult {
            index: layer,
            cure_depth_um: cure_depth.value(),
            peel_force_n: peel.value(),
            suction_force_n: suction.value(),
            total_force_n: total_force.value(),
            support_capacity_n: total_capacity.value(),
            safety_factor: safety_factor.map_or(f32::INFINITY, |s| s.value()), // INFINITY = no load; print_simulation accumulator handles it correctly
            cross_section_area_mm2: area.value(),
            area_delta_mm2: delta,
            vat_temperature_c: vat_temp.value(),
            viscosity_mpa_s: viscosity,
            z_deflection_um: z_deflection,
            effective_layer_height_um: effective_height,
            worst_cure_depth_um: worst_cure_depth,
        };

        (result, failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_resin() -> ResinProfile {
        ResinProfile::generic_standard()
    }

    fn test_printer() -> PrinterProfile {
        PrinterProfile::generic_msla_4k()
    }

    fn test_recipe() -> Recipe {
        Recipe::generic_standard()
    }

    fn test_supports() -> SupportConfig {
        SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        }
    }

    fn test_plate() -> PlateAdhesionProfile {
        PlateAdhesionProfile::default_textured()
    }

    fn test_thermal() -> ThermalContext {
        ThermalContext {
            ambient: AmbientTemperature::new(22.0)
                .expect("test fixture: 22.0 °C is in AmbientTemperature domain"),
            initial_led_temp: None,
        }
    }

    fn area(mm2: f64) -> CrossSectionArea {
        CrossSectionArea::new(mm2)
            .expect("test fixture: non-negative finite mm² is in CrossSectionArea domain")
    }

    #[test]
    fn small_area_layer_no_failures() {
        let (result, failures) = FailurePredictor::predict_layer(
            50,
            area(100.0),
            area(95.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            failures.is_empty(),
            "expected no failures, got: {failures:?}"
        );
        assert!(result.safety_factor > 1.0);
        assert!(result.cure_depth_um > 50.0);
    }

    #[test]
    fn large_area_triggers_support_overload() {
        // At 8000 mm²: peel = 104 N
        // Support cap = 87.96 N, interlayer bond = 50 kPa × 8000 mm² = 400 N
        // Total = 487.96 N — actually holds with interlayer bond!
        // Need to use zero plate adhesion to isolate support failure
        let no_plate = PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0,
            bottom_layer_count: 0,
            interlayer_bond_kpa: 0.0,
        };
        let (_, failures) = FailurePredictor::predict_layer(
            100,
            area(8000.0),
            area(7900.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &no_plate,
            &test_thermal(),
        );
        assert!(
            failures
                .iter()
                .any(|f| f.failure_type == FailureType::SupportOverload),
            "expected SupportOverload, got: {failures:?}"
        );
    }

    #[test]
    fn rapid_area_increase_warns() {
        let (_, failures) = FailurePredictor::predict_layer(
            50,
            area(250.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            failures
                .iter()
                .any(|f| f.failure_type == FailureType::RapidAreaIncrease),
            "expected RapidAreaIncrease warning"
        );
    }

    #[test]
    fn bottom_layer_uses_bottom_exposure() {
        let (result, _) = FailurePredictor::predict_layer(
            2,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        let (normal_result, _) = FailurePredictor::predict_layer(
            50,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(result.cure_depth_um > normal_result.cure_depth_um * 4.0);
    }

    #[test]
    fn later_layers_have_higher_temperature() {
        let (early, _) = FailurePredictor::predict_layer(
            10,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        let (late, _) = FailurePredictor::predict_layer(
            500,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(late.vat_temperature_c > early.vat_temperature_c);
        assert!(late.viscosity_mpa_s < early.viscosity_mpa_s);
    }

    #[test]
    fn high_force_causes_z_catastrophic() {
        let mut printer = test_printer();
        printer.z_stiffness_n_per_mm = 460.0;
        let (_, failures) = FailurePredictor::predict_layer(
            100,
            area(10000.0),
            area(9900.0),
            &LayerOverrides::default(),
            &test_resin(),
            &printer,
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            failures
                .iter()
                .any(|f| f.failure_type == FailureType::ZAxisCatastrophic),
            "expected ZAxisCatastrophic, got: {failures:?}"
        );
    }

    #[test]
    fn no_supports_plate_holds_50mm_cube() {
        // 50mm cube: A=2500, peel=32.5N
        // No supports, but plate adhesion = 100 kPa × 2500 = 250 N (bottom)
        // Interlayer = 50 kPa × 2500 = 125 N (normal layers)
        // Both >> 32.5 N → no SupportOverload
        let no_supports = SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        };
        let (result, failures) = FailurePredictor::predict_layer(
            0,
            area(2500.0),
            area(0.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &no_supports,
            &test_plate(),
            &test_thermal(),
        );
        let overloads: Vec<_> = failures
            .iter()
            .filter(|f| f.failure_type == FailureType::SupportOverload)
            .collect();
        assert!(
            overloads.is_empty(),
            "plate adhesion should hold 50mm cube without supports, got: {overloads:?}"
        );
        assert!(result.safety_factor > 1.0);
    }

    #[test]
    fn no_supports_normal_layer_interlayer_holds() {
        let no_supports = SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        };
        let (result, failures) = FailurePredictor::predict_layer(
            50,
            area(2500.0),
            area(2500.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &no_supports,
            &test_plate(),
            &test_thermal(),
        );
        let overloads: Vec<_> = failures
            .iter()
            .filter(|f| f.failure_type == FailureType::SupportOverload)
            .collect();
        assert!(
            overloads.is_empty(),
            "interlayer bond should hold normal layers, got: {overloads:?}"
        );
        assert!(result.safety_factor > 1.0);
    }

    #[test]
    fn worst_cure_depth_less_than_center() {
        // With 22% uniformity variation, worst cure should be less than center
        let (result, _) = FailurePredictor::predict_layer(
            50,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &test_printer(),
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            result.worst_cure_depth_um < result.cure_depth_um,
            "worst {:.1} should be < center {:.1}",
            result.worst_cure_depth_um,
            result.cure_depth_um
        );
    }

    #[test]
    fn zero_uniformity_variation_no_difference() {
        let mut printer = test_printer();
        printer.lcd_uniformity_variation = 0.0;
        let (result, _) = FailurePredictor::predict_layer(
            50,
            area(100.0),
            area(100.0),
            &LayerOverrides::default(),
            &test_resin(),
            &printer,
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            (result.worst_cure_depth_um - result.cure_depth_um).abs() < 0.01,
            "with 0% variation, worst should equal center"
        );
    }

    #[test]
    fn high_uniformity_variation_triggers_non_uniform_cure() {
        // Use a resin/exposure combo where center barely passes but edge fails
        // Dp=170, Ec=5.0, I=4.0 mW/cm², exposure=2.5s → E=10 → Cd=117.8 (OK for 50µm)
        // At 50% variation: corner factor = 0.75 → E_corner=7.5 → Cd=69.1 (still OK)
        // Need tighter margin. Use low exposure override: E=6.0 → Cd=31.0 (below 50µm at center!)
        // Actually we need center OK but corner not. With variation=0.5:
        //   E=8.0 → Cd_center = 170*ln(8/5) = 79.8 (OK for 50µm)
        //   E_corner = 8.0*0.75 = 6.0 → Cd_corner = 170*ln(6/5) = 31.0 (FAIL for 50µm)
        let mut printer = test_printer();
        printer.lcd_uniformity_variation = 0.50; // extreme 50% variation
        let overrides = LayerOverrides {
            exposure_sec: Some(2.0), // 4 mW/cm² × 2.0s = 8.0 mJ/cm²
            ..Default::default()
        };
        let (result, failures) = FailurePredictor::predict_layer(
            50,
            area(100.0),
            area(100.0),
            &overrides,
            &test_resin(),
            &printer,
            &test_recipe(),
            &test_supports(),
            &test_plate(),
            &test_thermal(),
        );
        assert!(
            result.cure_depth_um > 50.0,
            "center should pass: {:.1}",
            result.cure_depth_um
        );
        assert!(
            result.worst_cure_depth_um < 50.0,
            "corner should fail: {:.1}",
            result.worst_cure_depth_um
        );
        assert!(
            failures
                .iter()
                .any(|f| f.failure_type == FailureType::NonUniformCure),
            "expected NonUniformCure, got: {failures:?}"
        );
    }
}
