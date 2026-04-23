//! Step definitions for
//! `spec/uat/cli-requires-resin-for-recipe-fields.md` UAT-1 + UAT-2.
//!
//! Folded review finding #4: step defs now subprocess the real
//! `resinsim inspect zaxis --json` and parse the layer_height field
//! out of the output.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::world::UatWorld;

/// Parse a "layer_height":<number> from a JSON-ish stdout. Tolerates
/// both `"layer_height": 50.0` and `"layer_height_um": 50.0` variants
/// so scenario step defs don't break on a cosmetic JSON key rename.
fn layer_height_from_stdout(stdout: &str) -> Option<f32> {
    for key in [
        "\"commanded_layer_height_um\"",
        "\"layer_height_um\"",
        "\"layer_height\"",
    ] {
        if let Some(idx) = stdout.find(key) {
            let tail = &stdout[idx + key.len()..];
            let tail = tail.trim_start_matches([':', ' ', '\t', '\n']);
            let end = tail
                .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                .unwrap_or(tail.len());
            if let Ok(v) = tail[..end].parse::<f32>() {
                return Some(v);
            }
        }
    }
    None
}

// ---- UAT-1 (profile-sourced) ----------------------------------------------

#[given(
    regex = r#"^the user invokes "resinsim inspect zaxis --force 50 --resin generic_standard --data-dir <dir> --json"$"#
)]
fn given_inspect_zaxis_resin(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    world.cli_cmd = Some(vec![
        "inspect".into(),
        "zaxis".into(),
        "--force".into(),
        "50".into(),
        "--resin".into(),
        "generic_standard".into(),
        "--data-dir".into(),
        data_dir.to_string_lossy().into_owned(),
        "--json".into(),
    ]);
}

#[when(regex = r"^the binary resolves the profile \(ADR-0004 4-stage data-dir chain\)$")]
fn when_resolves_profile(world: &mut UatWorld) {
    run_cli_from_world(world);
}

#[then(
    regex = r"^layer_height in the JSON output equals 50\.0 \(from ResinProfile::generic_standard\(\)\.recipe\(\)\.layer_height_um\(\)\)$"
)]
fn then_layer_height_50(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let lh = layer_height_from_stdout(stdout).unwrap_or_else(|| {
        panic!("layer_height not found in stdout; got: {stdout}")
    });
    assert!(
        (lh - 50.0).abs() < 1e-3,
        "layer_height must be 50.0 from generic_standard recipe; got {lh}",
    );
}

#[then(regex = r"^no error is printed to stderr$")]
fn then_no_stderr_error(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let exit = world.cli_exit_code.unwrap_or(-1);
    assert_eq!(exit, 0, "binary must exit 0; stderr={stderr}");
    // KB-153 Ea-default warning is acceptable (documented
    // "loud warning" in UAT-4 of the thermal spec). Actual errors
    // would come with "error:" / "Error:" prefix or a non-zero exit.
    assert!(
        !stderr.to_lowercase().starts_with("error:"),
        "stderr must not start with an error line; got: {stderr}",
    );
}

// ---- UAT-1 (explicit flag wins) -------------------------------------------

#[given(regex = r#"^the same command with "--layer-height 30\.0" also supplied$"#)]
fn given_explicit_layer_height(world: &mut UatWorld) {
    // Cucumber constructs a fresh World per scenario, so
    // `world.cli_cmd` is None here — the "same command" phrase refers
    // to the UAT-1 profile-sourced invocation. Rebuild it and append
    // the explicit flag.
    let data_dir = workspace_data_dir();
    world.cli_cmd = Some(vec![
        "inspect".into(),
        "zaxis".into(),
        "--force".into(),
        "50".into(),
        "--resin".into(),
        "generic_standard".into(),
        "--data-dir".into(),
        data_dir.to_string_lossy().into_owned(),
        "--json".into(),
        "--layer-height".into(),
        "30.0".into(),
    ]);
}

#[when(regex = r"^the binary runs$")]
fn when_binary_runs(world: &mut UatWorld) {
    run_cli_from_world(world);
}

#[then(regex = r"^layer_height in the JSON output equals 30\.0$")]
fn then_layer_height_30(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let lh = layer_height_from_stdout(stdout).unwrap_or_else(|| {
        panic!("layer_height not found in stdout; got: {stdout}")
    });
    assert!(
        (lh - 30.0).abs() < 1e-3,
        "explicit --layer-height 30.0 must win; got {lh}",
    );
}

#[then(regex = r"^the resin's recipe value is ignored in favour of the explicit flag$")]
fn then_recipe_ignored(world: &mut UatWorld) {
    // Confirm the recipe's default (50) did NOT surface.
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let lh = layer_height_from_stdout(stdout).unwrap_or(f32::NAN);
    assert!(
        (lh - 50.0).abs() > 0.5,
        "explicit flag must override recipe; layer_height={lh} looks like the 50.0 recipe default",
    );
}

// ---- UAT-2: built-in default when no --resin / --layer-height -------------

#[given(
    regex = r#"^"resinsim inspect zaxis --force 50 --printer generic_msla_4k --data-dir <dir> --json" with no --resin or --layer-height flag$"#
)]
fn given_no_resin_no_layer(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    world.cli_cmd = Some(vec![
        "inspect".into(),
        "zaxis".into(),
        "--force".into(),
        "50".into(),
        "--printer".into(),
        "generic_msla_4k".into(),
        "--data-dir".into(),
        data_dir.to_string_lossy().into_owned(),
        "--json".into(),
    ]);
}

#[then(regex = r"^layer_height in the JSON output equals 50\.0 \(the built-in default\)$")]
fn then_default_50(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let lh = layer_height_from_stdout(stdout).unwrap_or_else(|| {
        panic!("layer_height not found in stdout; got: {stdout}")
    });
    assert!(
        (lh - 50.0).abs() < 1e-3,
        "built-in default layer_height must be 50.0; got {lh}",
    );
}

#[then(regex = r"^the subcommand exits 0$")]
fn then_exits_0(world: &mut UatWorld) {
    let exit = world.cli_exit_code.unwrap_or(-1);
    assert_eq!(exit, 0, "subcommand must exit 0");
}

// ---- helper ----

fn run_cli_from_world(world: &mut UatWorld) {
    let cmd = world
        .cli_cmd
        .as_ref()
        .unwrap_or_else(|| panic!("scenario invariant: Given step set cli_cmd"));
    let args: Vec<&str> = cmd.iter().map(String::as_str).collect();
    let env = world
        .cli_env
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let outcome = invoke_resinsim(&args, &env);
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}
