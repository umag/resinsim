//! Step definitions for `spec/uat/nanodlp-calibrate-compares-real-force.md`
//! UAT-1..UAT-3 (uat-unskip-a3-b, plan step 7). Largest module in this
//! increment; lands LAST because it depends on the shared job synthesiser
//! (`fixtures::NanoDlpJobBuilder`) proven by `nanodlp_import_simulates.rs`.
//!
//! SYMBOL VERIFICATION. The binary's `inspect calibrate --file [--json]`
//! (`cmd_calibrate`, main.rs:1470-1650) composes
//! `run_simulation_with_optional_voxel` (Tier-1 path when the `field-sim`
//! feature is off) -> `PrintSimulation::total_force_series` ->
//! `io::nanodlp::load_analytic_from_nanodlp` ->
//! `services::ForceSeriesExtractor` -> `services::ForceComparator::compare`
//! -> `services::ProfileCalibrator::calibrate` — all default-features,
//! `#[cfg]`-free on this call path.
//!
//! FIXTURE VERDICT (evidence, not assumption). `mini.nanodlp` is SUFFICIENT
//! for UAT-1 (every Then is satisfied by `cmd_calibrate`'s text branch on
//! it) but INSUFFICIENT for UAT-2/UAT-3:
//! `base_adhesion_shifts_peel_peak.rs` (lines 188-205) records the
//! empirical finding (2026-08-01) that `inspect calibrate` against
//! `mini.nanodlp` reports `offset +0` regardless of the base-adhesion term,
//! and its 0.08/0.04/0.02 mm² monotonically-decreasing predicted series
//! against the embedded log's 400/250/150-count monotonically-decreasing
//! actual series drives `fit_quality` to ~0.87 — well above the < 0.5
//! low-fit threshold. Both UAT-2's and UAT-3's Given premises would be
//! FALSE on `mini.nanodlp`. [`late_peaking_variant`] below is the ONE
//! synthesised fixture serving both (two distinct `#[given]` registrations,
//! one builder output), confirmed SUFFICIENT by a real-CLI probe performed
//! BEFORE any assertion in this module was written (see that function's doc
//! for the recorded numbers).
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory: no
//! collision for any Given/When/Then text below. UAT-1's When ("the user
//! INVOKES ...") is textually distinct from UAT-2/UAT-3's shared When ("the
//! user RUNS ..." — identical text between UAT-2 and UAT-3, registered
//! ONCE). `base_adhesion_shifts_peel_peak.rs`'s `` `inspect calibrate`
//! prints "Predicted base adhesion (layer 0): <N> N" `` is a different
//! literal from every Then here.
//!
//! SPEC-vs-BINARY MISMATCH (documented, not faked — see UAT-3's third Then
//! below). The spec's parenthetical "(null when a series is empty)" for
//! `peak_layer_offset` describes a branch UNREACHABLE through the CLI:
//! `cmd_calibrate` calls `ForceComparator::compare` first (main.rs:1543),
//! which returns `Err(...)` when either series is empty and exits 1 BEFORE
//! any JSON prints (main.rs:1546-1548). When `compare` succeeds, `n >= 1`,
//! so `predicted_peak_layer`/`actual_peak_layer` are always `Some` and
//! `peak_layer_offset` is therefore never null on any REACHABLE `--json`
//! output. This Then asserts the three keys exist as numbers and that
//! `offset == predicted - actual`; it does NOT synthesise a fake
//! empty-series input to force a null. If full null-branch coverage is
//! wanted, it belongs in an in-process `ForceComparator` test under a
//! different issue.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::{mini_nanodlp_path, BuiltNanoDlpJob, NanoDlpJobBuilder};
use super::world::UatWorld;

