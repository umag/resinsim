//! Step definitions for
//! `spec/uat/cli-requires-resin-for-recipe-fields.md` UAT-1 + UAT-2.
//!
//! CLI integration scenarios — same deferral rationale as
//! `cli_profile_by_name_loading.rs`. End-to-end coverage lives at
//! `resinsim-inspect/tests/profile_loader_cli.rs`. Follow-up issue:
//! `uat-gherkin-runner-cli-integration`.

use cucumber::{given, then, when};

use super::world::UatWorld;

// ---- UAT-1 (profile-sourced) ----------------------------------------------

#[given(
    regex = r#"^the user invokes "resinsim inspect zaxis --force 50 --resin generic_standard --data-dir <dir> --json"$"#
)]
fn given_inspect_zaxis_resin(_world: &mut UatWorld) {}

#[when(regex = r"^the binary resolves the profile \(ADR-0004 4-stage data-dir chain\)$")]
fn when_resolves_profile(_world: &mut UatWorld) {}

#[then(
    regex = r"^layer_height in the JSON output equals 50\.0 \(from ResinProfile::generic_standard\(\)\.recipe\(\)\.layer_height_um\(\)\)$"
)]
fn then_layer_height_50(_world: &mut UatWorld) {}

#[then(regex = r"^no error is printed to stderr$")]
fn then_no_stderr_error(_world: &mut UatWorld) {}

// ---- UAT-1 (explicit flag wins) -------------------------------------------

#[given(regex = r#"^the same command with "--layer-height 30\.0" also supplied$"#)]
fn given_explicit_layer_height(_world: &mut UatWorld) {}

#[when(regex = r"^the binary runs$")]
fn when_binary_runs(_world: &mut UatWorld) {}

#[then(regex = r"^layer_height in the JSON output equals 30\.0$")]
fn then_layer_height_30(_world: &mut UatWorld) {}

#[then(regex = r"^the resin's recipe value is ignored in favour of the explicit flag$")]
fn then_recipe_ignored(_world: &mut UatWorld) {}

// ---- UAT-2: built-in default when no --resin / --layer-height -------------

#[given(
    regex = r#"^"resinsim inspect zaxis --force 50 --printer generic_msla_4k --data-dir <dir> --json" with no --resin or --layer-height flag$"#
)]
fn given_no_resin_no_layer(_world: &mut UatWorld) {}

#[then(regex = r"^layer_height in the JSON output equals 50\.0 \(the built-in default\)$")]
fn then_default_50(_world: &mut UatWorld) {}

#[then(regex = r"^the subcommand exits 0$")]
fn then_exits_0(_world: &mut UatWorld) {}
