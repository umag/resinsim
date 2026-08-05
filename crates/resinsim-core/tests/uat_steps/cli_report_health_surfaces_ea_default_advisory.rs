//! Step definitions for
//! `spec/uat/cli-report-health-surfaces-ea-default-advisory.md` UAT-1..UAT-3
//! (uat-unskip-c2, plan step 4). A NEW spec authored in the same change as
//! this module — a spec without a module fails layer 1 immediately, and a
//! module without the spec fails layer 3's rename-resolution check, so they
//! are one unit (no `SPECS_WITHOUT_STEP_DEFS` entry is needed or added: the
//! spec ships fully stepped).
//!
//! WHY A UAT AT ALL, WHEN NEXTEST ALREADY COVERS THIS. This module is
//! deliberately NOT a duplicate of
//! `crates/resinsim-inspect/tests/thermal_cli_warnings.rs`'s four
//! `report_health_*` tests (`report_health_warns_when_envelope_flags_ea_default`,
//! `report_health_warns_exactly_once`,
//! `report_health_silent_when_envelope_flags_measured_ea`,
//! `report_health_silent_on_pre_flag_envelope`) or
//! `report_health_time_cli.rs`. Those nextest twins already pin the
//! plumbing (exact literals, exact counts, the flag's serde shape); this
//! spec earns its place by asserting the USER-VISIBLE CLI contract — which
//! STREAM carries what, the exactly-once property, and the three-way flag
//! semantics — as an executable UAT per
//! `agent-constraints/uat-conventions.md`, harvesting behaviour that
//! previously lived only in nextest.
//!
//! SYMBOL VERIFICATION. Every scenario drives `resinsim report health --in
//! <EA_ENVELOPE>` — `cmd_report_health` (main.rs:1687) -> `load_envelope` ->
//! `profile_loader::warn_if_envelope_ea_is_default`
//! (`crates/resinsim-inspect/src/profile_loader.rs`), called unconditionally
//! on the `Ok` arm before any stdout rendering, no `#[cfg]` anywhere on the
//! function. The wording is owned by
//! `cure_kinetics_ea_default_warning_text` (same file), a pure policy
//! function with its own byte-identity unit test — this module asserts
//! stable SUBSTRINGS of that text (the three needles + the consumer-context
//! phrase), never the whole sentence, so a future re-wrap of the literal
//! does not falsely fail. The producer side — `cmd_sim` (main.rs:1820) ->
//! `profile_loader::resolve_profiles` -> `load_resin` — is the twin seam
//! that stamps `cure_kinetics_ea_is_default` into the envelope; likewise
//! `#[cfg]`-free. `ensure_resinsim_built` (`cli_fixtures.rs:64-98`) builds
//! the subprocessed binary with no `--features`, so the binary under test
//! is byte-identical under `cargo uat` and `cargo uat-field-sim`.
//!
//! ENTRY POINT. Every Given produces a REAL envelope via a real `resinsim
//! sim` subprocess (`invoke_resinsim`, `cli_fixtures.rs`); UAT-3 additionally
//! tampers the produced file through a parsed `serde_json::Value` (the
//! provenance-strip technique `report_health_time_cli.rs`'s
//! `report_health_silent_on_pre_flag_envelope` already uses) — never a
//! hand-serialised envelope.
//!
//! UAT-2 FIXTURE TECHNIQUE. Builds a temp data dir by PARSING
//! `data/resins/generic_standard.toml` into a `toml::Table`, inserting
//! `cure_kinetics_ea_kj_mol` as a ROOT key, and serialising back —
//! `cli_temperature_flag_validation.rs::given_measured_ea_cure`'s
//! technique, strictly better than `thermal_cli_warnings.rs`'s
//! string-replace-on-`"[recipe]"`. Rooted in `fixtures::unique_tmp_dir`, NOT
//! that step's fixed `CARGO_TARGET_TMPDIR/uat-measured-ea` path, which would
//! race under cucumber's per-feature concurrent scenarios. Printer TOMLs
//! are copied across unchanged. This mutates the SHIPPED profile rather
//! than hand-authoring a TOML literal, so
//! `docs/patterns/anti/fixture-copy-of-shared-builder.md` is satisfied
//! without going through `ResinBuilder` (whose defaults intentionally do
//! NOT carry a measured Ea).
//!
//! REGEX DISTINCTNESS. The shared When,
//! `` `resinsim report health --in <EA_ENVELOPE>` `` (backtick-delimited),
//! uses the distinct placeholder `<EA_ENVELOPE>` so it cannot collide with
//! the tree's existing backtick- and double-quote-delimited `report health
//! --in` Whens (see `cli_report_health_print_time.rs`'s and
//! `cli_report_health_layer_height_provenance.rs`'s own REGEX DISTINCTNESS
//! notes for the full inventory). The three near-miss NEIGHBOURS, worded
//! deliberately differently because they are different assertions on a
//! different subcommand: `cli_temperature_flag_validation.rs`'s
//! `^stderr contains the strings "30 kJ/mol", "literature midpoint
//! estimate", and "KB-153"$` (this module: "stderr carries the needles...");
//! its `^stderr does NOT contain "30 kJ/mol"$` (this module: "stderr
//! carries no KB-153 needle"); and
//! `cli_sim_rejects_unknown_schema_version.rs`'s internal
//! `ADVISORY_NEEDLES`-based absence assertions inside `assert_only_branch`
//! (not their own Gherkin Thens, but the same four-needle set — cited here
//! as the negative-control complement of this module's UAT-1 positive
//! assertions: that module proves the advisory does NOT fire on four
//! rejection branches; this module's UAT-1 proves it DOES fire, exactly
//! once, on the success path). `Then the process exits with code 0` (all
//! three scenarios) is `ctb_layer_height_authority.rs`'s shared
//! registration — reused, not re-registered; this module's Givens never
//! populate `world.sim_primary` / `world.last_sim_err`.
//!
//! STREAM-SEPARATION CONTRACT (UAT-1's final Then). Matches
//! `report_health_time_cli.rs::report_health_json_stdout_unchanged_by_advisory`'s
//! claim generalised to text mode: the advisory is stderr-only, and stdout
//! carries none of the four needles.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

