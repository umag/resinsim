//! Shared test-support modules for the UAT BDD suite.
//!
//! `extract` parses `spec/uat/*.md` files where each scenario lives inside a
//! ```gherkin fenced code block. See docs/adr/0008-bdd-uat-spike-notes.md
//! and docs/patterns/extracting-gherkin-from-markdown.md for context.
//!
//! This tree is pulled in by two sibling test binaries:
//! - `tests/uat_extractor.rs` — default libtest harness; hosts the
//!   unit + property tests below via `extract_tests`.
//! - `tests/uat_gherkin.rs` — `harness = false` cucumber driver. It
//!   loads every `spec/uat/*.md` via the extractor and runs scenarios
//!   against the step defs under `tests/uat_steps/`.

pub mod extract;

pub mod extract_tests;

pub mod world;

pub mod fixtures;

pub mod cli_fixtures;

// Per-UAT-file step definition modules. snake_case names mirror the
// kebab-case spec/uat/*.md file names verbatim for grep traceability
// (docs/patterns/extracting-gherkin-from-markdown.md).
pub mod cli_profile_by_name_loading;
pub mod cli_requires_resin_for_recipe_fields;
pub mod cli_temperature_flag_validation;
pub mod cure_depth_nan_guard;
pub mod legacy_resin_toml_defaults;
pub mod legacy_resin_toml_without_recipe;
pub mod legacy_resin_toml_without_ref_lift_speed;
pub mod recipe_inside_printer_range;
pub mod recipe_out_of_range;
pub mod resin_switch_changes_simulation;
pub mod safety_factor_zero_force;
pub mod suction_detector_raft_false_positive;
pub mod thermal_degradation;
