//! Shared test-support + per-spec step-def modules for the viz UAT suite.
//!
//! `viz_cli` is shared support (subprocess invocation of the
//! `resinsim-viz` binary under test). Per-spec step-def modules mirror
//! resinsim-core's naming convention: snake_case name matches the
//! kebab-case `spec/uat/viz-*.md` stem verbatim, for grep traceability.
//!
//! This tree currently has ONE per-spec module
//! (`viz_screenshot_flag`), piloting `spec/uat/viz-screenshot-flag.md`
//! per docs/adr/0024-second-uat-harness-in-resinsim-viz.md. Every other
//! viz spec stays fully skipped, registered in
//! `tests/uat_viz_gherkin.rs::SPECS_WITHOUT_STEP_DEFS`.
//!
//! Unlike resinsim-core's `uat_steps/mod.rs`, there is no
//! `NON_STEP_MODULES` bookkeeping and no layer-3
//! `assert_mod_rs_and_use_list_agree` cross-check yet — with a single
//! step module, `-Aunused_imports` (.cargo/config.toml) has nothing to
//! silently drop, so that guard would be pure noise. Add both back when
//! a second viz step-def module lands (see `uat_viz_gherkin.rs`'s
//! comment at the same point).

pub mod viz_cli;

pub mod viz_screenshot_flag;
