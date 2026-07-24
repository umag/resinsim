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
    /// Applied KB-185 A/L peel shape factor (ADR-0022 Stage 3), dimensionless
    /// in `(0, 1]`. `None` when the resin opts out (`peel_shape_factor_strength`
    /// unset) — `predict_layer` then applies factor `1.0` (no correction). Set
    /// per-layer by `SimulationRunner` from the mask geometry + resin strength.
    pub peel_shape_factor: Option<f32>,
    /// Whether this layer is part of the raft/support base.
    /// Z deflection on raft layers is downgraded to Info (not Critical).
    pub is_raft: bool,
}

impl FailurePredictor {
    /// Simulate a single layer and return result + failures.
    ///
    /// `layer_height_um` is the runtime-authoritative slab thickness in
    /// micrometres. For CTB-based runs this is the value extracted from
    /// `LayerInput.layer_height_um` (file-axis, per ADR-0005 Consequences
    /// "Policy: CTB as file-axis authority"); for STL / area-only paths
    /// callers pass `recipe.layer_height_um()` as a fallback. Replaces six
    /// previous direct reads of `recipe.layer_height_um()` (cure-depth
    /// sufficiency, LCD-uniformity edge-cure check, effective-layer-height
    /// vs Z-deflection) — see ticket `ctb-layer-height-authority`.
    // The 11 arguments each name a distinct physical input; bundling them
    // into a `PredictLayerInputs` struct would move the collection point
    // one frame up without reducing the caller's parameter count.
    // Documented as accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn predict_layer(
        layer: u32,
        area: CrossSectionArea,
        prev_area: CrossSectionArea,
        overrides: &LayerOverrides,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        recipe: &Recipe,
        layer_height_um: f32,
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