/// The KB-153 advisory needles this module positively asserts (UAT-1) and
/// negatively asserts (UAT-2/UAT-3) — same four-needle set
/// `cli_sim_rejects_unknown_schema_version.rs`'s `ADVISORY_NEEDLES` uses,
/// duplicated locally (not shared) since that module's const is private to
/// it. See the module doc's REGEX DISTINCTNESS note for the negative-
/// control relationship between the two modules.
const ADVISORY_NEEDLES: [&str; 4] = [
    "30 kJ/mol",
    "literature midpoint estimate",
    "KB-153",
    "sim.json envelope",
];

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

/// Produce a real `sim.json` via a real `resinsim sim` subprocess against
/// `--resin generic_standard --printer generic_msla_4k` (the pair
/// `thermal_cli_warnings.rs` / `report_health_time_cli.rs` already use for
/// their KB-153 fixtures) under `data_dir`, into a fresh
/// `fixtures::unique_tmp_dir`. Never hand-serialised.
fn produce_real_sim_json(tag: &str, data_dir: &std::path::Path) -> std::path::PathBuf {
    let dir = unique_tmp_dir(tag);
    let stl = workspace_data_dir().join("test_cube.stl");
    let out = dir.join("cube.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
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

/// Build a temp data dir whose `resins/generic_standard.toml` carries a
/// measured `cure_kinetics_ea_kj_mol` at the ROOT level — parse-then-
/// rewrite (never a string-replace-on-`"[recipe]"`), same technique as
/// `cli_temperature_flag_validation.rs::given_measured_ea_cure`, but rooted
/// in `unique_tmp_dir` rather than that step's fixed
/// `CARGO_TARGET_TMPDIR/uat-measured-ea` path — this scenario runs
/// concurrently with its siblings under cucumber's per-feature scheduling,
/// and a fixed path would race. Printer TOMLs are copied across unchanged
/// (`report health`'s producer run still needs to resolve a printer).
fn measured_ea_data_dir(tag: &str, ea_kj_mol: f32) -> std::path::PathBuf {
    let dir = unique_tmp_dir(tag);
    let resins = dir.join("resins");
    let printers = dir.join("printers");
    std::fs::create_dir_all(&resins).expect("mkdir resins");
    std::fs::create_dir_all(&printers).expect("mkdir printers");

    let src = workspace_data_dir();
    for entry in std::fs::read_dir(src.join("printers")).expect("read_dir printers") {
        let entry = entry.expect("printers dir entry");
        std::fs::copy(entry.path(), printers.join(entry.file_name()))
            .expect("copy printer TOML unchanged");
    }

    let src_toml = std::fs::read_to_string(src.join("resins/generic_standard.toml"))
        .expect("read generic_standard.toml");
    let mut parsed: toml::Table =
        toml::from_str(&src_toml).expect("source generic_standard.toml must be valid TOML");
    parsed.insert(
        "cure_kinetics_ea_kj_mol".to_string(),
        toml::Value::Float(ea_kj_mol as f64),
    );
    let patched =
        toml::to_string(&parsed).expect("serialise patched generic_standard.toml back to TOML");
    std::fs::write(resins.join("generic_standard.toml"), &patched)
        .expect("write patched generic_standard.toml");
    // Sanity: the rewrite round-trips back to a valid ResinProfile, so a
    // re-serialise bug surfaces here rather than via a confusing downstream
    // CLI failure.
    let _: resinsim_core::entities::ResinProfile =
        toml::from_str(&patched).expect("patched TOML must round-trip back to ResinProfile");

    dir
}

// ---- UAT-1: warns exactly once on a flagged default-Ea envelope -----------

#[given(
    regex = r"^a sim\.json produced by a real `resinsim sim` against shipped profiles, whose cure_kinetics_ea_is_default is true$"
)]
fn given_flagged_default_ea_envelope(world: &mut UatWorld) {
    let data = workspace_data_dir();
    let out = produce_real_sim_json("ea-advisory-uat1", &data);
    // Premise check: no shipped resin TOML carries a measured Ea, so the
    // producer must have stamped the flag true. A producer regression here
    // surfaces as a loud fixture panic, not a silently-wrong Then later.
    let value = read_json(&out);
    assert_eq!(
        value["cure_kinetics_ea_is_default"],
        serde_json::json!(true),
        "scenario premise: expected the producer to stamp cure_kinetics_ea_is_default \
         true for a default-Ea shipped resin, got: {value}"
    );
    world.sim_json_path = Some(out);
}

