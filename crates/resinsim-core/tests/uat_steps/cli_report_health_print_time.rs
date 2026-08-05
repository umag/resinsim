//! Step definitions for `spec/uat/cli-report-health-print-time.md`
//! UAT-1..UAT-3 (uat-unskip-c2, plan step 2).
//!
//! SYMBOL VERIFICATION. Every scenario here drives `resinsim report health
//! --in <PATH> [--json]` — `cmd_report_health` (main.rs:1687) -> `load_envelope`
//! (simulation_repo.rs) -> `ReportGenerator::text_format` / `json_format`
//! (report_generator.rs:55 / :162) -> `PrintSimulation::summary`
//! (print_simulation.rs:515) -> its private `phase_times`
//! (print_simulation.rs:609) -> `LayerTimingCalculator::cumulative_times_sec`
//! (layer_timing_calculator.rs). None of `cmd_report_health`, `text_format`,
//! `json_format`, `summary`, `phase_times`, or `LayerTimingCalculator`
//! itself carries a `#[cfg(feature = "field-sim")]` attribute (zero
//! occurrences in `report_generator.rs` and `layer_timing_calculator.rs`;
//! `print_simulation.rs` carries 40 `cfg(feature` occurrences but none on
//! `summary`/`phase_times` — they are all on the voxel-field struct fields
//! declared elsewhere in the same file). The producer side —
//! `cmd_sim` (main.rs:1820) — is likewise `#[cfg]`-free, and
//! `ensure_resinsim_built` (`cli_fixtures.rs:64-98`) builds the subprocessed
//! `resinsim` binary with `--bin resinsim -p resinsim-inspect` and no
//! `--features`, so the binary under test is byte-identical under `cargo
//! uat` and `cargo uat-field-sim`. All nine scenario-steps in this module
//! are therefore reachable, identically, in both configs.
//!
//! ENTRY POINT. Every scenario subprocesses the REAL `resinsim` binary via
//! `invoke_resinsim` (`cli_fixtures.rs`) for both the producer Given(s) and
//! the consumer When(s) — never an in-process call. Every fixture path is
//! rooted in its own `fixtures::unique_tmp_dir` — cucumber runs a feature's
//! scenarios concurrently, so a fixed filename would race.
//!
//! "Never hand-serialized JSON": every `sim.json` on disk here is produced
//! by a real `resinsim sim` subprocess; test-side assertions parse the
//! CLI's own stdout (text or `--json`) or read the produced file back with
//! `serde_json::Value` — never a hand-built envelope.
//!
//! MIRRORING HAZARD (UAT-2's phase-sum Then). `PrintSimulation::phase_times`
//! computes `(total, bottom, transition, normal)` as a telescoping
//! difference of one cumulative series (`cumulative[i] - cumulative[j]`), so
//! a test-side re-derivation from recipe fields / lift kinematics /
//! `LayerTimingCalculator` would restate the production formula and assert
//! nothing (`docs/patterns/anti/test-mirrors-production-formula.md`). The
//! phase-sum Then below relates PRODUCTION'S OWN four printed numbers to
//! each other (read from the same `--json` response), with the honest
//! second observation being an independent production surface: a second,
//! text-mode invocation of the SAME envelope, cross-checked against the
//! JSON's `total_time_sec`.
//!
//! STDERR. The KB-153 advisory (`profile_loader::warn_if_envelope_ea_is_default`)
//! prints on stderr for every run here — no shipped resin TOML carries a
//! measured `cure_kinetics_ea_kj_mol`. No assertion in this module touches
//! stderr content or absence; stdout (the surface every Then reads) is
//! unaffected by the advisory.
//!
//! REGEX DISTINCTNESS. This module's three Whens are all UNQUOTED — `the
//! user invokes resinsim report health --in cube.sim.json`, `... --in
//! <PATH.sim.json> --json`, `... --in <each>.sim.json --json` — no
//! backticks, no double quotes. That is what keeps them distinct from the
//! tree's four existing `report health --in` registrations:
//! `cli_sim_rejects_unknown_schema_version.rs`'s two BACKTICK-delimited
//! Whens (`` `resinsim report health --in <PATH>` `` and its two literal-
//! path siblings) and `sim_json_roundtrips_zero_force_layer.rs`'s two
//! DOUBLE-QUOTE-delimited Whens (`"resinsim report health --in <PATH>"` /
//! `"... --json"`). Every literal `.` in a `.sim.json` suffix is escaped
//! (`\.`) below so an unescaped wildcard cannot make two Whens ambiguous.
//! `^the process exits with code 0$` (UAT-1's fifth Then) is owned by
//! `ctb_layer_height_authority.rs`'s generalised `then_exit_zero` — reused,
//! not re-registered; this module's Givens never populate
//! `world.sim_primary` / `world.last_sim_err`, which would trip that step's
//! observation-mode XOR guard.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

