//! Shared cucumber World used by every UAT scenario binding.
//!
//! Created ahead of the plan's step 6 because step 2's smoke test already
//! needs the printer/resin/simulation-result fields alongside the spike's
//! safety-factor + cure-depth fields — step-def modules under
//! `tests/uat_steps/` must share one World type for cucumber to run all
//! scenarios through the same `UatWorld::cucumber()` builder.
//!
//! Builder helpers (`PrinterBuilder`, `ResinBuilder`, `RecipeBuilder`,
//! `PredictLayerInputs`) land in step 6; for now the struct carries raw
//! domain types and scenario-specific `last_*_err` capture fields.

use cucumber::World;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::values::{PeelForce, SafetyFactor, SupportCapacity};

#[derive(Debug, Default, World)]
// Fields are read by the uat_gherkin cucumber binary; uat_extractor pulls in
// the module tree only for its sibling `extract` + `extract_tests` modules,
// so dead_code would otherwise fire there.
#[allow(dead_code)]
pub struct UatWorld {
    // ---- Safety-factor-zero-force scenarios (spike) ----
    pub capacity: Option<SupportCapacity>,
    pub force: Option<PeelForce>,
    pub computed_safety: Option<Option<SafetyFactor>>,

    // ---- Cure-depth-NaN-guard scenarios (spike) ----
    pub last_energy_err: Option<&'static str>,
    pub last_panic_msg: Option<String>,

    // ---- Recipe-outside-printer-range scenarios (step 2 smoke test) ----
    pub printer: Option<PrinterProfile>,
    pub resin: Option<ResinProfile>,
    pub last_sim_err: Option<String>,
}