/// The 6-layer late-peaking variant shared by UAT-2 and UAT-3 — two
/// distinct `#[given]` registrations (the spec texts differ) both point at
/// this one builder output, per the plan's own instruction. Lit-pixel
/// counts 4/8/16/32/48/64 give a per-layer cross-section area that
/// increases monotonically, so the predicted total force peaks LATE
/// (layer 5). The embedded analytic body is the KB-115-shaped log ALREADY
/// COMMITTED as `tests/fixtures/synthetic_stepped_forces.csv` (peak signals
/// 400/320/240/180/120/0, decaying from layer 0), reused via `include_str!`
/// rather than re-typed — this fixture and the athena module's UAT-1 pin
/// the exact same committed provenance.
///
/// PREMISE PROBE (performed before any assertion in this module was
/// written, per the golden-capture discipline). A throwaway probe built
/// this exact variant and ran the real `inspect calibrate` CLI against it;
/// the real output was:
///
/// ```text
///   Compared 6 layers
///   Correlation (predicted total force vs real peel signal): -0.980
///   Peak layer: predicted 5, real 0 (offset +5)
///   ! peak offset +5 layers — the sim peak sits far from the real
///     base-adhesion peak (KB-115); see ADR-0022
///   peel gain: 0.000006 N per raw count (fit R²=0.000)
///   ! low fit quality — single print; treat as indicative only
/// ```
///
/// `predicted_peak_layer=5`, `actual_peak_layer=0` (offset +5, so
/// `|offset| >= 3`) and `fit R²=0.000` (< 0.5) — both UAT-2's and UAT-3's
/// Given premises hold on REAL output, not by construction.
fn late_peaking_variant(tag: &str) -> BuiltNanoDlpJob {
    const SYNTHETIC_LOG: &str = include_str!("../fixtures/synthetic_stepped_forces.csv");
    NanoDlpJobBuilder::new()
        .with_lit_pixel_counts([4u64, 8, 16, 32, 48, 64])
        .with_analytic_body(SYNTHETIC_LOG)
        .build(tag)
}

/// `(bytes, mtime)` of `data/printers/athena_ii.toml` — UAT-1's "unchanged
/// on disk" Then reads the REAL workspace `data/` tree; safe because no
/// scenario in the suite writes into it (every scenario that mutates a
/// profile builds a copy under `CARGO_TARGET_TMPDIR` —
/// `base_adhesion_shifts_peel_peak.rs::resin_data_dir_without_base_adhesion`
/// is the precedent). This invariant must hold for any future sibling
/// scenario too.
fn printer_toml_snapshot() -> (Vec<u8>, std::time::SystemTime) {
    let path = workspace_data_dir().join("printers").join("athena_ii.toml");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("mtime {}: {e}", path.display()));
    (bytes, mtime)
}

fn run_calibrate_text(world: &mut UatWorld) {
    let fixture = world
        .nanodlp_fixture_path
        .clone()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let data_dir = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "calibrate",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "athena_ii",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// ---- Text-mode parsers over cmd_calibrate's rendered lines -----------------

fn parse_compared_layers(stdout: &str) -> usize {
    stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Compared "))
        .and_then(|rest| rest.split(" layers").next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or_else(|| panic!("could not parse 'Compared N layers' line from stdout: {stdout}"))
}

fn parse_correlation(stdout: &str) -> f64 {
    stdout
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("Correlation (predicted total force vs real peel signal): ")
        })
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or_else(|| panic!("could not parse 'Correlation (...)' line from stdout: {stdout}"))
}

/// Parse `  Peak layer: predicted {p}, real {a} (offset {off:+})`
/// (main.rs:1624). Returns `(predicted, real, offset_str)` — the offset is
/// returned as its RAW rendered string (not just the parsed number) so
/// callers can check the explicit sign flag.
fn parse_peak_layer_line(stdout: &str) -> (i64, i64, String) {
    let line = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Peak layer: predicted "))
        .unwrap_or_else(|| panic!("could not find 'Peak layer: predicted' line in stdout: {stdout}"));
    let mut parts = line.splitn(2, ", real ");
    let p_str = parts
        .next()
        .unwrap_or_else(|| panic!("malformed 'Peak layer:' line: {line}"));
    let rest = parts
        .next()
        .unwrap_or_else(|| panic!("'Peak layer:' line missing ', real ': {line}"));
    let mut rest_parts = rest.splitn(2, " (offset ");
    let a_str = rest_parts
        .next()
        .unwrap_or_else(|| panic!("malformed 'Peak layer:' line: {line}"));
    let off_part = rest_parts
        .next()
        .unwrap_or_else(|| panic!("'Peak layer:' line missing ' (offset ': {line}"));
    let off_str = off_part.trim_end_matches(')').to_string();
    let p: i64 = p_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse predicted peak {p_str:?}: {e}"));
    let a: i64 = a_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse real peak {a_str:?}: {e}"));
    (p, a, off_str)
}