/// True if `s` matches production's `H:MM:SS` shape
/// (`app::formatters::format_duration_hms`): unpadded hours, zero-padded
/// MM/SS. A structural check on the OBSERVED text — never a recomputed
/// expected value — so "contains a colon-separated triple" cannot be
/// satisfied by an unrelated substring.
fn looks_like_hms(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 3
        && !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].len() == 2
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[2].len() == 2
        && parts[2].chars().all(|c| c.is_ascii_digit())
}

/// Parse an `H:MM:SS` string (production's `format_duration_hms` shape)
/// back to seconds, for the UAT-2 text-vs-JSON cross-surface check. Returns
/// `None` on any shape mismatch rather than panicking, so the caller can
/// produce a message naming the offending text.
fn parse_hms_to_secs(s: &str) -> Option<f64> {
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Parse `world.cli_stdout` as `serde_json::Value` — reads the CLI's own
/// `--json` response, never a hand-built `Value`.
fn parsed_json_stdout(world: &UatWorld) -> serde_json::Value {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("parse stdout as JSON: {e}\nstdout: {stdout}"))
}

// ---- UAT-1: human output contains Total time + per-phase breakdown --------

#[given(
    regex = r"^the resinsim sim subcommand has produced cube\.sim\.json from an STL with --printer elegoo_mars5_ultra \+ --resin elegoo_ceramic_grey_v2 \+ --n-supports 0$"
)]
fn given_producer_has_produced_cube_sim_json(world: &mut UatWorld) {
    let dir = unique_tmp_dir("print-time-uat1");
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = dir.join("cube.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("workspace data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: producer run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    world.cli_tmp_dir = Some(dir);
    world.sim_json_path = Some(out);
}

#[when(regex = r"^the user invokes resinsim report health --in cube\.sim\.json$")]
fn when_invoke_report_health_uat1(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path");
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

#[then(regex = r#"^stdout contains the line "Total time:"$"#)]
fn then_stdout_contains_total_time_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("Total time:"))
        .unwrap_or_else(|| panic!("expected a 'Total time:' line in stdout, got:\n{stdout}"));
    // Second observation: the value after the label parses as production's
    // own H:MM:SS shape.
    let value = line
        .trim_start()
        .strip_prefix("Total time:")
        .expect("checked by starts_with above")
        .trim();
    assert!(
        looks_like_hms(value),
        "expected the Total time value to look like H:MM:SS, got {value:?} in line {line:?}"
    );
}

/// Shared by the `bottom:` / `transition:` / `normal:` Thens: locate the
/// labelled line, assert it appears AFTER the `Total time:` header line
/// (so "contains" cannot be satisfied by an unrelated line elsewhere in
/// the report), and assert its value parses as production's H:MM:SS shape.
fn assert_phase_line_after_total(stdout: &str, label: &str) {
    let lines: Vec<&str> = stdout.lines().collect();
    let total_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with("Total time:"))
        .unwrap_or_else(|| {
            panic!("expected a 'Total time:' line before the phase breakdown, got:\n{stdout}")
        });
    let phase_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(label))
        .unwrap_or_else(|| panic!("expected a {label:?} line in stdout, got:\n{stdout}"));
    assert!(
        phase_idx > total_idx,
        "expected the {label:?} line to appear AFTER 'Total time:', got:\n{stdout}"
    );
    let value = lines[phase_idx]
        .trim_start()
        .strip_prefix(label)
        .expect("checked by starts_with above")
        .trim();
    assert!(
        looks_like_hms(value),
        "expected the {label:?} value to look like H:MM:SS, got {value:?} in line {:?}",
        lines[phase_idx]
    );
}

#[then(regex = r#"^stdout contains the line "bottom:"$"#)]
fn then_stdout_contains_bottom_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert_phase_line_after_total(stdout, "bottom:");
}

#[then(regex = r#"^stdout contains the line "transition:"$"#)]
fn then_stdout_contains_transition_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert_phase_line_after_total(stdout, "transition:");
}

#[then(regex = r#"^stdout contains the line "normal:"$"#)]
fn then_stdout_contains_normal_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert_phase_line_after_total(stdout, "normal:");
}

