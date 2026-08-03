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
use resinsim_core::values::{LayerMask, PeelForce, SafetyFactor, SupportCapacity};

use super::fixtures::{
    PRINTER_BUILD_ENVELOPE_INLINE, PRINTER_FIELD_SIM_SCALARS, RESIN_FIELD_SIM_THERMAL_LINES,
};

#[derive(Debug, Default, World)]
pub struct UatWorld {
    // ---- Safety-factor-zero-force scenarios ----
    /// Unused by current step defs (step 9 moved to predict_layer
    /// integration which populates `predict_layer_result`), retained
    /// for future component-level scenarios.
    #[expect(
        dead_code,
        reason = "pre-step-9 spike mirror; kept for future scenarios"
    )]
    pub capacity: Option<SupportCapacity>,
    #[expect(
        dead_code,
        reason = "pre-step-9 spike mirror; kept for future scenarios"
    )]
    pub force: Option<PeelForce>,
    #[expect(
        dead_code,
        reason = "pre-step-9 spike mirror; kept for future scenarios"
    )]
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
    #[allow(dead_code, reason = "diagnostic capture; not yet asserted")]
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

    // ---- ctb-layer-height-authority UAT scenarios ----
    /// Per-scenario CTB layer stack. Set by `Given a CTB input sliced at
    /// N µm`. Stored on World rather than a thread_local because cucumber
    /// runs scenarios concurrently — a thread_local would leak state
    /// across scenarios on the same thread (caught 2026-05-19).
    pub ctb_layer_inputs: Option<Vec<resinsim_core::io::sliced::LayerInput>>,

    // ---- Peel-physics band (base-adhesion-shifts-peel-peak,
    // profile-vacuum-pressure-scales-suction,
    // peel-shape-factor-scales-with-aspect-ratio) — uat-unskip-campaign
    // increment 1, plan step 7. Grouped so the three sibling modules share
    // one obvious block rather than scattering fields across the struct. ----
    /// Resin under test, built via `ResinBuilder` — never hand-copied TOML.
    pub peel_resin: Option<ResinProfile>,
    /// Printer under test, built via `PrinterBuilder`.
    pub peel_printer: Option<PrinterProfile>,
    /// Full per-layer output from the shared "When a job is simulated"
    /// step (`SimulationRunner::run_from_areas` / `run_from_layer_inputs`),
    /// read directly by Then steps — never reconstructed from a formula.
    pub peel_sim_layers: Option<Vec<resinsim_core::entities::LayerResult>>,
    /// `validate()` error captured for profile-rejection scenarios (vacuum
    /// UAT-3), so the step asserts on the `Result` instead of panicking at
    /// construction time.
    pub peel_validate_err: Option<String>,
    /// Layer masks under direct shape-factor comparison (peel-shape
    /// UAT-1), built via `LayerMaskBuilder`.
    pub peel_masks: Option<Vec<LayerMask>>,
    /// Synthetic `LayerInput` stack (e.g. a closed-cup sealed cavity) for
    /// `run_from_layer_inputs`-driven scenarios (profile-vacuum-pressure
    /// UAT-1). Kept distinct from `ctb_layer_inputs` above — that field is
    /// specifically the CTB-sourced stack for ctb-layer-height-authority.
    pub peel_layer_inputs: Option<Vec<resinsim_core::io::sliced::LayerInput>>,
    /// Per-mask `PeelForceCalculator::peel_shape_factor` results, in the
    /// same order as `peel_masks` (peel-shape-factor-scales-with-aspect-
    /// ratio UAT-1's direct compact-vs-thin comparison).
    pub peel_mask_shape_factors: Option<Vec<f32>>,
    /// `FailurePredictor::predict_layer` output with NO shape factor
    /// applied — the "before" half of peel-shape UAT-4's isolation check.
    pub peel_shape_unshaped_result: Option<resinsim_core::entities::LayerResult>,
    /// Same, WITH a shape factor applied — the "after" half.
    pub peel_shape_shaped_result: Option<resinsim_core::entities::LayerResult>,

    // ---- Interlayer-crack band (interlayer-crack-knockdown-scales-with-
    // perimeter) — uat-unskip-campaign increment A2. Grouped per the
    // peel-physics band's precedent above. ----
    /// The equal-area compact/thin `LayerResult` stacks — UAT-1's
    /// compact-square-vs-thin pair AND UAT-2's without/with-real-perimeter
    /// pair AND UAT-4's still-holds branch (`crack_compact_layers`) reuse
    /// this field across their scenarios (cucumber resets `World` between
    /// scenarios, so there is no cross-scenario leakage). "No-crack" side
    /// first, "cracked" side second, by convention.
    pub crack_compact_layers: Option<Vec<resinsim_core::entities::LayerResult>>,
    /// The "cracked" side of the pair above — UAT-1's thin mask, UAT-2's
    /// with-real-perimeter run, UAT-4's fires branch.
    pub crack_thin_layers: Option<Vec<resinsim_core::entities::LayerResult>>,
    /// `CrackPropagator::effective_bonded_fraction` output, compact-then-
    /// thin order (UAT-1).
    pub crack_bonded_fractions: Option<Vec<f64>>,
    /// `SupportAnalyzer::assess(..).plate_capacity_n` SCALARS only — never
    /// the full `SupportAssessment` (the scalar is all any Then step
    /// reads). Reused across UAT-1 (compact-then-thin interlayer capacity),
    /// UAT-3 (bottom-layer baseline-then-cracked plate adhesion), and
    /// UAT-4 (the single reduced_interlayer_n on whichever side of the
    /// Delamination gate the current When/Then pair is proving).
    pub crack_interlayer_capacity_n: Option<Vec<f32>>,
    /// UAT-3's placeholder-mask layers — the bottom-layer-with-real-
    /// perimeter run is captured separately (via
    /// `crack_interlayer_capacity_n`); this field holds the
    /// `run_from_areas` (1×1) and `run_from_layer_inputs` (W×H fallback)
    /// layer stacks, concatenated.
    pub crack_placeholder_layers: Option<Vec<resinsim_core::entities::LayerResult>>,
    /// UAT-4's first When/Then pair: `PrintSimulation::failures()` from the
    /// branch where the crack-reduced interlayer capacity is BELOW the
    /// shaped peel load (Delamination fires).
    pub crack_failures_below: Option<Vec<resinsim_core::entities::FailureEvent>>,
    /// UAT-4's second When/Then pair: same, for the branch where capacity
    /// still EXCEEDS the peel load (no Delamination).
    pub crack_failures_above: Option<Vec<resinsim_core::entities::FailureEvent>>,

    // ---- sim-json-roundtrips-zero-force-layer — uat-unskip-campaign
    // ratified A2 top-up. Reuses `sim_primary` (recipe + pairing band,
    // above) for the constructed `PrintSimulation`, and `last_sim_err` /
    // `cli_exit_code` / `cli_stdout` / `cli_stderr` (CLI subprocess band,
    // above) for the two consumer scenarios' real `resinsim report
    // health` invocations. This is the one field genuinely new to this
    // spec. ----
    /// Path of the `sim.json` this spec's Given/When steps produced via
    /// the real `save_to_path` / `save_with_provenance` entry points —
    /// never a hand-serialized JSON literal.
    pub sim_json_path: Option<std::path::PathBuf>,

    // ---- Nanodlp band (athena-analytic-log-ingest,
    // cumulative-times-sec-accessor, nanodlp-import-simulates,
    // nanodlp-archive-bomb-rejected, nanodlp-calibrate-compares-real-force)
    // — uat-unskip-a3-b. Grouped per the peel-physics / interlayer-crack
    // bands' precedent above. Reuses `cli_cmd` / `cli_exit_code` /
    // `cli_stdout` / `cli_stderr` / `sim_json_path` (CLI subprocess band,
    // above) for every CLI-subprocess scenario in this band — the fields
    // below are only the ones genuinely new to these five specs. ----
    /// Path of a `.nanodlp` fixture under test — either a committed fixture
    /// (`mini.nanodlp` / `bomb-dimensions.nanodlp`) or a
    /// `NanoDlpJobBuilder`-synthesised archive.
    pub nanodlp_fixture_path: Option<std::path::PathBuf>,
    /// Committed Athena analytic-CSV twin paths (plain + `.csv.gz`) or a
    /// synthesised malformed-row temp CSV — athena-analytic-log-ingest.
    pub athena_csv_paths: Option<Vec<std::path::PathBuf>>,
    /// Raw `--json` stdout from one or more `inspect athena --json`
    /// invocations, parsed by the Then steps that cross-check the text-mode
    /// numbers against a second production observation.
    pub athena_json_stdout: Option<Vec<String>>,
    /// In-process `io::sliced::parse_sliced` output — nanodlp-import-
    /// simulates UAT-2's "the job is imported" When. `values::LayerInput`
    /// (NOT the `io::sliced::LayerInput` re-export path — plan-review
    /// finding 1) since `values::layer_input` is the canonical home post
    /// ctb-layer-height-authority's move (`io::sliced` only re-exports it
    /// for old callers).
    pub imported_layers: Option<Vec<resinsim_core::values::LayerInput>>,
    /// `(K, support_exposure_sec, normal_exposure_sec)` recorded by
    /// `NanoDlpJobBuilder::build` at fixture-construction time — Thens
    /// compare per-layer `exposure_sec` against THESE recorded inputs,
    /// never a re-derived `if i < K` branch expression.
    pub imported_recipe_exposures: Option<(u32, f32, f32)>,
    /// `PrintSimulation` built for cumulative-times-sec-accessor's two
    /// scenarios (100-layer cube via `run_from_layer_inputs`, and the empty
    /// aggregate via `PrintSimulation::new`).
    pub cumulative_sim: Option<PrintSimulation>,
    /// `sim.cumulative_times_sec()` output, captured once by the shared
    /// When so both UAT-1 and UAT-2's Thens read the same production Vec.
    pub cumulative_times: Option<Vec<f32>>,
    /// `(bytes, mtime)` of `data/printers/athena_ii.toml`, captured before
    /// a calibrate invocation so the Then can assert the file is
    /// byte-identical and un-touched afterward (nanodlp-calibrate-compares-
    /// real-force UAT-1).
    pub printer_toml_before: Option<(Vec<u8>, std::time::SystemTime)>,
    /// Wall-clock duration of a CLI invocation — nanodlp-archive-bomb-
    /// rejected's 30s advisory bound (relative to the branch-message
    /// discrimination, not a strict timing assertion; see the module doc).
    pub cli_elapsed: Option<std::time::Duration>,
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
    /// ADR-0022 Stage 2 suction ΔP. `None` ⇒ the TOML omits the key
    /// entirely, matching the "unset" branch of `PrinterProfile`'s
    /// `Option<f32>` field (profile-vacuum-pressure-scales-suction UAT-2).
    vacuum_pressure_kpa: Option<f32>,
}

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
            vacuum_pressure_kpa: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
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

    /// Not called by any scenario landed so far (base-adhesion / vacuum /
    /// peel-shape don't touch Z-deflection failure prediction) — scoped
    /// `expect` rather than the blanket impl-level `allow` this builder
    /// used to carry, so removing the blanket (uat-unskip-campaign
    /// increment 1) doesn't silently widen into "every unused method is
    /// now invisible to clippy" again. Reserved for a future
    /// Z-deflection-band UAT scenario.
    #[expect(dead_code, reason = "reserved for a future Z-deflection UAT scenario")]
    pub fn with_z_stiffness(mut self, n_per_mm: f32) -> Self {
        self.z_stiffness_n_per_mm = n_per_mm;
        self
    }

    /// ADR-0022 Stage 2 suction ΔP (profile-vacuum-pressure-scales-suction
    /// UAT-1/UAT-3). Deliberately accepts any `f32` including out-of-range
    /// or non-finite values — validity is `PrinterProfile::validate()`'s
    /// job, exercised via [`Self::build_unvalidated`] for the rejection
    /// scenario, not this setter's.
    pub fn with_vacuum_pressure_kpa(mut self, kpa: f32) -> Self {
        self.vacuum_pressure_kpa = Some(kpa);
        self
    }

    pub fn build(self) -> resinsim_core::entities::PrinterProfile {
        let p = self.build_unvalidated();
        p.validate()
            .expect("PrinterBuilder output must satisfy validate()");
        p
    }

    /// Parse the assembled TOML WITHOUT validating. For scenarios that
    /// intentionally construct an out-of-range profile (above-atmospheric /
    /// zero / negative / NaN `vacuum_pressure_kpa`) and assert on the
    /// `validate()` `Err` themselves — `build()` would panic before the
    /// step ever got to make that assertion.
    pub fn build_unvalidated(self) -> resinsim_core::entities::PrinterProfile {
        // TOML has no `NaN` literal (Rust's `{v}` Display prints "NaN",
        // capitalised, which the TOML parser rejects) — the spec-lowercase
        // `nan` keyword is what TOML 1.0 / the `toml` crate accept.
        let vacuum_line = match self.vacuum_pressure_kpa {
            None => String::new(),
            Some(v) if v.is_nan() => "vacuum_pressure_kpa = nan\n".to_string(),
            Some(v) => format!("vacuum_pressure_kpa = {v}\n"),
        };
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
{vacuum_line}{field_sim_scalars}{envelope}"#,
            name = self.name,
            led = self.led_power_mw_cm2,
            layer_min = self.layer_min,
            layer_max = self.layer_max,
            exp_min = self.exposure_min,
            exp_max = self.exposure_max,
            lift_min = self.lift_speed_min,
            lift_max = self.lift_speed_max,
            stiff = self.z_stiffness_n_per_mm,
            // ADR-0020 / t2f4: root-level vat-wall/convective scalars +
            // INLINE build_envelope_mm (never a `[build_envelope_mm]`
            // header block — a header would silently swallow any scalar
            // appended after it into the table; see
            // docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
            field_sim_scalars = PRINTER_FIELD_SIM_SCALARS,
            envelope = PRINTER_BUILD_ENVELOPE_INLINE,
        );
        toml::from_str(&toml_str).expect("PrinterBuilder TOML must parse")
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
    /// ADR-0022 Stage 1 first-layer base-adhesion term (KB-116). `None` ⇒
    /// the TOML omits the key, matching the "unset" branch
    /// (base-adhesion-shifts-peel-peak UAT-2).
    base_adhesion_elevation_kpa: Option<f32>,
    /// ADR-0022 Stage 3 A/L peel shape factor strength (KB-185 Tier-1).
    /// `None` ⇒ the TOML omits the key (peel-shape-factor-scales-with-
    /// aspect-ratio UAT-2).
    peel_shape_factor_strength: Option<f32>,
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
            base_adhesion_elevation_kpa: None,
            peel_shape_factor_strength: None,
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

    /// ADR-0022 Stage 1 first-layer base-adhesion term (KB-116).
    /// base-adhesion-shifts-peel-peak UAT-1.
    pub fn with_base_adhesion_elevation_kpa(mut self, kpa: f32) -> Self {
        self.base_adhesion_elevation_kpa = Some(kpa);
        self
    }

    /// ADR-0022 Stage 3 A/L peel shape factor strength (KB-185 Tier-1).
    /// peel-shape-factor-scales-with-aspect-ratio UAT-1/UAT-3/UAT-4.
    pub fn with_peel_shape_factor_strength(mut self, strength: f32) -> Self {
        self.peel_shape_factor_strength = Some(strength);
        self
    }

    pub fn build(self) -> resinsim_core::entities::ResinProfile {
        let thermal_lines = match (self.degradation_temp_c, self.min_safe_temp_c) {
            (Some(d), Some(m)) => format!("degradation_temp_c = {d}\nmin_safe_temp_c = {m}\n"),
            _ => String::new(),
        };
        // Both opt-in ADR-0022 scalars are plain root-level keys — `None`
        // omits the line entirely so the profile round-trips through the
        // same "unset" branch as a pre-Stage-1/Stage-3 TOML.
        let base_adhesion_line = self
            .base_adhesion_elevation_kpa
            .map(|v| format!("base_adhesion_elevation_kpa = {v}\n"))
            .unwrap_or_default();
        let peel_shape_line = self
            .peel_shape_factor_strength
            .map(|v| format!("peel_shape_factor_strength = {v}\n"))
            .unwrap_or_default();
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
{base_adhesion_line}{peel_shape_line}{field_sim_thermal}{thermal_lines}
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
            // ADR-0020 / t2f4: root-level thermal-material scalars, required
            // under field-sim. Must land BEFORE {recipe} — a [recipe] header
            // has already been opened by the time {recipe} interpolates, so
            // anything after it would silently nest into the recipe table
            // (docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
            field_sim_thermal = RESIN_FIELD_SIM_THERMAL_LINES,
            recipe = self.recipe.to_toml(),
        );
        let r: resinsim_core::entities::ResinProfile =
            toml::from_str(&toml_str).expect("ResinBuilder TOML must parse");
        r.validate()
            .expect("ResinBuilder output must satisfy validate()");
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
        use resinsim_core::services::build_plate::PlateAdhesionProfile;
        use resinsim_core::services::failure_predictor::{
            LayerOverrides, SupportConfig, ThermalContext,
        };
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
        self.area =
            resinsim_core::values::CrossSectionArea::new(0.0).expect("0 mm² is non-negative");
        self.prev_area = self.area;
        self
    }
}

