//! Step definitions for `spec/uat/cli-sim-rejects-unknown-schema-version.md`
//! UAT-1..UAT-4 (uat-unskip-c1, plan step 6). A four-way discrimination
//! suite over `load_envelope_with_budget`'s (simulation_repo.rs:644-689)
//! exactly four rejection branches.
//!
//! SYMBOL VERIFICATION. Every scenario here drives `resinsim report health
//! --in <PATH>` — `cmd_report_health` (main.rs:1685-1697) -> `load_envelope`
//! (simulation_repo.rs:602-610) -> `load_envelope_with_budget`
//! (simulation_repo.rs:644-689), all `#[cfg]`-free on this call path (the
//! only `#[cfg(feature = "field-sim")]` branch inside
//! `load_envelope_with_budget` is the sidecar-reattachment call, reached
//! only when `envelope.fields_sidecar` is `Some`, which none of these four
//! fixtures carry). `ensure_resinsim_built` (`cli_fixtures.rs:64-98`)
//! builds the subprocessed binary with no `--features`, so the binary is
//! byte-identical under `cargo uat` and `cargo uat-field-sim` regardless.
//! Fixtures that need a valid producer run go through `resinsim sim` on the
//! same default-features binary (see
//! `cli_sim_producer_writes_sim_json.rs`'s SYMBOL VERIFICATION for that
//! call path).
//!
//! `load_envelope_with_budget` has exactly four rejection branches,
//! documented as stable substrings at simulation_repo.rs:566-570 and
//! surfaced through `cmd_report_health`'s `eprintln!("Error: {e}")` +
//! `exit(1)` (main.rs:1691-1697):
//!
//! | branch   | line     | literal                                          |
//! |----------|----------|---------------------------------------------------|
//! | read     | :648-649 | `"failed to read {}: {e}"`                       |
//! | parse    | :650-651 | `"failed to parse {}: {e}"`                      |
//! | version  | :658-663 | `"unknown schema_version {} in {} (expected {}){hint}"` |
//! | validate | :667-669 | `"invalid simulation {}: {e}"`                   |
//!
//! Every Then below asserts its own branch's substring present AND the
//! other three absent (`assert_only_branch`, below) — the shape
//! `nanodlp_archive_bomb_rejected.rs` established for discrimination-style
//! assertions. A fifth production branch forces one edit to
//! `RejectBranch`/`assert_only_branch`, not four scattered edits.
//!
//! ADVISORY-ABSENCE CROSS-CHECK (the free fifth assertion, from the
//! post-refresh re-read). `cmd_report_health` bails on `Err` at
//! main.rs:1694-1695 and only reaches
//! `profile_loader::warn_if_envelope_ea_is_default` at main.rs:1703 on the
//! `Ok` arm — so every one of these four error paths must short-circuit
//! BEFORE the KB-153 advisory. `assert_only_branch` therefore also asserts
//! the KB-153 needles (`"30 kJ/mol"`, `"literature midpoint estimate"`,
//! `"KB-153"`, `"sim.json envelope"`) are ABSENT from stderr on every
//! scenario, proving the load failed before main.rs:1703 rather than
//! merely that an error string appeared somewhere. This is a genuine
//! negative control: `thermal_cli_warnings.rs:503-547`
//! (`report_health_warns_when_envelope_flags_ea_default`) drives the exact
//! same pipeline this module uses — `sim --stl data/test_cube.stl --resin
//! generic_standard --printer generic_msla_4k` then `report health --in`
//! — and asserts all four needles ARE present on the SUCCESS path, with
//! `:557-586` (`report_health_warns_exactly_once`) pinning the count at
//! exactly 1. This module asserts the complement on the four FAILURE
//! paths.
//!
//! Fixtures are always produced by a real `resinsim sim` run and then
//! tampered through parsed `serde_json::Value`, never hand-serialised
//! (UAT-3's garbage-bytes fixture is the sole exception — its whole point
//! is that the bytes are NOT JSON).
//!
//! REGEX DISTINCTNESS. This module's three Whens are all backtick-
//! delimited `` `resinsim report health --in ...` `` with mutually
//! distinct tails (`<PATH>`, the literal `/no/such/file.sim.json`, the
//! literal `/tmp/garbage.sim.json`), so none collide with each other. The
//! sharpest ambiguity risk in the increment: UAT-1/UAT-4's shared
//! `` `resinsim report health --in <PATH>` `` (backtick-delimited) differs
//! from `sim_json_roundtrips_zero_force_layer.rs:273`'s
//! `"resinsim report health --in <PATH>"` (double-quote-delimited) by ONE
//! delimiter character — cucumber's regex match is exact-string-anchored
//! (`^...$`), so the differing quote character keeps them distinct, but a
//! future edit that silently normalised the delimiter would collide. Also
//! distinct from `sim_json_roundtrips_zero_force_layer.rs:327`'s
//! `"resinsim report health --in <PATH> --json"` (double-quoted, `--json`
//! suffix) and `cli_profile_by_name_loading.rs:38,131,205`'s
//! `"resinsim report health --data-dir …" is invoked` (wholly different
//! sentence shape).
//!
//! `^the process exits with non-zero code$` (all four scenarios) is
//! `cli_sim_producer_writes_sim_json.rs`'s shared registration — reused
//! here, no new registration. `^the process does not panic$` (UAT-3) and
//! `^the process does not panic \(no "thread 'main' panicked" in
//! stderr\)$` (UAT-1) are two separate registrations for two separate
//! spec sentences — see the module's cross-spec leakage note below.
//!
//! CROSS-SPEC LEAKAGE (checked, safe). Registering
//! `^the process does not panic \(no "thread 'main' panicked" in
//! stderr\)$` also DEFINES that step in `cli-sim-rejects-tampered-sidecar`
//! UAT-1 (spec/uat/cli-sim-rejects-tampered-sidecar.md), and
//! `^the process does not panic$` also DEFINES it in
//! `cli-sim-budget-mismatch-on-load` UAT-1
//! (spec/uat/cli-sim-budget-mismatch-on-load.md). Both are harmless: each
//! of those scenarios skips at its own FIRST step (an undefined `Given`
//! naming a `--voxel-cure-mm` fixture that has no step-def module —
//! neither spec has one), so this later-step registration is never
//! reached; their register counts (4 and 3 respectively) are unaffected.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

