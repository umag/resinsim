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
pub mod athena_analytic_log_ingest;
pub mod base_adhesion_shifts_peel_peak;
// FIELD-SIM-GATED (uat-unskip-band-d step 7): sole producer of
// `FailureType::WarpingRisk`, `FailurePredictor::predict_strain_failures`,
// is itself `#[cfg(feature = "field-sim")]`. See the module's own doc
// comment for the full symbol derivation.
#[cfg(feature = "field-sim")]
pub mod calibration_disclosure_3of3_predicate;
pub mod cli_inspect_field_slices_voxel_field;
// FIELD-SIM-GATED (uat-unskip-cross-feature-toml-interchange): UAT-2's
// sole error producer, ResinProfile::validate()'s thermal_conductivity_w_mk
// required check (resin_profile.rs), is #[cfg(feature = "field-sim")].
// See the module's own doc comment for the full rationale.
#[cfg(feature = "field-sim")]
pub mod cross_feature_toml_interchange;
// FIELD-SIM-GATED (uat-unskip-cli-sim-rejects-tampered-sidecar): all
// four scenarios need the field-sim binary (producer --voxel-cure-mm +
// consumer load_and_install_sidecar_with_budget, both #[cfg(feature =
// "field-sim")]). See the module's own doc comment for the full symbol
// derivation.
#[cfg(feature = "field-sim")]
pub mod cli_sim_rejects_tampered_sidecar;
pub mod cli_profile_by_name_loading;
pub mod cli_report_health_layer_height_provenance;
pub mod cli_report_health_print_time;
pub mod cli_report_health_surfaces_ea_default_advisory;
pub mod cli_requires_resin_for_recipe_fields;
pub mod cli_sim_producer_writes_sim_json;
pub mod cli_sim_rejects_unknown_schema_version;
pub mod cli_sim_voxel_cure_emits_tier2_thermal_log;
pub mod cli_temperature_flag_validation;
pub mod ctb_layer_height_authority;
pub mod cumulative_times_sec_accessor;
pub mod cure_depth_nan_guard;
// FIELD-SIM-GATED (uat-unskip-band-d step 6): sole entry point
// `SimulationRunner::run_from_layer_inputs_with_voxel` is itself
// `#[cfg(feature = "field-sim")]`. See the module's own doc comment for
// the full symbol derivation.
#[cfg(feature = "field-sim")]
pub mod honest_zero_yield_fraction_on_calibrated_solid;
pub mod interlayer_crack_knockdown_scales_with_perimeter;
pub mod legacy_resin_toml_defaults;
pub mod legacy_resin_toml_without_recipe;
pub mod legacy_resin_toml_without_ref_lift_speed;
pub mod light_crosstalk_3d_gaussian_convolution;
// FIELD-SIM-GATED (uat-unskip-light-crosstalk-3d-gaussian-convolution): sole
// entry point `SimulationRunner::run_from_layer_inputs_with_voxel` is itself
// `#[cfg(feature = "field-sim")]`. SECOND module for this spec — the first
// (ungated, above) covers UAT-5/6/7 validation scenarios. See the module's
// own doc comment for the full derivation.
#[cfg(feature = "field-sim")]
pub mod light_crosstalk_3d_gaussian_convolution_runtime;
pub mod nanodlp_archive_bomb_rejected;
pub mod nanodlp_calibrate_compares_real_force;
pub mod nanodlp_import_simulates;
pub mod peel_shape_factor_scales_with_aspect_ratio;
pub mod profile_vacuum_pressure_scales_suction;
pub mod recipe_inside_printer_range;
pub mod recipe_out_of_range;
pub mod resin_switch_changes_simulation;
pub mod safety_factor_zero_force;
pub mod sim_json_roundtrips_zero_force_layer;
pub mod suction_detector_raft_false_positive;
pub mod thermal_degradation;
// FIELD-SIM-GATED (uat-unskip-sim-fields-sidecar-roundtrip): every
// producer scenario's entry points — SimulationRunner::
// run_from_layer_inputs_with_voxel (simulation_runner.rs:446-448),
// encode_paired_sidecar (simulation_repo.rs:424-435), CLI --voxel-cure-mm
// (main.rs:234-236), load_and_install_sidecar_with_budget
// (simulation_repo.rs:685-687) — are all #[cfg(feature = "field-sim")].
// UAT-4 (Tier-1 negative) is ungated but tested under field-sim for
// semantic strength. See the module's own doc comment.
#[cfg(feature = "field-sim")]
pub mod sim_fields_sidecar_roundtrip;
// FIELD-SIM-GATED (uat-unskip-voxel-cure-field-photoinitiator-depletion):
// every scenario's entry points — SimulationRunner::
// run_from_layer_inputs_with_voxel (simulation_runner.rs:446-448),
// CureField (cure_field.rs:32), PhotoinitiatorField
// (photoinitiator_field.rs:29), VoxelCureCalculator
// (voxel_cure_calculator.rs:45), CLI --voxel-cure-mm (main.rs:237) — are
// all #[cfg(feature = "field-sim")]. See the module's own doc comment for
// the full symbol derivation.
#[cfg(feature = "field-sim")]
pub mod voxel_cure_field_photoinitiator_depletion;