/// Parse `  peel gain: {:.6} N per raw count (fit R²={:.3})` (main.rs:1639).
/// Returns `(gain, fit_r2)`.
fn parse_peel_gain_and_r2(stdout: &str) -> (f64, f64) {
    let line = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("peel gain: "))
        .unwrap_or_else(|| panic!("could not find 'peel gain:' line in stdout: {stdout}"));
    let mut parts = line.splitn(2, " N per raw count (fit R²=");
    let gain_str = parts
        .next()
        .unwrap_or_else(|| panic!("malformed 'peel gain:' line: {line}"));
    let r2_part = parts
        .next()
        .unwrap_or_else(|| panic!("'peel gain:' line missing ' N per raw count (fit R²=': {line}"));
    let r2_str = r2_part.trim_end_matches(')');
    let gain: f64 = gain_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse peel gain {gain_str:?}: {e}"));
    let r2: f64 = r2_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse fit R² {r2_str:?}: {e}"));
    (gain, r2)
}

// ---- UAT-1: calibrate reports a comparison and suggested overrides -------

#[given(regex = r"^a \.nanodlp job containing slice PNGs and an analytic-\*\.csv\.gz force log$")]
fn given_nanodlp_job_with_analytic_log(world: &mut UatWorld) {
    // Committed mini.nanodlp — SUFFICIENT for every UAT-1 Then (see the
    // FIXTURE VERDICT module doc above).
    world.nanodlp_fixture_path = Some(mini_nanodlp_path());
    world.printer_toml_before = Some(printer_toml_snapshot());
}

#[when(regex = r"^the user invokes `resinsim inspect calibrate --file <job\.nanodlp>`$")]
fn when_user_invokes_inspect_calibrate(world: &mut UatWorld) {
    run_calibrate_text(world);
}

#[then(regex = r#"^stderr reports "Simulating <job>"$"#)]
fn then_stderr_reports_simulating(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let fixture = world
        .nanodlp_fixture_path
        .as_ref()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    // main.rs:1511 `eprintln!("Simulating {file} (decoding layer PNGs —
    // slow for large jobs)...")`.
    let needle = format!("Simulating {}", fixture.display());
    assert!(
        stderr.contains(&needle),
        "stderr must report {needle:?}, got: {stderr}"
    );
}

#[then(regex = r#"^stdout reports "Compared N layers" where N equals the layer count$"#)]
fn then_stdout_reports_compared_n_layers(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let n = parse_compared_layers(stdout);

    // Second, independent production observation:
    // `inspect layers --file <job> --json` -> info.total_layers.
    let fixture = world
        .nanodlp_fixture_path
        .as_ref()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "layers",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--json",
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "inspect layers --json must succeed; stderr={}",
        outcome.stderr
    );
    let info: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|e| panic!("inspect layers --json stdout must parse: {e}"));
    let total_layers = info["info"]["total_layers"]
        .as_u64()
        .unwrap_or_else(|| panic!("info.total_layers must be a JSON number, got {info}"));

    assert_eq!(
        n as u64, total_layers,
        "'Compared N layers' must equal inspect layers --json info.total_layers"
    );
}

#[then(
    regex = r#"^stdout reports a "Correlation \(predicted total force vs real peel signal\)" value in \[-1, 1\]$"#
)]
fn then_stdout_reports_correlation_in_range(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let corr = parse_correlation(stdout);
    assert!(
        corr.is_finite() && (-1.0..=1.0).contains(&corr),
        "correlation must be finite and in [-1, 1], got {corr}"
    );
}