#[when(regex = r"^the user invokes `resinsim report health --in <EA_ENVELOPE>`$")]
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

// `Then the process exits with code 0` — ctb_layer_height_authority.rs's
// generalised then_exit_zero; no registration here (all three scenarios).

#[then(
    regex = r#"^stderr carries the needles "30 kJ/mol", "literature midpoint estimate", and "KB-153"$"#
)]
fn then_stderr_carries_the_three_needles(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    for needle in ["30 kJ/mol", "literature midpoint estimate", "KB-153"] {
        assert!(
            stderr.contains(needle),
            "expected stderr to carry {needle:?}, got:\n{stderr}"
        );
    }
}

#[then(
    regex = r#"^stderr carries the consumer-context line naming "sim\.json envelope" and the "resinsim sim" remedy$"#
)]
fn then_stderr_carries_consumer_context_line(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("sim.json envelope") && stderr.contains("resinsim sim"),
        "expected stderr to name that the fact came from the envelope and that the remedy is \
         re-running `resinsim sim`, got:\n{stderr}"
    );
}

#[then(regex = r"^the advisory appears exactly once$")]
fn then_advisory_appears_exactly_once(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // Per-surface detection shape mandated by
    // docs/patterns/anti/warning-duplicated-per-subcommand.md.
    assert_eq!(
        stderr.matches("KB-153").count(),
        1,
        "expected the advisory to appear exactly once on the consumer path, got:\n{stderr}"
    );
}