/// The four stable rejection-branch substrings documented at
/// simulation_repo.rs:566-570.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RejectBranch {
    Read,
    Parse,
    Version,
    Validate,
}

impl RejectBranch {
    const ALL: [RejectBranch; 4] = [
        RejectBranch::Read,
        RejectBranch::Parse,
        RejectBranch::Version,
        RejectBranch::Validate,
    ];

    fn needle(self) -> &'static str {
        match self {
            RejectBranch::Read => "failed to read",
            RejectBranch::Parse => "failed to parse",
            RejectBranch::Version => "unknown schema_version",
            RejectBranch::Validate => "invalid simulation",
        }
    }
}

/// The KB-153 advisory needles (`profile_loader::
/// cure_kinetics_ea_default_warning_text` /
/// `warn_if_envelope_ea_is_default`) — see the module doc's ADVISORY-
/// ABSENCE CROSS-CHECK section.
const ADVISORY_NEEDLES: [&str; 4] = [
    "30 kJ/mol",
    "literature midpoint estimate",
    "KB-153",
    "sim.json envelope",
];

/// Assert stderr contains `expected`'s stable substring AND none of the
/// OTHER three — proving WHICH branch fired, not merely that an error
/// string appeared somewhere (`nanodlp_archive_bomb_rejected.rs`'s
/// discrimination shape). Also asserts every `ADVISORY_NEEDLES` entry is
/// absent (the free fifth assertion — every branch here rejects before
/// `warn_if_envelope_ea_is_default` can run).
fn assert_only_branch(world: &UatWorld, expected: RejectBranch) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains(expected.needle()),
        "expected stderr to contain {:?} (the {expected:?} branch), got: {stderr}",
        expected.needle(),
    );
    for branch in RejectBranch::ALL {
        if branch != expected {
            assert!(
                !stderr.contains(branch.needle()),
                "expected stderr to NOT contain {:?} (the {branch:?} branch — only \
                 {expected:?} may have fired), got: {stderr}",
                branch.needle(),
            );
        }
    }
    for needle in ADVISORY_NEEDLES {
        assert!(
            !stderr.contains(needle),
            "expected the KB-153 advisory needle {needle:?} to be ABSENT — the load must fail \
             before main.rs:1703 (warn_if_envelope_ea_is_default) on every rejection branch, \
             got: {stderr}"
        );
    }
}