#[then(regex = r#"^stdout reports a "Peak layer: predicted P, real A \(offset ±D\)" line$"#)]
fn then_stdout_reports_peak_layer_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let (p, a, off_str) = parse_peak_layer_line(stdout);
    let off: i64 = off_str
        .parse()
        .unwrap_or_else(|e| panic!("parse offset {off_str:?}: {e}"));
    assert_eq!(off, p - a, "rendered offset must equal predicted - real");
    // `{off:+}` always renders an explicit sign, even for zero ("+0") — the
    // spec's "±D" is satisfied by the sign flag, not by D being non-zero.
    assert!(
        off_str.starts_with('+') || off_str.starts_with('-'),
        "rendered offset must carry an explicit sign, got {off_str:?}"
    );
    assert!(
        !stdout.contains("n/a (empty comparison window)"),
        "the empty-comparison-window fallback must be absent when a Peak layer line is present"
    );
}

#[then(regex = r#"^stdout reports a "peel gain" with a "fit R²" value$"#)]
fn then_stdout_reports_peel_gain_and_fit_r2(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(stdout.contains("peel gain"), "stdout must contain 'peel gain'");
    assert!(stdout.contains("fit R²"), "stdout must contain 'fit R²'");
    let (gain, r2) = parse_peel_gain_and_r2(stdout);
    assert!(gain.is_finite(), "peel gain must be finite, got {gain}");
    assert!(
        (0.0..=1.0).contains(&r2),
        "fit R² must be in [0, 1], got {r2}"
    );
}

#[then(regex = r#"^stdout labels the overrides "Suggested" and "NOT applied"$"#)]
fn then_stdout_labels_overrides_suggested_not_applied(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // Verbatim banner, main.rs:1637:
    // "  --- Suggested athena_ii.toml overrides (NOT applied) ---"
    assert!(
        stdout.contains("Suggested") && stdout.contains("NOT applied"),
        "stdout must label the overrides 'Suggested' and 'NOT applied', got: {stdout}"
    );
}

#[then(regex = r"^the printer profile file on disk is unchanged$")]
fn then_printer_profile_file_unchanged(world: &mut UatWorld) {
    let before = world
        .printer_toml_before
        .clone()
        .expect("scenario invariant: Given step populated printer_toml_before");
    let after = printer_toml_snapshot();
    assert_eq!(
        before.0, after.0,
        "data/printers/athena_ii.toml bytes must be unchanged after calibrate"
    );
    assert_eq!(
        before.1, after.1,
        "data/printers/athena_ii.toml mtime must be unchanged after calibrate"
    );
}

// ---- UAT-2: low fit quality is flagged, not hidden -------------------------

#[given(regex = r"^a \.nanodlp whose real force peaks at a different layer than the sim$")]
fn given_nanodlp_real_force_peaks_different_layer(world: &mut UatWorld) {
    let job = late_peaking_variant("calibrate-uat2");
    world.nanodlp_fixture_path = Some(job.path);
}

// ---- shared When for UAT-2/UAT-3 -------------------------------------------

#[when(regex = r"^the user runs `resinsim inspect calibrate --file <job\.nanodlp>`$")]
fn when_user_runs_inspect_calibrate(world: &mut UatWorld) {
    run_calibrate_text(world);
}

#[then(regex = r"^stdout contains a low-fit-quality warning$")]
fn then_stdout_contains_low_fit_quality_warning(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // Verbatim, main.rs:1647.
    assert!(
        stdout.contains("! low fit quality — single print; treat as indicative only"),
        "stdout must contain the low-fit-quality warning, got: {stdout}"
    );
    let (_gain, r2) = parse_peel_gain_and_r2(stdout);
    assert!(
        r2 < 0.5,
        "the warning's trigger condition (fit R² < 0.5) must independently hold, got {r2}"
    );
}

#[then(regex = r"^the suggested gain is still reported for transparency$")]
fn then_suggested_gain_still_reported(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains("peel gain") && stdout.contains("N per raw count"),
        "the peel gain line must be present in the SAME invocation that warned about low fit \
         quality (not suppressed on the low-fit path), got: {stdout}"
    );
}

// ---- UAT-3: the predicted-vs-real peak-layer offset is surfaced (KB-115) -

