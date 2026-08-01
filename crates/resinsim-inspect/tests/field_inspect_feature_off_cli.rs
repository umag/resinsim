//! Feature-off surface for `resinsim inspect field`. t2f6-field-inspector.
//!
//! Runs ONLY when `resinsim-inspect/field-sim` is OFF — the counterpart
//! to `field_inspect_cli.rs`, which is gated the opposite way. Asserts
//! the config-(1)/(3) half of the ADR-0017 matrix actually EXERCISES
//! something rather than merely compiling: the subcommand stays
//! visible in `--help`, and the handler exits 2 with an actionable
//! message naming the feature and the rebuild command — the
//! deliberate divergence from the `--voxel-cure-mm` bare-unknown-flag
//! precedent (ADR-0023).
//!
//! `#![cfg(not(feature = "field-sim"))]` is load-bearing, not
//! decorative: this file spawns `env!("CARGO_BIN_EXE_resinsim")`,
//! which is THE SAME BINARY cargo is building for the current
//! invocation. Under config 4 (`cargo nextest run --workspace
//! --features resinsim-inspect/field-sim,...`) that binary genuinely
//! HAS field-sim compiled in, so every "must exit 2 / must stay
//! feature-off" assertion here would fail against real (working)
//! output instead of the feature-off stub — caught by running the
//! four-config matrix, exactly what config 4 exists to catch (ADR-0017
//! four-config matrix: "config 2/4 catch the inverse").
#![cfg(not(feature = "field-sim"))]

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_resinsim")
}

#[test]
fn field_subcommand_still_listed_in_inspect_help() {
    let out = Command::new(bin())
        .args(["inspect", "--help"])
        .output()
        .expect("spawn resinsim inspect --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("field"),
        "the `field` subcommand must stay visible in --help even in the default build: {stdout}"
    );
}

#[test]
fn field_help_lists_all_flags() {
    let out = Command::new(bin())
        .args(["inspect", "field", "--help"])
        .output()
        .expect("spawn resinsim inspect field --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--in",
        "--field",
        "--slice",
        "--bins",
        "--values",
        "--cured-only",
        "--json",
    ] {
        assert!(
            stdout.contains(flag),
            "field --help must list {flag} even feature-off: {stdout}"
        );
    }
}

#[test]
fn field_handler_exits_2_naming_feature_and_rebuild_command() {
    let out = Command::new(bin())
        .args([
            "inspect",
            "field",
            "--in",
            "does-not-need-to-exist.sim.json",
            "--field",
            "cure",
            "--slice",
            "z=0",
        ])
        .output()
        .expect("spawn resinsim inspect field");
    assert_eq!(
        out.status.code(),
        Some(2),
        "feature-off handler must exit 2; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("field-sim"),
        "stderr must name the field-sim feature: {stderr}"
    );
    assert!(
        stderr.contains("cargo build") && stderr.contains("--features"),
        "stderr must name the exact rebuild command: {stderr}"
    );
    assert!(
        out.stdout.is_empty(),
        "stdout must stay empty on the feature-off error path"
    );
}

#[test]
fn field_handler_exit_2_message_is_identical_under_json_flag() {
    let out = Command::new(bin())
        .args([
            "inspect",
            "field",
            "--in",
            "does-not-need-to-exist.sim.json",
            "--field",
            "cure",
            "--slice",
            "z=0",
            "--json",
        ])
        .output()
        .expect("spawn resinsim inspect field --json");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "--json must not change the feature-off error path"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("field-sim"));
}

#[test]
fn slice_parse_time_validation_still_runs_feature_off() {
    // --slice validation is plain data (parse_slice_spec), so it must
    // still reject a malformed spec at CLAP PARSE TIME (before the
    // feature-off handler body even runs) in the default build too.
    let out = Command::new(bin())
        .args([
            "inspect",
            "field",
            "--in",
            "irrelevant.sim.json",
            "--field",
            "cure",
            "--slice",
            "not-a-valid-spec",
        ])
        .output()
        .expect("spawn resinsim inspect field with a malformed --slice");
    assert!(!out.status.success());
    // clap's own parse-time rejection exits 2 as well, but via a
    // different code path than the feature-off handler body; either
    // way this must not be exit 0 and must not panic.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked at"),
        "malformed --slice must not panic feature-off: {stderr}"
    );
}
