//! Shared cucumber World used by every UAT scenario binding.
//!
//! Step-def modules under `tests/uat_steps/` share one World type so
//! cucumber can run every scenario through the same
//! `UatWorld::cucumber()` builder. Builder helpers (`PrinterBuilder`,
//! `ResinBuilder`, etc.) land in step 7; for now the struct carries
//! raw domain types + scenario-specific capture fields.

use cucumber::World;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{PeelForce, SafetyFactor, SupportCapacity};

#[derive(Debug, Default, World)]
pub struct UatWorld {
    // ---- Safety-factor-zero-force scenarios ----
    /// Unused by current step defs (step 9 moved to predict_layer
    /// integration which populates `predict_layer_result`), retained
    /// for future component-level scenarios.
    #[expect(dead_code, reason = "pre-step-9 spike mirror; kept for future scenarios")]
    pub capacity: Option<SupportCapacity>,
    #[expect(dead_code, reason = "pre-step-9 spike mirror; kept for future scenarios")]
    pub force: Option<PeelForce>,
    #[expect(dead_code, reason = "pre-step-9 spike mirror; kept for future scenarios")]
    pub computed_safety: Option<Option<SafetyFactor>>,
    /// Step-9 predict_layer output. Per-scenario (cucumber resets
    /// World between scenarios) so the capture doesn't leak across
    /// runs. Folds review finding #3 (OnceLock → World field).
    pub predict_layer_result: Option<(
        resinsim_core::entities::LayerResult,
        Vec<resinsim_core::entities::FailureEvent>,
    )>,

    // ---- Cure-depth-NaN-guard scenarios ----
    pub last_energy_err: Option<&'static str>,
    pub last_panic_msg: Option<String>,

    // ---- Recipe + pairing scenarios (recipe-outside, recipe-inside,
    // resin-switch, thermal-degradation) ----
    pub printer: Option<PrinterProfile>,
    pub resin: Option<ResinProfile>,
    pub resin_alt: Option<ResinProfile>,
    pub last_sim_err: Option<String>,
    pub sim_primary: Option<PrintSimulation>,
    pub sim_alt: Option<PrintSimulation>,
    pub pairing_result: Option<Result<(), Vec<String>>>,

    // ---- TOML parse + validate scenarios (legacy-*) ----
    pub toml_text: Option<String>,
    pub parse_result: Option<Result<(), String>>,
    pub validate_result: Option<Result<(), String>>,

    // ---- Thermal scenarios ----
    pub last_vat_temp_c: Option<f32>,
    pub thermal_degradation_flagged: Option<bool>,

    // ---- Suction-detector scenarios ----
    pub cavity_events: Option<Vec<CavityEventSummary>>,
    /// Not asserted directly — captured for post-hoc diagnostics only.
    #[expect(dead_code, reason = "diagnostic capture; not yet asserted")]
    pub suction_failure_count: Option<usize>,
    pub suction_event_layer: Option<u32>,
    pub sealed_area_mm2: Option<f32>,
    pub suction_force_n: Option<f32>,

    // ---- CLI subprocess scenarios ----
    pub cli_cmd: Option<Vec<String>>,
    pub cli_env: Option<Vec<(String, String)>>,
    pub cli_exit_code: Option<i32>,
    pub cli_stdout: Option<String>,
    pub cli_stderr: Option<String>,
}

/// Summary of a single `CavityDetector` event for step-def assertions.
/// Keeps the World independent of any internal detector types.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CavityEventSummary {
    pub layer: u32,
    pub area_mm2: f32,
    pub force_n: f32,
}

// ---- Typed builders (plan step 7) -----------------------------------------
//
// Replace the ad-hoc `fixtures::printer_with_ranges` / direct factory
// calls with builder APIs that the step defs can re-use. Defaults track
// `PrinterProfile::generic_msla_4k()` / `ResinProfile::generic_standard()`
// so builder output matches the hand-written tests' canonical fixtures.

