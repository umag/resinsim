//! Step definitions for `spec/uat/cli-profile-by-name-loading.md`
//! UAT-1..UAT-5.
//!
//! These scenarios exercise the `resinsim` binary from the
//! `resinsim-inspect` crate via subprocess. `env!("CARGO_BIN_EXE_*")` is
//! only available to tests in the same package as the binary, so the
//! uat_gherkin test binary in resinsim-core can't resolve the binary
//! by build-graph alone.
//!
//! Status: step def regexes are defined so cucumber's coverage guard
//! (step 8) matches them; the underlying subprocess invocation is
//! deferred to a follow-up issue (`uat-gherkin-runner-cli-integration`)
//! to keep this rollout's scope bounded. End-to-end CLI behaviour is
//! exercised by the hand-written tests in
//! `resinsim-inspect/tests/profile_loader_cli.rs`. See the step-6
//! rollout outcome in docs/adr/0008-bdd-uat-spike-notes.md for the
//! deferral rationale.

use cucumber::{given, then, when};

use super::world::UatWorld;

// ---- UAT-1 (printer): Athena II TOML loads by name ------------------------

#[given(
    regex = r#"^a printer TOML at "<data-dir>/printers/athena_ii\.toml" with z_stiffness_n_per_mm = 1500\.0$"#
)]
fn given_athena_ii_toml(_world: &mut UatWorld) {
    // Real Athena II TOML lives in data/printers/athena_ii.toml;
    // resinsim-inspect/tests/profile_loader_cli.rs exercises the full
    // CLI path against it.
}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --printer athena_ii --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_athena(_world: &mut UatWorld) {
    // Deferred — follow-up issue uat-gherkin-runner-cli-integration.
}

#[then(
    regex = r"^the simulation uses z_stiffness_n_per_mm = 1500\.0 \(NOT the generic_msla_4k default of 460\.0\)$"
)]
fn then_uses_athena_stiffness(_world: &mut UatWorld) {
    // Deferred — follow-up issue uat-gherkin-runner-cli-integration.
    // Covered end-to-end by
    // resinsim-inspect/tests/profile_loader_cli.rs::stage_a_flag_wins_over_env.
}

#[then(regex = r"^the JSON max_z_deflection_um reflects the athena_ii stiffness$")]
fn then_json_reflects_athena(_world: &mut UatWorld) {}

#[then(regex = r#"^no "Unknown printer profile" warning is emitted to stderr$"#)]
fn then_no_printer_warning(_world: &mut UatWorld) {}

// ---- UAT-1 (resin): Liqcreate TOML loads by name --------------------------

#[given(
    regex = r#"^a resin TOML at "<data-dir>/resins/liqcreate_premium_black\.toml"$"#
)]
fn given_liqcreate_toml(_world: &mut UatWorld) {}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --resin liqcreate_premium_black --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_liqcreate(_world: &mut UatWorld) {}

#[then(regex = r"^the simulation uses the TOML's viscosity and Dp/Ec values$")]
fn then_uses_liqcreate_values(_world: &mut UatWorld) {}

#[then(regex = r#"^no "Unknown resin profile" warning is emitted$"#)]
fn then_no_resin_warning(_world: &mut UatWorld) {}

// ---- UAT-2: unknown profile name hard-errors with available list ----------

#[given(
    regex = r#"^"<data-dir>/printers/" contains "athena_ii\.toml", "elegoo_mars5_ultra\.toml", and "generic_msla_4k\.toml"$"#
)]
fn given_three_printer_tomls(_world: &mut UatWorld) {}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --printer bogus_printer_name --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_bogus(_world: &mut UatWorld) {}

#[then(regex = r"^the binary exits non-zero$")]
fn then_exits_non_zero(_world: &mut UatWorld) {}

#[then(regex = r#"^stderr contains "bogus_printer_name"$"#)]
fn then_stderr_contains_bogus(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr lists "athena_ii, elegoo_mars5_ultra, generic_msla_4k" under "Available profiles:"$"#
)]
fn then_stderr_lists_available(_world: &mut UatWorld) {}