        if !cure_depth.is_sufficient(layer_height_um) {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::InsufficientCure,
                severity: Severity::Critical,
                message: format!(
                    "Cure depth {:.1} µm < layer height {} µm",
                    cure_depth.value(),
                    layer_height_um
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
            if cure_depth.is_sufficient(layer_height_um) && !cd.is_sufficient(layer_height_um) {
                failures.push(FailureEvent {
                    layer,
                    failure_type: FailureType::NonUniformCure,
                    severity: Severity::Warning,
                    message: format!(
                        "Center cure {:.1} µm OK but edge cure {:.1} µm < {} µm layer height ({:.0}% LCD variation)",
                        cure_depth.value(), cd.value(), layer_height_um,
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
        // KB-185 Tier-1 aspect-ratio shape factor (ADR-0022 Stage 3): modulate
        // the peel term ONLY (not suction/base). 1.0 (no correction) unless the
        // runner supplied a per-layer factor from the mask + resin strength.
        let shape_factor = overrides.peel_shape_factor.unwrap_or(1.0);
        let peel = PeelForce::new(peel.value() * shape_factor)
            .expect("peel force × shape factor in (0, 1] is non-negative finite");
        let suction_n = overrides.suction_force_n.unwrap_or(0.0);
        let suction = PeelForce::new(suction_n).expect(
            "suction_force_n from SuctionDetector is non-negative; default 0.0 is non-negative",
        );
        // KB-116 first-layer base adhesion: elevated release-layer σ at layer 0
        // relaxing over the bottom-layer count (ADR-0022 Stage 1). Opt-in per
        // resin; 0.0 elevation ⇒ zero base, so legacy resins are unchanged.
        let base = PeelForceCalculator::base_adhesion_force(
            resin.effective_base_adhesion_elevation_kpa(),
            area,
            layer,
            recipe.bottom_layer_count() as f32,
        );
        let total_force = PeelForceCalculator::total_force(peel, suction, base);

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
            ZAxisCompensator::effective_layer_height_um(layer_height_um, z_deflection);

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
                    z_deflection, layer_height_um
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
            base_force_n: base.value(),
            peel_shape_factor: overrides.peel_shape_factor,
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
            // ADR-0018 / t2f3 — Tier-1 path never populates these; the
            // SimulationRunner voxel pass overwrites them when active.
            strain_magnitude_max: None,
            stress_von_mises_max_mpa: None,
            strain_gradient_max_frac: None,
            voxel_yield_fraction: None,
        };

        (result, failures)
    }

    /// Predict strain- and stress-driven failures for one layer (t2f3 /
    /// ADR-0018). Co-exists with [`predict_layer`] — the SimulationRunner
    /// invokes both per layer and merges their failure vectors before
    /// `sim.add_layer(...)`.
    ///
    /// Detection rules (KB-162 / KB-161):
    /// - `WarpingRisk` — emitted when the layer's per-voxel **yield
    ///   fraction** exceeds [`YIELD_FRACTION_WARN_THRESHOLD`] (warning)
    ///   or [`YIELD_FRACTION_CRIT_THRESHOLD`] (critical). The yield
    ///   fraction is the share of cured voxels in the slab whose
    ///   von Mises stress exceeds `resin.tensile_strength_mpa()` —
    ///   i.e. the share that have crossed the multi-axial yield
    ///   surface (KB-162). This is the physically correct yield
    ///   criterion: tensile_strength IS the uniaxial yield stress; von
    ///   Mises generalises uniaxial yield to multi-axial. No arbitrary
    ///   safety factor.
    /// - `CohesiveFailure` — emitted when the layer's maximum interior
    ///   strain gradient `|∇ε|` exceeds [`GRADIENT_THRESHOLD_FRAC`]
    ///   (warning). Cured-vs-empty pairs (part-surface boundaries)
    ///   are filtered out per the post-t2f3 calibration finding.
    ///
    /// When the resin's mechanical moduli are uncalibrated
    /// (`resin.has_calibrated_moduli() == false`, i.e. any of the
    /// three moduli — E, ν, z_ratio — is `None`), the
    /// `FailureEvent.message` is suffixed with a
    /// "(uncalibrated moduli — magnitude has ±50% uncertainty,
    /// see KB-163 + KB-164)" caveat so users can distinguish
    /// calibration-artefact emissions from real physics. KB-163
    /// covers the E + ν defaults; KB-164 covers the z_ratio
    /// anisotropy whose ±0.3 uncertainty is a material σ_vm
    /// magnitude driver post-anisotropy redesign.
    ///
    /// **Model-gap caveat (ADR-0018 §9, KB-162):** the per-voxel σ_vm
    /// value used here reflects free-shrinkage stress only — it does
    /// NOT include the cumulative residual stress that builds up as
    /// later layers cure against already-cured layers below. Real
    /// MSLA prints warp because of the latter. With the v1 free-
    /// shrinkage model, `voxel_yield_fraction` reads 0 on most prints
    /// even though the threshold (tensile_strength_mpa) is physically
    /// correct. The fraction becomes a useful early-warning signal
    /// once the Tier-3 compatibility-violation residual stress model
    /// lands.
    #[cfg(feature = "field-sim")]
    pub fn predict_strain_failures(
        layer: u32,
        strain_field: &crate::values::StrainField,
        stress_field: &crate::values::StressField,
        resin: &ResinProfile,
    ) -> Vec<FailureEvent> {
        let mut failures: Vec<FailureEvent> = Vec::new();

        let calibration_caveat: &str = if resin.has_calibrated_moduli() {
            ""
        } else {
            " (uncalibrated moduli — magnitude has ±50% uncertainty, see KB-163 + KB-164)"
        };

        // WarpingRisk — per-voxel yield-fraction signal ----------------
        let tensile_mpa = resin.tensile_strength_mpa();
        let yield_fraction = match stress_field.yield_fraction(layer, tensile_mpa) {
            Ok(v) => v,
            // Out-of-bounds layer index — typically programmer error;
            // surface as no detection rather than panic to keep the
            // strain pass advisory rather than fatal.
            Err(_) => return failures,
        };
        if yield_fraction >= YIELD_FRACTION_CRIT_THRESHOLD {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::WarpingRisk,
                severity: Severity::Critical,
                message: format!(
                    "Layer yield fraction {yield_pct:.1}% exceeds {crit_pct:.0}% \
                     critical threshold against tensile strength {tensile_mpa:.1} MPa \
                     — large share of cured voxels predicted to yield{calibration_caveat}",
                    yield_pct = 100.0 * yield_fraction,
                    crit_pct = 100.0 * YIELD_FRACTION_CRIT_THRESHOLD,
                ),
            });
        } else if yield_fraction >= YIELD_FRACTION_WARN_THRESHOLD {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::WarpingRisk,
                severity: Severity::Warning,
                message: format!(
                    "Layer yield fraction {yield_pct:.2}% exceeds {warn_pct:.1}% \
                     warning threshold against tensile strength {tensile_mpa:.1} MPa \
                     — local hotspots predicted to yield{calibration_caveat}",
                    yield_pct = 100.0 * yield_fraction,
                    warn_pct = 100.0 * YIELD_FRACTION_WARN_THRESHOLD,
                ),
            });
        }

        // Strain-gradient CohesiveFailure ------------------------------
        let grad_max = match strain_field.gradient_layer_max(layer) {
            Ok(v) => v,
            Err(_) => return failures,
        };
        if grad_max >= GRADIENT_THRESHOLD_FRAC {
            failures.push(FailureEvent {
                layer,
                failure_type: FailureType::CohesiveFailure,
                severity: Severity::Warning,
                message: format!(
                    "Layer strain gradient {grad_max:.4} exceeds threshold \
                     {GRADIENT_THRESHOLD_FRAC:.4} — micro-crack risk along the \
                     gradient direction{calibration_caveat}",
                ),
            });
        }

        failures
    }
}