/// Builder for the `LayerMask` shapes the peel-physics band's shape-factor
/// scenarios need (plan step 7). Named shapes only — no general-purpose
/// mask DSL, so each factory documents exactly which UAT it exists for.
pub struct LayerMaskBuilder;

impl LayerMaskBuilder {
    /// A solid `side × side` block MARGINED inside a `(side + 2) × (side +
    /// 2)` grid — i.e. NOT fully solid — so it exercises the real
    /// `raw = 4√A / L` formula rather than the `is_fully_solid()`
    /// placeholder guard (that guard is what UAT-3 tests separately, via
    /// [`Self::fully_solid`]). `side = 3` (9-cell area, matching the
    /// shipped `build_shape_factor_map_off_fully_solid_and_thin` nextest
    /// fixture) is the KB-181 square baseline: perimeter 12 mm at 1 mm
    /// voxels → `raw = 4·3/12 = 1.0`.
    ///
    /// Pair with [`Self::thin_1xn`]`(side * side, ..)` for an EQUAL-AREA
    /// compact-vs-thin comparison (peel-shape-factor-scales-with-aspect-
    /// ratio UAT-1's exact requirement — the existing nextest fixture
    /// compares a 9-cell square against a 5-cell line, which is NOT
    /// equal-area and so cannot stand in for this UAT).
    pub fn compact_square(side: u32, voxel_mm: f32) -> LayerMask {
        let grid = side + 2;
        let mut m = LayerMask::new(grid, grid, voxel_mm).expect("margined square grid constructs");
        for x in 1..=side {
            for y in 1..=side {
                m.set(x, y).expect("block is inside the 1-cell margin");
            }
        }
        m
    }

