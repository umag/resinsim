//! Step definitions for `spec/uat/athena-analytic-log-ingest.md` UAT-1..UAT-2
//! (uat-unskip-a3-b, plan step 3). Lands FIRST in this increment: committed
//! fixtures only, no synthesised archive, proving the CLI step shape at the
//! lowest risk.
//!
//! SYMBOL VERIFICATION (performed before this module was written, per the
//! band-membership-by-symbol re-derivation dated 2026-08-03). Every
//! production entry point `inspect athena` (`cmd_athena`,
//! crates/resinsim-inspect/src/main.rs:1394-1464) composes is default-
//! features, `#[cfg]`-free: `io::athena::{load_analytic_csv, parse_analytic}`,
//! `services::{ForceSeriesExtractor::extract_layer_forces, peak_index,
//! filter_layer_range}`. `grep -n '#\[cfg(feature' io/athena.rs` returns
//! nothing.
//!
//! REGEX DISTINCTNESS block. Checked directly against the global step-def
//! inventory (every `regex = r` line under `tests/uat_steps/`):
//! `cli_temperature_flag_validation.rs`'s `` the process exits with a
//! non-zero code `` / `` the process exits with a non-zero code (2) `` / ``
//! the process exits with code 2 `` and `no simulation rows are printed on
//! stdout` are all textually distinct from this module's `` the command
//! exits non-zero with an actionable parse error naming the row ``;
//! `sim_json_roundtrips_zero_force_layer.rs`'s `` the process exits 0 `` is
//! distinct from anything registered here (this module never asserts a bare
//! "exits 0" — UAT-1's success path is proven by the parsed-count/peak
//! Thens, not a separate exit-code step). No collision found.
//!
//! HONEST-BENEFIT CAVEAT. Several of the numeric claims here are already
//! pinned by shipped nextest tests: `src/io/athena.rs`'s
//! `parse_tall_splits_channels` / `malformed_row_rejected`, and
//! `tests/athena_fixture_roundtrip.rs`'s
//! `extract_with_prelude_count_matches_hand_computed_layer_table` (6 layers,
//! layer-0 peak 400.0) / `peak_index_is_layer_zero_the_kb115_shape`. The
//! value of this module is spec-to-CLI traceability and register shrink —
//! proving the numbers survive the FULL `cmd_athena` rendering pipeline
//! (parse -> extract -> filter -> peak_index -> println! formatting), not
//! new defect-finding power over the unit/integration layer.
//!
//! UAT-1's "stdout reports the number of layers with force data" /
//! "reports a peak peel signal" Thens do not re-derive the count or the
//! peak from the fixture — they parse the CLI's OWN rendered text line,
//! cross-check it against a second production observation (the SAME
//! invocation's `--json` output), and only then compare to the literal
//! `tests/athena_fixture_roundtrip.rs` already pins
//! (docs/patterns/anti/test-mirrors-production-formula.md).

use cucumber::{given, then, when};

use super::cli_fixtures::invoke_resinsim;
use super::fixtures::{fixture_path, write_tall_analytic_csv};
use super::world::UatWorld;

// ---- UAT-1: tall analytic CSV parses and reports per-layer force ----------

#[given(regex = r#"^an Athena analytic log in tall "ID,T,V" form \(gzip or plain\)$"#)]
fn given_athena_log_tall_form(world: &mut UatWorld) {
    // Committed twins (docs/patterns/golden-file-byte-identity-guard.md
    // precedent: reuse the committed fixture rather than synthesise one).
    // `tests/athena_fixture_roundtrip.rs::plain_and_gzip_twins_parse_identically`
    // already proves these two files parse to the SAME samples; this
    // scenario proves the CLI renders them identically too.
    world.athena_csv_paths = Some(vec![
        fixture_path("synthetic_stepped_forces.csv"),
        fixture_path("synthetic_stepped_forces.csv.gz"),
    ]);
}