// `Then the process exits with code 0` — served by
// ctb_layer_height_authority.rs's generalised then_exit_zero; no
// registration here (UAT-1's fifth Then).

// ---- UAT-2: --json phase fields sum to total, within tolerance -----------

#[given(regex = r"^a sim\.json envelope produced by `resinsim sim` against shipped profiles$")]
fn given_shipped_profile_envelope(world: &mut UatWorld) {
    let dir = unique_tmp_dir("print-time-uat2");
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = dir.join("cube.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("workspace data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: producer run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    world.cli_tmp_dir = Some(dir);
    world.sim_json_path = Some(out);
}

#[when(regex = r"^the user invokes resinsim report health --in <PATH\.sim\.json> --json$")]
fn when_invoke_report_health_json_uat2(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("sim.json path is UTF-8"),
            "--json",
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the JSON summary object has a numeric total_time_sec > 0$")]
fn then_json_summary_total_time_sec_positive(world: &mut UatWorld) {
    let value = parsed_json_stdout(world);
    let total = value["summary"]["total_time_sec"]
        .as_f64()
        .unwrap_or_else(|| panic!("expected numeric summary.total_time_sec, got: {value}"));
    assert!(total > 0.0, "expected total_time_sec > 0, got {total}");
}

#[then(
    regex = r"^the JSON summary has numeric bottom_time_sec, transition_time_sec, normal_time_sec$"
)]
fn then_json_summary_numeric_phase_fields(world: &mut UatWorld) {
    let value = parsed_json_stdout(world);
    for key in ["bottom_time_sec", "transition_time_sec", "normal_time_sec"] {
        assert!(
            value["summary"][key].as_f64().is_some(),
            "expected numeric summary.{key}, got: {value}"
        );
    }
}

#[then(
    regex = r"^bottom_time_sec \+ transition_time_sec \+ normal_time_sec equals total_time_sec within 0\.1% tolerance$"
)]
fn then_phase_sum_equals_total_within_tolerance(world: &mut UatWorld) {
    let value = parsed_json_stdout(world);
    let summary = &value["summary"];
    let total = summary["total_time_sec"]
        .as_f64()
        .expect("prior Then already asserted this is numeric");
    let bottom = summary["bottom_time_sec"]
        .as_f64()
        .expect("prior Then already asserted this is numeric");
    let transition = summary["transition_time_sec"]
        .as_f64()
        .expect("prior Then already asserted this is numeric");
    let normal = summary["normal_time_sec"]
        .as_f64()
        .expect("prior Then already asserted this is numeric");
    let sum = bottom + transition + normal;
    // Production-number-vs-production-number only: relates the JSON's own
    // four printed fields to each other. Tolerance formula matches
    // report_health_time_cli.rs's `report_health_json_includes_print_time_fields`
    // exactly. Never re-derived from recipe fields, layer counts, or lift
    // kinematics — see the module doc's MIRRORING HAZARD note.
    let tol = (total.abs() * 1e-3).max(1e-6);
    assert!(
        (sum - total).abs() < tol,
        "phase sum {sum} should equal total {total} within {tol}"
    );

    // Second, independent observation: re-invoke the SAME envelope WITHOUT
    // --json and compare the text-mode "Total time:" line against the
    // JSON's total_time_sec — two independent production surfaces
    // cross-checked, no formula duplicated.
    let path = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("sim.json path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0,
        "text-mode cross-check invocation must succeed: stderr={}",
        outcome.stderr
    );
    let line = outcome
        .stdout
        .lines()
        .find(|l| l.trim_start().starts_with("Total time:"))
        .unwrap_or_else(|| {
            panic!(
                "expected a 'Total time:' line in text-mode stdout, got:\n{}",
                outcome.stdout
            )
        });
    let text_value = line
        .trim_start()
        .strip_prefix("Total time:")
        .expect("checked by starts_with above")
        .trim();
    let text_secs = parse_hms_to_secs(text_value)
        .unwrap_or_else(|| panic!("could not parse H:MM:SS from {text_value:?}"));
    assert!(
        (text_secs - total).abs() < 1.0,
        "text-mode Total time ({text_secs}s, from {text_value:?}) must agree with JSON \
         total_time_sec ({total}s) within 1s"
    );
}

// ---- UAT-3: Tilt total_time_sec < Linear total_time_sec on factory defaults

#[given(
    regex = r"^two sim\.json envelopes from `resinsim sim` against the same STL and resin profile$"
)]
fn given_two_envelopes_premise(world: &mut UatWorld) {
    // Preamble Given: allocates the shared unique dir + STL the two `And`
    // Givens below produce their envelopes into/from. Populates no
    // observation-mode field.
    let dir = unique_tmp_dir("print-time-uat3");
    world.cli_tmp_dir = Some(dir);
}