    /// A solid 1-cell-wide, `length`-cell-tall line, margined inside a
    /// `3 × (length + 2)` grid (same "not fully solid" rationale as
    /// [`Self::compact_square`]). `length = side * side` from a paired
    /// `compact_square(side, ..)` call gives equal solid area with a much
    /// larger perimeter, so the ranking + `(0, 1)` bound in UAT-1 come from
    /// a genuine aspect-ratio difference, not an area difference.
    pub fn thin_1xn(length: u32, voxel_mm: f32) -> LayerMask {
        let mut m = LayerMask::new(3, length + 2, voxel_mm).expect("margined line grid constructs");
        for y in 1..=length {
            m.set(1, y).expect("line is inside the 1-cell margin");
        }
        m
    }

    /// A fully-solid `width × height` mask — the exact shape
    /// `SimulationRunner::run_from_areas` (`1×1`) and the
    /// `run_from_layer_inputs` maskless-input fallback (`W×H`) synthesise.
    /// `is_fully_solid()` is `true` by construction, so ADR-0022 Stage 3
    /// discriminates it to shape factor `1.0` regardless of strength
    /// (peel-shape-factor-scales-with-aspect-ratio UAT-3).
    pub fn fully_solid(width: u32, height: u32, voxel_mm: f32) -> LayerMask {
        LayerMask::new_all_solid(width, height, voxel_mm).expect("fully-solid grid constructs")
    }
}