/// Builder for a `PrinterProfile` via TOML round-trip.
///
/// Pub(crate) fields on `PrinterProfile` prevent direct construction from
/// integration tests, so the builder assembles a TOML document and
/// deserialises it. Defaults mirror `generic_msla_4k()` (20..100 µm layer
/// height range, 1..60 s exposure range, 460 N/mm stiffness, etc.).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PrinterBuilder {
    name: String,
    layer_min: f32,
    layer_max: f32,
    exposure_min: f32,
    exposure_max: f32,
    lift_speed_min: f32,
    lift_speed_max: f32,
    z_stiffness_n_per_mm: f32,
    led_power_mw_cm2: f32,
}

#[allow(dead_code)]
impl PrinterBuilder {
    /// Defaults track `PrinterProfile::generic_msla_4k()` — the same
    /// factory the hand-written tests/cure_properties.rs, tests/force_properties.rs,
    /// and tests/layer_timing_properties.rs fixtures depend on.
    pub fn new() -> Self {
        Self {
            name: "UatPrinter".into(),
            layer_min: 20.0,
            layer_max: 100.0,
            exposure_min: 1.0,
            exposure_max: 60.0,
            lift_speed_min: 10.0,
            lift_speed_max: 200.0,
            z_stiffness_n_per_mm: 460.0, // KB-130 generic_msla_4k default
            led_power_mw_cm2: 4.0,
        }
    }

    pub fn with_layer_height_range(mut self, min: f32, max: f32) -> Self {
        self.layer_min = min;
        self.layer_max = max;
        self
    }

    pub fn with_exposure_range(mut self, min: f32, max: f32) -> Self {
        self.exposure_min = min;
        self.exposure_max = max;
        self
    }

    pub fn with_z_stiffness(mut self, n_per_mm: f32) -> Self {
        self.z_stiffness_n_per_mm = n_per_mm;
        self
    }

    pub fn build(self) -> resinsim_core::entities::PrinterProfile {
        let toml_str = format!(
            r#"
name = "{name}"
led_power_mw_cm2 = {led}
pixel_pitch_um = 50.0
layer_height_range_um = {{ min = {layer_min}, max = {layer_max} }}
exposure_range_sec = {{ min = {exp_min}, max = {exp_max} }}
lift_speed_range_mm_min = {{ min = {lift_min}, max = {lift_max} }}
bottom_layer_count_max = 15
z_stiffness_n_per_mm = {stiff}
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
"#,
            name = self.name,
            led = self.led_power_mw_cm2,
            layer_min = self.layer_min,
            layer_max = self.layer_max,
            exp_min = self.exposure_min,
            exp_max = self.exposure_max,
            lift_min = self.lift_speed_min,
            lift_max = self.lift_speed_max,
            stiff = self.z_stiffness_n_per_mm,
        );
        let p: resinsim_core::entities::PrinterProfile = toml::from_str(&toml_str)
            .expect("PrinterBuilder TOML must parse");
        p.validate().expect("PrinterBuilder output must satisfy validate()");
        p
    }
}

impl Default for PrinterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for a `ResinProfile` via TOML round-trip. Defaults track
/// `ResinProfile::generic_standard()` — the same chemistry that
/// tests/cure_properties.rs uses (Ec=5.0 mJ/cm², Dp=170 µm).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResinBuilder {
    name: String,
    critical_energy_mj_cm2: f32,
    penetration_depth_um: f32,
    viscosity_mpa_s: f32,
    tensile_strength_mpa: f32,
    peel_adhesion_kpa: f32,
    ref_lift_speed_mm_min: f32,
    reference_temp_c: f32,
    activation_energy_kj_mol: f32,
    density_g_cm3: f32,
    linear_shrinkage_pct: f32,
    degradation_temp_c: Option<f32>,
    min_safe_temp_c: Option<f32>,
    recipe: RecipeBuilder,
}