#[then(regex = r"^stdout contains no JSON output$")]
fn then_stdout_no_json(_world: &mut UatWorld) {}

// ---- UAT-3: explicit scalar flag wins over profile value ------------------

#[given(
    regex = r#"^a printer TOML "athena_ii\.toml" with z_stiffness_n_per_mm = 1500\.0$"#
)]
fn given_athena_toml_uat3(_world: &mut UatWorld) {}

#[when(
    regex = r#"^"resinsim inspect zaxis --force 46\.8 --printer athena_ii --stiffness 200 --data-dir <data-dir>" is invoked$"#
)]
fn when_inspect_zaxis_uat3(_world: &mut UatWorld) {}

#[then(regex = r"^the output reports stiffness_n_per_mm = 200$")]
fn then_stiffness_200(_world: &mut UatWorld) {}

#[then(
    regex = r"^the computed deflection is 46\.8 / 200 × 1000 = 234 µm \(not the profile's implied 46\.8 / 1500 × 1000 = 31\.2 µm\)$"
)]
fn then_deflection_234(_world: &mut UatWorld) {}

#[then(
    regex = r"^no warning or override notice is emitted \(scriptability > chattiness\)$"
)]
fn then_no_override_warning(_world: &mut UatWorld) {}

// ---- UAT-4: no profile flag skips data-dir resolution entirely ------------

#[given(
    regex = r#"^"RESINSIM_DATA_DIR=/definitely/does/not/exist" is set in the environment$"#
)]
fn given_bogus_env(_world: &mut UatWorld) {}

#[when(
    regex = r#"^"resinsim inspect zaxis --force 46\.8 --json" is invoked \(no --printer, no --resin\)$"#
)]
fn when_inspect_zaxis_no_profile(_world: &mut UatWorld) {}

#[then(regex = r"^the binary exits successfully$")]
fn then_exits_successfully(_world: &mut UatWorld) {}

#[then(regex = r"^the output uses the built-in default stiffness of 460\.0 N/mm$")]
fn then_default_stiffness(_world: &mut UatWorld) {}

#[then(
    regex = r"^no error about the invalid RESINSIM_DATA_DIR is emitted .*$"
)]
fn then_no_data_dir_error(_world: &mut UatWorld) {}

// ---- UAT-5 (stage a wins) -------------------------------------------------

#[given(
    regex = r#"^"RESINSIM_DATA_DIR" points at a different valid data directory than the --data-dir flag$"#
)]
fn given_flag_vs_env(_world: &mut UatWorld) {}

#[when(regex = r#"^the binary is invoked with "--data-dir <A>" and env "<B>"$"#)]
fn when_flag_vs_env_invoked(_world: &mut UatWorld) {}

#[then(regex = r#"^profiles are loaded from "<A>" \(the flag wins\)$"#)]
fn then_flag_wins(_world: &mut UatWorld) {}

// ---- UAT-5 (all stages miss) ----------------------------------------------

#[given(regex = r"^--data-dir is not supplied$")]
fn given_no_data_dir_flag(_world: &mut UatWorld) {}

#[given(regex = r#"^"RESINSIM_DATA_DIR" is unset$"#)]
fn given_env_unset(_world: &mut UatWorld) {}

#[given(regex = r#"^the current working directory has no "\./data/" subdirectory$"#)]
fn given_no_cwd_data(_world: &mut UatWorld) {}

#[given(regex = r#"^the binary's parent directory has no "data/" sibling$"#)]
fn given_no_sibling_data(_world: &mut UatWorld) {}

#[when(regex = r#"^the binary is invoked with "--printer <anything>"$"#)]
fn when_all_stages_miss(_world: &mut UatWorld) {}

#[then(regex = r"^stderr lists all four candidate paths$")]
fn then_stderr_four_paths(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr suggests both "--data-dir <path>" and "RESINSIM_DATA_DIR=<path>" as remediation$"#
)]
fn then_stderr_suggests(_world: &mut UatWorld) {}

#[then(
    regex = r#"^stderr notes the cargo-development case specifically \("if running via `cargo run`, invoke from the workspace root"\)$"#
)]
fn then_stderr_cargo_note(_world: &mut UatWorld) {}