#[then(regex = r"^stdout carries the health report and none of the KB-153 needles$")]
fn then_stdout_carries_report_and_no_needles(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // Stream-separation contract: the advisory is stderr-only.
    assert!(
        stdout.contains("Print health report:"),
        "expected stdout to still carry the health report header, got:\n{stdout}"
    );
    for needle in ADVISORY_NEEDLES {
        assert!(
            !stdout.contains(needle),
            "expected stdout to carry NONE of the KB-153 needles (advisory is stderr-only), \
             found {needle:?} in:\n{stdout}"
        );
    }
}

// ---- UAT-2: silent on a measured-Ea envelope -------------------------------

#[given(regex = r"^a sim\.json produced against a resin profile with a measured cure_kinetics_ea_kj_mol$")]
fn given_measured_ea_envelope(world: &mut UatWorld) {
    let data = measured_ea_data_dir("ea-advisory-uat2", 45.0);
    let out = produce_real_sim_json("ea-advisory-uat2-out", &data);
    world.sim_json_path = Some(out);
}

// Shared When — see UAT-1's when_invoke_report_health.

// `Then the process exits with code 0` — shared registration, see UAT-1.

#[then(regex = r"^stderr carries no KB-153 needle$")]
fn then_stderr_carries_no_kb153_needle(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("KB-153") && !stderr.contains("30 kJ/mol"),
        "expected stderr to carry NO KB-153 needle, got:\n{stderr}"
    );
}

#[then(regex = r"^the envelope's cure_kinetics_ea_is_default is false$")]
fn then_envelope_flag_is_false(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path");
    let value = read_json(&path);
    // The polarity that makes "silent" mean "the flag said false" (a
    // measured Ea was found), not "the producer forgot to stamp it" —
    // ADR-0002's three-valued contract this spec's Rationale names.
    assert_eq!(
        value["cure_kinetics_ea_is_default"],
        serde_json::json!(false),
        "expected the envelope's own flag to be false (not absent), got: {value}"
    );
}

// ---- UAT-3: silent on a pre-flag envelope (accepted false negative) ------

#[given(regex = r"^a sim\.json envelope whose cure_kinetics_ea_is_default key has been stripped$")]
fn given_pre_flag_envelope(world: &mut UatWorld) {
    let data = workspace_data_dir();
    let out = produce_real_sim_json("ea-advisory-uat3", &data);
    // Tamper through a parsed serde_json::Value — the provenance-strip
    // technique report_health_time_cli.rs's report_health_silent_on_pre_flag_envelope
    // already uses. Never a hand-serialised envelope.
    let mut value = read_json(&out);
    value
        .as_object_mut()
        .expect("envelope root is an object")
        .remove("cure_kinetics_ea_is_default");
    write_json(&out, &value);
    world.sim_json_path = Some(out);
}

// Shared When — see UAT-1's when_invoke_report_health.

// `Then the process exits with code 0` — shared registration, see UAT-1.

// Shared "stderr carries no KB-153 needle" Then with UAT-2 — see
// then_stderr_carries_no_kb153_needle. Same assertion, two different
// reasons (flag false vs flag absent) — UAT-2's companion Then proves
// which by checking the envelope's own flag value; this scenario's premise
// (the key is stripped, i.e. genuinely absent) is proven by the Given
// itself having removed it.

#[then(regex = r"^stdout still carries the health report$")]
fn then_stdout_still_carries_health_report(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains("Print health report:"),
        "expected stdout to still carry the health report header even on a pre-flag envelope, \
         got:\n{stdout}"
    );
}