#[given(
    regex = r"^a \.nanodlp whose real force peaks at layer 0 \(base adhesion\) while the area-driven sim peaks mid-print$"
)]
fn given_nanodlp_real_peaks_layer0_sim_mid_print(world: &mut UatWorld) {
    let job = late_peaking_variant("calibrate-uat3");
    world.nanodlp_fixture_path = Some(job.path);
}

#[then(regex = r#"^stdout reports a "Peak layer:" line whose offset is non-zero$"#)]
fn then_stdout_reports_peak_layer_offset_nonzero(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let (_p, _a, off_str) = parse_peak_layer_line(stdout);
    let off: i64 = off_str
        .parse()
        .unwrap_or_else(|e| panic!("parse offset {off_str:?}: {e}"));
    assert_ne!(off, 0, "expected a non-zero peak-layer offset, got {off}");
}

#[then(regex = r"^when the offset is large stdout emits a KB-115 / ADR-0022 hint line$")]
fn then_large_offset_emits_kb115_hint(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let (_p, _a, off_str) = parse_peak_layer_line(stdout);
    let off: i64 = off_str
        .parse()
        .unwrap_or_else(|e| panic!("parse offset {off_str:?}: {e}"));
    // Non-vacuity guard: prove the conditional's antecedent is actually
    // true for this fixture before asserting the consequent.
    assert!(
        off.abs() >= 3,
        "fixture invariant: |offset| must be >= 3 (PEAK_OFFSET_HINT) for this Then to be \
         non-vacuous, got {off}"
    );
    // main.rs:1626-1628, one `println!` with a `\` line continuation, so
    // the emitted text is a single line.
    assert!(
        stdout.contains("! peak offset ") && stdout.contains("(KB-115)") && stdout.contains("see ADR-0022"),
        "stdout must emit the KB-115 / ADR-0022 hint line, got: {stdout}"
    );
}

#[then(
    regex = r"^`--json` output includes predicted_peak_layer, actual_peak_layer, and peak_layer_offset \(null when a series is empty\)$"
)]
fn then_json_output_includes_peak_layer_fields(world: &mut UatWorld) {
    // Cross-check against the text-mode line the previous Then already
    // parsed, from the shared When's stdout.
    let text_stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let (p_text, a_text, off_text_str) = parse_peak_layer_line(text_stdout);
    let off_text: i64 = off_text_str
        .parse()
        .unwrap_or_else(|e| panic!("parse offset {off_text_str:?}: {e}"));

    // SECOND invocation of the same command with --json — the spec's
    // "(null when a series is empty)" parenthetical describes a branch
    // UNREACHABLE through this CLI (see the module-level SPEC-vs-BINARY
    // MISMATCH doc above): ForceComparator::compare errors out before any
    // JSON prints when a series is empty, so on any REACHABLE --json
    // output these three keys are always present numbers, never null.
    let fixture = world
        .nanodlp_fixture_path
        .as_ref()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let data_dir = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "calibrate",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "athena_ii",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
            "--json",
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "inspect calibrate --json must succeed; stderr={}",
        outcome.stderr
    );
    let v: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|e| panic!("--json stdout must parse as JSON: {e}; stdout={}", outcome.stdout));

    let predicted = v["comparison"]["predicted_peak_layer"]
        .as_i64()
        .unwrap_or_else(|| panic!("comparison.predicted_peak_layer must be a JSON number, got {v}"));
    let actual = v["comparison"]["actual_peak_layer"]
        .as_i64()
        .unwrap_or_else(|| panic!("comparison.actual_peak_layer must be a JSON number, got {v}"));
    let offset = v["comparison"]["peak_layer_offset"]
        .as_i64()
        .unwrap_or_else(|| panic!("comparison.peak_layer_offset must be a JSON number, got {v}"));

    assert_eq!(
        offset,
        predicted - actual,
        "peak_layer_offset must equal predicted_peak_layer - actual_peak_layer"
    );
    assert_eq!(
        predicted, p_text,
        "--json predicted_peak_layer must agree with the text-mode Peak layer line"
    );
    assert_eq!(
        actual, a_text,
        "--json actual_peak_layer must agree with the text-mode Peak layer line"
    );
    assert_eq!(
        offset, off_text,
        "--json peak_layer_offset must agree with the text-mode Peak layer line"
    );
}