#[when(regex = r"^the user runs `resinsim inspect athena --file <log\.csv\.gz>`$")]
fn when_user_runs_inspect_athena_gz(world: &mut UatWorld) {
    // Runs the CLI once per twin, and additionally once with `--json` per
    // twin (the `cli_temperature_flag_validation.rs` "for v in [...]"
    // precedent), so the text-mode numbers can be cross-checked against a
    // second production observation rather than a hardcode.
    let paths = world
        .athena_csv_paths
        .clone()
        .expect("scenario invariant: Given step populated athena_csv_paths");
    let mut text_stdout = Vec::with_capacity(paths.len());
    let mut json_stdout = Vec::with_capacity(paths.len());
    for path in &paths {
        let p = path.to_str().expect("fixture path is UTF-8");
        let text = invoke_resinsim(&["inspect", "athena", "--file", p], &[]);
        assert_eq!(
            text.exit_code, 0,
            "inspect athena (text mode) must succeed for {p}; stderr={}",
            text.stderr
        );
        text_stdout.push(text.stdout);

        let json = invoke_resinsim(&["inspect", "athena", "--file", p, "--json"], &[]);
        assert_eq!(
            json.exit_code, 0,
            "inspect athena --json must succeed for {p}; stderr={}",
            json.stderr
        );
        json_stdout.push(json.stdout);
    }
    world.athena_text_stdout = Some(text_stdout);
    world.athena_json_stdout = Some(json_stdout);
}

/// Parse the exact `  Layers with force data: {count}` line
/// (main.rs:1456).
fn parse_layers_with_force_data(stdout: &str) -> usize {
    stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Layers with force data: "))
        .and_then(|rest| rest.trim().parse::<usize>().ok())
        .unwrap_or_else(|| {
            panic!("could not parse 'Layers with force data:' line from stdout: {stdout}")
        })
}

/// Parse the exact `  Peak peel signal: {peak_signal:.1} counts at layer
/// {idx}` line (main.rs:1458). Returns `(peak_signal, layer)`.
fn parse_peak_peel_signal(stdout: &str) -> (f64, u32) {
    let line = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Peak peel signal: "))
        .unwrap_or_else(|| panic!("could not find 'Peak peel signal:' line in stdout: {stdout}"));
    let mut parts = line.splitn(2, " counts at layer ");
    let signal_str = parts
        .next()
        .unwrap_or_else(|| panic!("malformed 'Peak peel signal:' line: {line}"));
    let layer_str = parts
        .next()
        .unwrap_or_else(|| panic!("'Peak peel signal:' line missing 'counts at layer': {line}"));
    let signal: f64 = signal_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse peak signal {signal_str:?}: {e}"));
    let layer: u32 = layer_str
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("parse peak layer {layer_str:?}: {e}"));
    (signal, layer)
}

fn parsed_json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("--json stdout must parse as JSON: {e}; stdout={stdout}"))
}

#[then(regex = r"^stdout reports the number of layers with force data$")]
fn then_stdout_reports_layer_count(world: &mut UatWorld) {
    let text = world
        .athena_text_stdout
        .as_ref()
        .expect("scenario invariant: When step populated athena_text_stdout");
    let json = world
        .athena_json_stdout
        .as_ref()
        .expect("scenario invariant: When step populated athena_json_stdout");
    assert_eq!(text.len(), 2, "expected [plain, gz] text stdout entries");
    assert_eq!(json.len(), 2, "expected [plain, gz] json stdout entries");

    let plain_count = parse_layers_with_force_data(&text[0]);
    let gz_count = parse_layers_with_force_data(&text[1]);
    assert_eq!(
        plain_count, gz_count,
        "plain and gz twins must report the same layer count"
    );

    for (label, j) in [("plain", &json[0]), ("gz", &json[1])] {
        let stats_layers = parsed_json(j)["stats"]["layers"]
            .as_u64()
            .unwrap_or_else(|| panic!("{label}: stats.layers must be a JSON number, got {j}"));
        assert_eq!(
            stats_layers as usize, plain_count,
            "{label}: text-mode layer count must equal --json stats.layers"
        );
    }

    // Pinned by tests/athena_fixture_roundtrip.rs::
    // extract_with_prelude_count_matches_hand_computed_layer_table for this
    // exact committed fixture.
    assert_eq!(
        plain_count, 6,
        "committed synthetic_stepped_forces fixture must report 6 layers"
    );
}

