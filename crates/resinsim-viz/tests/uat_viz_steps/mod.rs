//! Shared test-support + per-spec step-def modules for the viz UAT suite.
//!
//! `viz_cli` is shared support (subprocess invocation of the
//! `resinsim-viz` binary under test). Per-spec step-def modules mirror
//! resinsim-core's naming convention: snake_case name matches the
//! kebab-case `spec/uat/viz-*.md` stem verbatim, for grep traceability.
//!
//! `NON_STEP_MODULES` lists shared-support modules (viz_cli) that are
//! NOT per-spec step-def bindings — the layer-3 cross-check
//! (`assert_mod_rs_and_use_list_agree` in `uat_viz_gherkin.rs`) reads
//! it to distinguish step modules from support modules when comparing
//! `pub mod` declarations against the `use` bindings that force linking.
//!
//! NON_STEP_MODULES and the layer-3 `assert_mod_rs_and_use_list_agree`
//! cross-check are now active (triggered by the second step-def module
//! landing). See `uat_viz_gherkin.rs` for the check.

pub mod viz_cli;

pub mod viz_allow_mismatch_soft_fallback;
pub mod viz_bad_pairing;
pub mod viz_layer_count_mismatch_hard_error;
pub mod viz_load_ctb_with_sim_renders_heatmap;
pub mod viz_load_sim_missing_sidecar;
pub mod viz_screenshot_ctb;
pub mod viz_screenshot_flag;

/// Modules under `uat_viz_steps/` that are shared support code, not
/// per-spec step-def bindings. Single source for this list — layer 3
/// (`assert_mod_rs_and_use_list_agree` in `uat_viz_gherkin.rs`) reads
/// it from here.
pub const NON_STEP_MODULES: &[&str] = &["viz_cli"];