#[allow(dead_code)]
impl ResinBuilder {
    pub fn new() -> Self {
        Self {
            name: "UatResin".into(),
            critical_energy_mj_cm2: 5.0, // KB-100 Premium Black
            penetration_depth_um: 170.0, // KB-100 Premium Black
            viscosity_mpa_s: 200.0,      // KB-141 typical
            tensile_strength_mpa: 35.0,  // KB-140 conservative
            peel_adhesion_kpa: 13.0,     // KB-110 standard FEP
            ref_lift_speed_mm_min: 60.0,
            reference_temp_c: 25.0,
            activation_energy_kj_mol: 52.0, // KB-150
            density_g_cm3: 1.1,
            linear_shrinkage_pct: 1.5,
            degradation_temp_c: None,
            min_safe_temp_c: None,
            recipe: RecipeBuilder::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_critical_energy(mut self, mj_cm2: f32) -> Self {
        self.critical_energy_mj_cm2 = mj_cm2;
        self
    }

    pub fn with_penetration_depth(mut self, um: f32) -> Self {
        self.penetration_depth_um = um;
        self
    }

    pub fn with_viscosity(mut self, mpa_s: f32) -> Self {
        self.viscosity_mpa_s = mpa_s;
        self
    }

    pub fn with_peel_adhesion(mut self, kpa: f32) -> Self {
        self.peel_adhesion_kpa = kpa;
        self
    }

    pub fn with_thermal_thresholds(mut self, degradation_c: f32, min_safe_c: f32) -> Self {
        self.degradation_temp_c = Some(degradation_c);
        self.min_safe_temp_c = Some(min_safe_c);
        self
    }

    pub fn with_recipe(mut self, recipe: RecipeBuilder) -> Self {
        self.recipe = recipe;
        self
    }

    pub fn build(self) -> resinsim_core::entities::ResinProfile {
        let thermal_lines = match (self.degradation_temp_c, self.min_safe_temp_c) {
            (Some(d), Some(m)) => format!("degradation_temp_c = {d}\nmin_safe_temp_c = {m}\n"),
            _ => String::new(),
        };
        let toml_str = format!(
            r#"name = "{name}"
penetration_depth_um = {dp}
critical_energy_mj_cm2 = {ec}
tensile_strength_mpa = {ts}
peel_adhesion_kpa = {pa}
ref_lift_speed_mm_min = {rls}
linear_shrinkage_pct = {lsp}
viscosity_mpa_s = {visc}
reference_temp_c = {ref_t}
activation_energy_kj_mol = {ea}
density_g_cm3 = {dens}
{thermal_lines}
{recipe}
"#,
            name = self.name,
            dp = self.penetration_depth_um,
            ec = self.critical_energy_mj_cm2,
            ts = self.tensile_strength_mpa,
            pa = self.peel_adhesion_kpa,
            rls = self.ref_lift_speed_mm_min,
            lsp = self.linear_shrinkage_pct,
            visc = self.viscosity_mpa_s,
            ref_t = self.reference_temp_c,
            ea = self.activation_energy_kj_mol,
            dens = self.density_g_cm3,
            recipe = self.recipe.to_toml(),
        );
        let r: resinsim_core::entities::ResinProfile =
            toml::from_str(&toml_str).expect("ResinBuilder TOML must parse");
        r.validate().expect("ResinBuilder output must satisfy validate()");
        r
    }
}

impl Default for ResinBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for a `Recipe` table — used standalone (via
/// `build_standalone()` which unwraps from a temporary ResinProfile) or
/// nested inside `ResinBuilder`.
///
/// Defaults track `data/resins/generic_standard.toml`'s recipe block
/// (layer_height_um=50, normal_exposure=2.5, bottom_exposure=25,
/// bottom_layer_count=6, transition_layers=3, lift_speed=60, ...).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RecipeBuilder {
    layer_height_um: f32,
    normal_exposure_sec: f32,
    lift_speed_mm_min: f32,
}

#[allow(dead_code)]
impl RecipeBuilder {
    pub fn new() -> Self {
        Self {
            layer_height_um: 50.0,
            normal_exposure_sec: 2.5,
            lift_speed_mm_min: 60.0,
        }
    }

    pub fn with_layer_height(mut self, um: f32) -> Self {
        self.layer_height_um = um;
        self
    }

    pub fn with_normal_exposure(mut self, sec: f32) -> Self {
        self.normal_exposure_sec = sec;
        self
    }

    pub fn with_lift_speed(mut self, mm_min: f32) -> Self {
        self.lift_speed_mm_min = mm_min;
        self
    }

    pub(crate) fn to_toml(&self) -> String {
        format!(
            r#"[recipe]
layer_height_um = {layer}
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = {exp}
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = {lift}
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#,
            layer = self.layer_height_um,
            exp = self.normal_exposure_sec,
            lift = self.lift_speed_mm_min,
        )
    }

    /// Extract the built `Recipe` by round-tripping through a minimal
    /// `ResinProfile` (since `Recipe::new` is `pub(crate)`). The
    /// scenario's assertions go through the resin, so callers typically
    /// use `ResinBuilder::with_recipe(..)` directly.
    pub fn build_standalone(self) -> resinsim_core::entities::Recipe {
        let resin = ResinBuilder::new().with_recipe(self).build();
        resin.recipe().clone()
    }
}

impl Default for RecipeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Bundle of the 10 arguments `FailurePredictor::predict_layer` consumes.
/// Use `default_for_test()` to get a valid invocation for the safety-
/// factor + cure-depth UAT scenarios; mutate the struct fields
/// before invoking for targeted probes.
///
/// This is the PredictLayerInputs helper the plan step 7 prescribes —
/// existing step-9 rollout of predict_layer integration uses it to
/// replace the spike's tautology mirror at safety-factor-zero-force.
#[allow(dead_code)]
pub struct PredictLayerInputs {
    pub layer: u32,
    pub area: resinsim_core::values::CrossSectionArea,
    pub prev_area: resinsim_core::values::CrossSectionArea,
    pub overrides: resinsim_core::services::failure_predictor::LayerOverrides,
    pub resin: resinsim_core::entities::ResinProfile,
    pub printer: resinsim_core::entities::PrinterProfile,
    pub supports: resinsim_core::services::failure_predictor::SupportConfig,
    pub plate: resinsim_core::services::build_plate::PlateAdhesionProfile,
    pub thermal: resinsim_core::services::failure_predictor::ThermalContext,
}

#[allow(dead_code)]
impl PredictLayerInputs {
    /// Defaults track the hand-written test fixtures at
    /// src/app/simulation_runner.rs::tests::default_plate / test_ambient /
    /// cube_areas. Paired with `PrinterBuilder::new().build()` +
    /// `ResinBuilder::new().build()` which share values with
    /// `PrinterProfile::generic_msla_4k()` + `ResinProfile::generic_standard()`.
    pub fn default_for_test() -> Self {
        use resinsim_core::services::failure_predictor::{
            LayerOverrides, SupportConfig, ThermalContext,
        };
        use resinsim_core::services::build_plate::PlateAdhesionProfile;
        use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

        let area = CrossSectionArea::new(100.0).expect("100 mm² is non-negative");
        Self {
            layer: 20, // past bottom_layer_count (6) — normal exposure branch
            area,
            prev_area: area,
            overrides: LayerOverrides::default(),
            resin: ResinBuilder::new().build(),
            printer: PrinterBuilder::new().build(),
            supports: SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            plate: PlateAdhesionProfile::default_textured(),
            thermal: ThermalContext {
                ambient: AmbientTemperature::new(22.0)
                    .expect("22 °C is in AmbientTemperature domain"),
                initial_led_temp: None,
            },
        }
    }

    /// Set zero peel force by forcing a zero layer area — cure energy
    /// × 0 area = 0 peel force — which is the safety-factor-zero-force
    /// scenario's precondition. Returns Self for chaining.
    pub fn with_zero_area(mut self) -> Self {
        self.area = resinsim_core::values::CrossSectionArea::new(0.0)
            .expect("0 mm² is non-negative");
        self.prev_area = self.area;
        self
    }
}