#[given(regex = r"^the first produced with --printer elegoo_mars5_ultra \(Tilt\)$")]
fn given_tilt_envelope(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("pair-premise Given populated cli_tmp_dir");
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = dir.join("tilt.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "generic_standard",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("workspace data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: Tilt producer run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    let mut envs = world.print_time_envelope_paths.take().unwrap_or_default();
    envs.push(out);
    world.print_time_envelope_paths = Some(envs);
}

#[given(regex = r"^the second produced with --printer generic_msla_4k \(Linear\)$")]
fn given_linear_envelope(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("pair-premise Given populated cli_tmp_dir");
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = dir.join("linear.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--printer",
            "generic_msla_4k",
            "--resin",
            "generic_standard",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("workspace data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: Linear producer run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    let mut envs = world.print_time_envelope_paths.take().unwrap_or_default();
    envs.push(out);
    world.print_time_envelope_paths = Some(envs);
}

#[when(regex = r"^the user invokes resinsim report health --in <each>\.sim\.json --json$")]
fn when_invoke_report_health_json_both(world: &mut UatWorld) {
    let envs = world
        .print_time_envelope_paths
        .clone()
        .expect("Given populated the Tilt+Linear envelope pair");
    assert_eq!(
        envs.len(),
        2,
        "expected exactly two envelopes (Tilt, Linear), got {}",
        envs.len()
    );
    let mut stdouts = Vec::with_capacity(2);
    for path in &envs {
        let outcome = invoke_resinsim(
            &[
                "report",
                "health",
                "--in",
                path.to_str().expect("envelope path is UTF-8"),
                "--json",
            ],
            &[],
        );
        assert!(
            outcome.exit_code == 0,
            "report health --json must succeed for {}: stderr={}",
            path.display(),
            outcome.stderr
        );
        stdouts.push(outcome.stdout);
    }
    world.print_time_json_stdouts = Some(stdouts);
}

#[then(regex = r"^the Tilt total_time_sec is strictly less than the Linear total_time_sec$")]
fn then_tilt_total_time_less_than_linear(world: &mut UatWorld) {
    let stdouts = world
        .print_time_json_stdouts
        .clone()
        .expect("When populated print_time_json_stdouts");
    let tilt_json: serde_json::Value =
        serde_json::from_str(&stdouts[0]).expect("Tilt stdout is valid JSON");
    let linear_json: serde_json::Value =
        serde_json::from_str(&stdouts[1]).expect("Linear stdout is valid JSON");
    let tilt_total = tilt_json["summary"]["total_time_sec"]
        .as_f64()
        .expect("Tilt total_time_sec is numeric");
    let linear_total = linear_json["summary"]["total_time_sec"]
        .as_f64()
        .expect("Linear total_time_sec is numeric");
    // Direction only — never the magnitude of the ratio. The spec's 39.7%
    // figure (Lilith Torso, 4492 layers) is an empirical band for a
    // different model, not a contract this scenario's ~200-layer STL must
    // reproduce.
    assert!(
        tilt_total < linear_total,
        "expected Tilt total_time_sec ({tilt_total}) < Linear total_time_sec ({linear_total})"
    );

    // Discrimination cross-check: the two envelopes name genuinely
    // different printers, so a bug that produced the same envelope twice
    // cannot pass vacuously. `report health --json` does not surface
    // printer_name, so this reads provenance straight from the two
    // produced sim.json files.
    let envs = world
        .print_time_envelope_paths
        .clone()
        .expect("Given populated the Tilt+Linear envelope pair");
    let tilt_env: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&envs[0]).expect("read Tilt envelope"),
    )
    .expect("Tilt envelope is valid JSON");
    let linear_env: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&envs[1]).expect("read Linear envelope"),
    )
    .expect("Linear envelope is valid JSON");
    let tilt_printer = tilt_env["provenance"]["printer_name"].as_str();
    let linear_printer = linear_env["provenance"]["printer_name"].as_str();
    assert_ne!(
        tilt_printer, linear_printer,
        "expected the Tilt and Linear envelopes to name different printers (proves the pair \
         wasn't accidentally duplicated); got Tilt={tilt_printer:?} Linear={linear_printer:?}"
    );
    assert_eq!(tilt_printer, Some("Elegoo Mars 5 Ultra"));
    assert_eq!(linear_printer, Some("Generic MSLA 4K"));
}