/// Read a file as a `serde_json::Value` — never a hand-built `Value`.
fn read_json(path: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {} as JSON: {e}", path.display()))
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) {
    let text =
        serde_json::to_string_pretty(value).expect("serde_json::Value -> String cannot fail");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// Produce a real `sim.json` via a real `resinsim sim` subprocess run
/// (never hand-serialised) into a fresh `unique_tmp_dir`, and return its
/// path. Callers tamper the returned file through `serde_json::Value`.
fn produce_real_sim_json(tag: &str) -> std::path::PathBuf {
    let dir = unique_tmp_dir(tag);
    let input = dir.join("model.stl");
    std::fs::copy(workspace_data_dir().join("test_cube.stl"), &input)
        .unwrap_or_else(|e| panic!("copy test_cube.stl to {}: {e}", input.display()));
    let out = dir.join("model.sim.json");
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: real `resinsim sim` run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    out
}

// ---- UAT-1: schema_version tampered to 999 ----------------------------------

#[given(regex = r"^a sim\.json envelope where schema_version has been tampered to 999$")]
fn given_schema_version_tampered_to_999(world: &mut UatWorld) {
    let path = produce_real_sim_json("schema-uat1");
    let mut value = read_json(&path);
    value["schema_version"] = serde_json::json!(999);
    write_json(&path, &value);
    world.sim_json_path = Some(path);
}

// Shared When for UAT-1 and UAT-4 — reads world.sim_json_path, which each
// scenario's own Given populates. Backtick-delimited; distinct from
// sim_json_roundtrips_zero_force_layer.rs:273's double-quoted form by one
// delimiter character (see module doc REGEX DISTINCTNESS).
#[when(regex = r"^the user invokes `resinsim report health --in <PATH>`$")]
fn when_invoke_report_health(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("scenario invariant: Given populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("sim.json path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with non-zero code` — module-1 registration
// (cli_sim_producer_writes_sim_json.rs); reused here, no registration.

#[then(regex = r#"^stderr mentions "unknown schema_version"$"#)]
fn then_stderr_mentions_unknown_schema_version(world: &mut UatWorld) {
    assert_only_branch(world, RejectBranch::Version);
}

#[then(regex = r#"^stderr mentions "999"$"#)]
fn then_stderr_mentions_999(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("999"),
        "expected stderr to name the rejected version, got: {stderr}"
    );
    // simulation_repo.rs:658-663's format string names both the rejected
    // and the expected version.
    assert!(
        stderr.contains("(expected 2)"),
        "expected stderr to name the expected version, got: {stderr}"
    );
    // Discriminates 999 from the schema_version == 1 hint sub-branch
    // (simulation_repo.rs:653-657) — that hint must NOT fire here.
    assert!(
        !stderr.contains(" — v1 files are no longer supported"),
        "the v1 hint sub-branch must not fire for a schema_version=999 envelope: {stderr}"
    );
}

#[then(regex = r#"^the process does not panic \(no "thread 'main' panicked" in stderr\)$"#)]
fn then_does_not_panic_long_form(world: &mut UatWorld) {
    // invoke_resinsim already env_removes RUST_BACKTRACE
    // (cli_fixtures.rs:157-160) so an ambient setting cannot fake a hit.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
        "expected no panic in stderr, got: {stderr}"
    );
}

// ---- UAT-2: missing input file mentions the path ----------------------------

#[when(regex = r"^the user invokes `resinsim report health --in /no/such/file\.sim\.json`$")]
fn when_invoke_report_health_missing_file(world: &mut UatWorld) {
    // The literal absolute path is used verbatim — it needs no temp dir
    // because the scenario asserts its absence.
    let outcome = invoke_resinsim(
        &["report", "health", "--in", "/no/such/file.sim.json"],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with non-zero code` — reused, module-1
// registration.

#[then(regex = r#"^stderr mentions "/no/such/file\.sim\.json"$"#)]
fn then_stderr_mentions_missing_path(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("/no/such/file.sim.json"),
        "expected stderr to mention the missing path, got: {stderr}"
    );
}

#[then(regex = r#"^stderr contains "failed to read"$"#)]
fn then_stderr_contains_failed_to_read(world: &mut UatWorld) {
    assert_only_branch(world, RejectBranch::Read);
}

// ---- UAT-3: malformed JSON surfaces parse error -----------------------------

#[given(regex = r#"^a file at /tmp/garbage\.sim\.json containing the bytes "this is not json"$"#)]
fn given_garbage_json_file(world: &mut UatWorld) {
    let dir = unique_tmp_dir("schema-uat3");
    let path = dir.join("garbage.sim.json");
    std::fs::write(&path, "this is not json")
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    world.sim_json_path = Some(path);
}

// Distinct regex from UAT-1/UAT-4's `<PATH>` form and UAT-2's literal
// `/no/such/file.sim.json`.
#[when(regex = r"^the user invokes `resinsim report health --in /tmp/garbage\.sim\.json`$")]
fn when_invoke_report_health_garbage(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("scenario invariant: Given populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("garbage file path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with non-zero code` — reused, module-1
// registration.

#[then(regex = r#"^stderr contains "failed to parse"$"#)]
fn then_stderr_contains_failed_to_parse(world: &mut UatWorld) {
    // "failed to read" is a genuine near-miss here (both start "failed to
    // "), which is precisely why assert_only_branch's absent-siblings
    // check matters.
    assert_only_branch(world, RejectBranch::Parse);
}

#[then(regex = r"^the process does not panic$")]
fn then_does_not_panic_short_form(world: &mut UatWorld) {
    // Separate registration from UAT-1's parenthesised long form — two
    // different sentences in the spec, two regexes (see module doc
    // CROSS-SPEC LEAKAGE note for why this is also safe to reuse in
    // cli-sim-budget-mismatch-on-load).
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
        "expected no panic in stderr, got: {stderr}"
    );
}

// ---- UAT-4: tampered child entity surfaces "invalid simulation" ------------

#[given(
    regex = r"^a sim\.json with valid schema_version=2 but recipe\.layer_height_um set to -1\.0$"
)]
fn given_valid_schema_but_invalid_recipe(world: &mut UatWorld) {
    // Precedent that this yields the validate branch and not a parse
    // failure: simulation_repo.rs::load_validates_child_entities and
    // print_simulation.rs::validate_returns_err_when_recipe_invalid, which
    // assert the error contains "recipe" and "layer_height_um".
    let path = produce_real_sim_json("schema-uat4");
    let mut value = read_json(&path);
    value["simulation"]["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
    write_json(&path, &value);
    world.sim_json_path = Some(path);
}

// Shared When with UAT-1 — see `when_invoke_report_health` above.

// `Then the process exits with non-zero code` — reused, module-1
// registration.

#[then(regex = r#"^stderr contains "invalid simulation"$"#)]
fn then_stderr_contains_invalid_simulation(world: &mut UatWorld) {
    assert_only_branch(world, RejectBranch::Validate);
}

#[then(regex = r#"^stderr contains "layer_height_um"$"#)]
fn then_stderr_contains_layer_height_um(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("layer_height_um"),
        "expected stderr to name the invalid field, got: {stderr}"
    );
    // Discriminates the aggregate-validate branch (simulation_repo.rs:
    // 667-669) from the provenance-validate branch (:673-675) — the
    // nearest neighbour, which would otherwise also satisfy a naive
    // contains("invalid").
    assert!(
        !stderr.contains("invalid provenance"),
        "must be the aggregate-validate branch, not the provenance-validate branch: {stderr}"
    );
}