/// Strain-gradient threshold for `CohesiveFailure` emission (KB-161).
/// Dimensionless `|∇ε|` per-voxel-pair. Literature midpoint: 0.005
/// (0.5% strain step between adjacent voxels) — calibrate via Athena II
/// in a follow-on issue.
#[cfg(feature = "field-sim")]
pub const GRADIENT_THRESHOLD_FRAC: f32 = 0.005;

/// Yield fraction at which `WarpingRisk` emits at `Severity::Warning`.
/// 0.1% of cured voxels yielded — even a small share of yielded voxels
/// inside a layer is worth surfacing because it signals a hotspot the
/// printer is approaching the tensile limit somewhere. (KB-162.)
#[cfg(feature = "field-sim")]
pub const YIELD_FRACTION_WARN_THRESHOLD: f32 = 0.001;

/// Yield fraction at which `WarpingRisk` emits at `Severity::Critical`.
/// 5% of cured voxels yielded — a substantial fraction of the layer is
/// past the multi-axial yield surface; the part is expected to deform
/// plastically or crack at this layer. (KB-162.)
#[cfg(feature = "field-sim")]
pub const YIELD_FRACTION_CRIT_THRESHOLD: f32 = 0.05;

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
            test_recipe().layer_height_um(),
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

    /// ADR-0022 Stage 3: the peel shape factor scales the PEEL term ONLY —
    /// suction and base forces are untouched (KB-185 Tier-1 modulates σ_peel).
    #[test]
    fn peel_shape_factor_scales_peel_only() {
        let unshaped = LayerOverrides {
            suction_force_n: Some(2.0),
            ..Default::default()
        };
        let shaped = LayerOverrides {
            suction_force_n: Some(2.0),
            peel_shape_factor: Some(0.5),
            ..Default::default()
        };
        // Layer 0 so any base-adhesion term is at full strength; it is identical
        // across both runs since it does not depend on the shape factor.
        let run = |ov: &LayerOverrides| {
            FailurePredictor::predict_layer(
                0,
                area(500.0),
                area(500.0),
                ov,
                &test_resin(),
                &test_printer(),
                &test_recipe(),
                test_recipe().layer_height_um(),
                &test_supports(),
                &test_plate(),
                &test_thermal(),
            )
            .0
        };
        let base_r = run(&unshaped);
        let shaped_r = run(&shaped);

        assert!(base_r.peel_force_n > 0.0, "need a non-zero peel to scale");
        assert!(
            (shaped_r.peel_force_n - 0.5 * base_r.peel_force_n).abs() < 1e-4,
            "peel should halve: base={} shaped={}",
            base_r.peel_force_n,
            shaped_r.peel_force_n
        );
        // Suction + base are untouched by the peel shape factor.
        assert_eq!(shaped_r.suction_force_n, base_r.suction_force_n);
        assert_eq!(shaped_r.base_force_n, base_r.base_force_n);
        // The total drops by exactly the peel reduction — nothing else moved.
        let total_delta = base_r.total_force_n - shaped_r.total_force_n;
        let peel_delta = base_r.peel_force_n - shaped_r.peel_force_n;
        assert!(
            (total_delta - peel_delta).abs() < 1e-4,
            "total delta {total_delta} should equal peel delta {peel_delta}"
        );
        // Observability: the field records exactly what was applied.
        assert_eq!(base_r.peel_shape_factor, None);
        assert_eq!(shaped_r.peel_shape_factor, Some(0.5));
    }

    /// ADR-0022 Stage 1 MECHANISM gate: the KB-116 base term, folded into
    /// total_force, pulls the total-force peak EARLIER than the area-driven
    /// peel peak. Decoupled from whether the real spike geometry cooperates —
    /// here the first layer is given a large area so an area-scaled base term
    /// can win. Proves the wiring works in CI.
    #[test]
    fn base_adhesion_shifts_total_force_peak_earlier() {
        use crate::services::argmax_by;
        // Elevated first-layer adhesion Δσ₀ ≈ 2× the steady peel σ — plausible
        // for the fresh, oxygen-inhibited release layer.
        let mut resin = test_resin();
        resin.base_adhesion_elevation_kpa = Some(25.0);
        let recipe = test_recipe();
        let printer = test_printer();
        let supports = test_supports();
        let plate = test_plate();
        let thermal = test_thermal();
        let height = recipe.layer_height_um();
        // Areas so the area-driven peel peaks at a LATER layer (index 3, max
        // area) while the large first layer lets base pull the TOTAL peak to 0.
        let areas = [450.0, 200.0, 300.0, 500.0, 250.0];
        let mut peel = Vec::new();
        let mut total = Vec::new();
        let mut prev = 0.0f64;
        for (layer, &a) in areas.iter().enumerate() {
            let (lr, _) = FailurePredictor::predict_layer(
                layer as u32,
                area(a),
                area(prev),
                &LayerOverrides::default(),
                &resin,
                &printer,
                &recipe,
                height,
                &supports,
                &plate,
                &thermal,
            );
            if layer == 0 {
                assert!(
                    lr.base_force_n > 0.0,
                    "base at layer 0 must be positive, got {}",
                    lr.base_force_n
                );
            }
            peel.push(lr.peel_force_n);
            total.push(lr.total_force_n);
            prev = a;
        }
        let peel_peak = argmax_by(&peel, |&v| v as f64).expect("non-empty");
        let total_peak = argmax_by(&total, |&v| v as f64).expect("non-empty");
        assert!(
            peel_peak > 0,
            "sanity: area-driven peel should peak at a later layer, got {peel_peak}"
        );
        assert!(
            total_peak < peel_peak,
            "base term must pull the total-force peak earlier: total_peak={total_peak}, peel_peak={peel_peak}"
        );
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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
            test_recipe().layer_height_um(),
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

    // --- predict_strain_failures (t2f3 / ADR-0018) -------------------

    #[cfg(feature = "field-sim")]
    mod predict_strain_failures {
        use super::*;
        use crate::values::{StrainField, StrainTensor, StressField, StressTensor};

        fn fields_2x2x1() -> (StrainField, StressField) {
            (
                StrainField::new(2, 2, 1, 0.5, [0.0; 3]).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions"),
                StressField::new(2, 2, 1, 0.5, [0.0; 3]).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions"),
            )
        }

        #[test]
        fn no_failures_for_zero_fields() {
            let (strain, stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            assert!(
                f.is_empty(),
                "zero fields must produce no failures, got {f:?}"
            );
        }

        #[test]
        fn warping_warning_at_small_yield_fraction() {
            // 2×2 = 4 cured voxels; ONE yielded → fraction = 0.25 > 0.05
            // critical threshold → emits Critical. To test Warning, use
            // a larger grid where fraction lands in [0.001, 0.05).
            let mut strain = StrainField::new(40, 40, 1, 0.5, [0.0; 3]).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let mut stress = StressField::new(40, 40, 1, 0.5, [0.0; 3]).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let resin = ResinProfile::generic_standard();
            // Fill all 1600 voxels with sub-tensile stress (cured but
            // not yielded), then mark 5 of them as yielded
            // → 5/1600 = 0.3% > 0.1% warn but < 5% crit.
            let sub = StressTensor::new(5.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let yielded =
                StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            // tensile = 35, so vm=50 > 35 yields, vm=5 < 35 doesn't.
            let placeholder = StrainTensor::from_isotropic(-0.001).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..40 {
                for iy in 0..40 {
                    strain.lock_strain_at(ix, iy, 0, placeholder).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                    stress.accumulate_at(ix, iy, 0, sub).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            for k in 0..5 {
                stress.accumulate_at(k, 0, 0, yielded).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            }
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            // Expect a single WarpingRisk emission at Warning severity.
            // (CohesiveFailure may also emit because the small/large
            // strain difference is zero — all cells are placeholder, so
            // grad_max == 0; thus single emission.)
            let warpings: Vec<_> = f
                .iter()
                .filter(|e| e.failure_type == FailureType::WarpingRisk)
                .collect();
            assert_eq!(
                warpings.len(),
                1,
                "expected exactly one WarpingRisk; got {f:?}"
            );
            assert_eq!(warpings[0].severity, Severity::Warning);
        }

        #[test]
        fn warping_critical_at_high_yield_fraction() {
            // 4 cured voxels, 1 yielded → fraction = 0.25 > 0.05 → Critical.
            let (mut strain, mut stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let sub = StressTensor::new(5.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let placeholder = StrainTensor::from_isotropic(-0.001).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..2 {
                for iy in 0..2 {
                    strain.lock_strain_at(ix, iy, 0, placeholder).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                    stress.accumulate_at(ix, iy, 0, sub).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            stress.accumulate_at(0, 0, 0, yielded).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            let warpings: Vec<_> = f
                .iter()
                .filter(|e| e.failure_type == FailureType::WarpingRisk)
                .collect();
            assert_eq!(warpings.len(), 1);
            assert_eq!(warpings[0].severity, Severity::Critical);
        }

        #[test]
        fn no_warping_when_no_voxel_yields() {
            // All 4 voxels well below tensile (35 MPa) → no emission.
            let (mut strain, mut stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let sub = StressTensor::new(10.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let placeholder = StrainTensor::from_isotropic(-0.001).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..2 {
                for iy in 0..2 {
                    strain.lock_strain_at(ix, iy, 0, placeholder).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                    stress.accumulate_at(ix, iy, 0, sub).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            assert!(
                !f.iter().any(|e| e.failure_type == FailureType::WarpingRisk),
                "no voxel yielded → no WarpingRisk; got {f:?}"
            );
        }

        #[test]
        fn cohesive_failure_for_high_interior_gradient() {
            // Post-t2f3-calibration: gradient_layer_max now skips
            // cured-vs-empty pairs (part-surface noise). Test the
            // intended signal: two ADJACENT CURED voxels with very
            // different magnitudes (thick-thin interior step). The
            // Frobenius diff between isotropic ε = -0.005 and -0.02 is
            // |Δε|·√3 = 0.015·√3 ≈ 0.026 > GRADIENT_THRESHOLD_FRAC.
            let (mut strain, stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let small = StrainTensor::from_isotropic(-0.005).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let large = StrainTensor::from_isotropic(-0.02).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            strain.lock_strain_at(0, 0, 0, small).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            strain.lock_strain_at(1, 0, 0, large).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            assert!(
                f.iter()
                    .any(|e| e.failure_type == FailureType::CohesiveFailure),
                "interior strain step must surface CohesiveFailure: {f:?}"
            );
        }

        #[test]
        fn no_cohesive_failure_for_cured_vs_empty_only() {
            // Single cured voxel surrounded by empty — used to trip the
            // detector before t2f3.1 because the surface boundary
            // produced a Frobenius diff of L·√3. New filter rejects
            // these pairs entirely.
            let (mut strain, stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let t = StrainTensor::from_isotropic(-0.02).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            strain.lock_strain_at(0, 0, 0, t).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            assert!(
                !f.iter()
                    .any(|e| e.failure_type == FailureType::CohesiveFailure),
                "cured-vs-empty must NOT emit CohesiveFailure: {f:?}"
            );
        }

        #[test]
        fn no_cohesive_failure_for_uniform_layer() {
            let (mut strain, stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            let t = StrainTensor::from_isotropic(-0.005).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..2 {
                for iy in 0..2 {
                    strain.lock_strain_at(ix, iy, 0, t).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            assert!(!f
                .iter()
                .any(|e| e.failure_type == FailureType::CohesiveFailure));
        }

        #[test]
        fn message_discloses_uncalibrated_moduli_for_unset_resin() {
            // elegoo_ceramic_grey_v2 deliberately omits E, ν, AND z_ratio
            // (t2f3.1 widened the predicate from 2-of-2 to 3-of-3).
            let (mut strain, mut stress) = fields_2x2x1();
            let resin = ResinProfile::elegoo_ceramic_grey_v2();
            assert!(!resin.has_calibrated_moduli());
            // tensile_strength_mpa = 38 → vm = 50 yields.
            let sub = StressTensor::new(5.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let placeholder = StrainTensor::from_isotropic(-0.001).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..2 {
                for iy in 0..2 {
                    strain.lock_strain_at(ix, iy, 0, placeholder).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                    stress.accumulate_at(ix, iy, 0, sub).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            stress.accumulate_at(0, 0, 0, yielded).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            let warping = f
                .iter()
                .find(|e| e.failure_type == FailureType::WarpingRisk)
                .expect("expected WarpingRisk emission");
            assert!(
                warping.message.contains("uncalibrated moduli"),
                "message must disclose uncalibrated moduli; got: {}",
                warping.message
            );
            // t2f3.1 A2: caveat cites BOTH KB-163 (E + ν defaults) and
            // KB-164 (z_ratio anisotropy ±0.3 band). Lock both substrings
            // so a future single-cite regression fails this test.
            assert!(
                warping.message.contains("KB-163"),
                "caveat must cite KB-163 (E + ν defaults); got: {}",
                warping.message
            );
            assert!(
                warping.message.contains("KB-164"),
                "caveat must cite KB-164 (z_ratio anisotropy ±0.3 band); got: {}",
                warping.message
            );
        }

        #[test]
        fn message_no_caveat_for_calibrated_resin() {
            // generic_standard ships with explicit E + ν.
            let (mut strain, mut stress) = fields_2x2x1();
            let resin = ResinProfile::generic_standard();
            assert!(resin.has_calibrated_moduli());
            let sub = StressTensor::new(5.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let yielded = StressTensor::new(50.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let placeholder = StrainTensor::from_isotropic(-0.001).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            for ix in 0..2 {
                for iy in 0..2 {
                    strain.lock_strain_at(ix, iy, 0, placeholder).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                    stress.accumulate_at(ix, iy, 0, sub).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
                }
            }
            stress.accumulate_at(0, 0, 0, yielded).expect("test fixture: literal stress/strain components and in-bounds index satisfy field preconditions");
            let f = FailurePredictor::predict_strain_failures(0, &strain, &stress, &resin);
            let warping = f
                .iter()
                .find(|e| e.failure_type == FailureType::WarpingRisk)
                .expect("expected WarpingRisk emission");
            assert!(
                !warping.message.contains("uncalibrated moduli"),
                "calibrated resin must NOT show caveat; got: {}",
                warping.message
            );
        }
    }
}