#[then(regex = r"^stdout reports a peak peel signal in raw load-cell counts$")]
fn then_stdout_reports_peak_peel_signal(world: &mut UatWorld) {
    let text = world
        .athena_text_stdout
        .as_ref()
        .expect("scenario invariant: When step populated athena_text_stdout");
    let json = world
        .athena_json_stdout
        .as_ref()
        .expect("scenario invariant: When step populated athena_json_stdout");

    assert!(
        text[0].contains("counts") && text[1].contains("counts"),
        "peak-signal line must carry the literal word 'counts'"
    );

    let (plain_signal, plain_layer) = parse_peak_peel_signal(&text[0]);
    let (gz_signal, gz_layer) = parse_peak_peel_signal(&text[1]);
    assert!(
        (plain_signal - gz_signal).abs() < 1e-9,
        "plain and gz twins must report the same peak signal: {plain_signal} vs {gz_signal}"
    );
    assert_eq!(
        plain_layer, gz_layer,
        "plain and gz twins must report the same peak layer"
    );

    for (label, j) in [("plain", &json[0]), ("gz", &json[1])] {
        let v = parsed_json(j);
        let stats_peak = v["stats"]["peak_signal"]
            .as_f64()
            .unwrap_or_else(|| panic!("{label}: stats.peak_signal must be a JSON number, got {v}"));
        assert!(
            (stats_peak - plain_signal).abs() < 1e-9,
            "{label}: text-mode peak signal must equal --json stats.peak_signal: \
             {plain_signal} vs {stats_peak}"
        );
        let stats_peak_layer = v["stats"]["peak_layer"]
            .as_u64()
            .unwrap_or_else(|| panic!("{label}: stats.peak_layer must be a JSON number, got {v}"));
        assert_eq!(
            stats_peak_layer as u32, plain_layer,
            "{label}: text-mode peak layer must equal --json stats.peak_layer"
        );
    }

    // Pinned by tests/athena_fixture_roundtrip.rs (layer-0 peak 400.0,
    // KB-115 shape) via peak_index_is_layer_zero_the_kb115_shape /
    // extract_with_prelude_count_matches_hand_computed_layer_table.
    assert!(
        (plain_signal - 400.0).abs() < 1e-9,
        "committed fixture's pinned peak signal is 400.0, got {plain_signal}"
    );
    assert_eq!(plain_layer, 0, "committed fixture's pinned peak layer is 0");
}

#[then(regex = r#"^stdout labels the values "not Newtons"$"#)]
fn then_stdout_labels_not_newtons(world: &mut UatWorld) {
    let text = world
        .athena_text_stdout
        .as_ref()
        .expect("scenario invariant: When step populated athena_text_stdout");
    for (label, stdout) in [("plain", &text[0]), ("gz", &text[1])] {
        // Verbatim tail of main.rs:1462's
        // `  (raw load-cell counts, sign-corrected; not Newtons — see
        // \`calibrate\`)` line.
        assert!(
            stdout.contains("not Newtons"),
            "{label}: stdout must label the values 'not Newtons', got: {stdout}"
        );
    }
}

// ---- UAT-2: malformed rows are rejected ------------------------------------

#[given(regex = r"^an analytic CSV containing a row with a non-numeric V field$")]
fn given_analytic_csv_non_numeric_v(world: &mut UatWorld) {
    // Exact shape io/athena.rs's own `malformed_row_rejected` unit test
    // uses.
    let path = write_tall_analytic_csv(
        "athena-malformed-row",
        "ID,T,V\n100,6,not_a_number\n",
        false,
    );
    world.athena_csv_paths = Some(vec![path]);
}

#[when(regex = r"^the user runs `resinsim inspect athena --file <log\.csv>`$")]
fn when_user_runs_inspect_athena_plain(world: &mut UatWorld) {
    let paths = world
        .athena_csv_paths
        .clone()
        .expect("scenario invariant: Given step populated athena_csv_paths");
    let path = paths
        .first()
        .expect("UAT-2 Given populates exactly one CSV path");
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "athena",
            "--file",
            path.to_str().expect("temp CSV path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(
    regex = r"^the command exits non-zero with an actionable parse error naming the row$"
)]
fn then_command_exits_nonzero_naming_row(world: &mut UatWorld) {
    let exit_code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();

    assert_ne!(exit_code, 0, "expected a non-zero exit code; stderr={stderr}");
    // main.rs:1401-ish `eprintln!("Error: {e}");` — the CLI's own wrapping.
    assert!(
        stderr.contains("Error: "),
        "stderr must carry the CLI's 'Error: ' prefix, got: {stderr}"
    );
    // io/athena.rs:117 `format!("analytic CSV row {}: {e}", row + 1)` wraps
    // the inner `bad V` error from athena.rs:133-136
    // `format!("analytic CSV row {}: bad V {:?}: {e}", row + 1, &rec[2])`.
    // Row 1 (1-indexed) is the sole data row.
    assert!(
        stderr.contains("analytic CSV row 1"),
        "stderr must name the offending row, got: {stderr}"
    );
    assert!(
        stderr.contains("bad V"),
        "stderr must name the offending field, got: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "no simulation/inspection rows should print on stdout for a hard parse error, got: {stdout}"
    );
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
        "a parse error must be a clean CLI error, not a Rust panic: {stderr}"
    );
}
