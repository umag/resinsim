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
// Fields are read by subsets of step-def modules; `dead_code` would
// otherwise fire in the uat_extractor binary that pulls the tree in only
// for the extract + extract_tests siblings.
#[allow(dead_code)]
pub struct UatWorld {
    // ---- Safety-factor-zero-force scenarios ----
    pub capacity: Option<SupportCapacity>,
    pub force: Option<PeelForce>,
    pub computed_safety: Option<Option<SafetyFactor>>,

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
