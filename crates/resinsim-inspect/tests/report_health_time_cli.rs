//! CLI smoke tests for `report health`'s print-time output.
//!
//! The sliced-data path (CTB) is gated behind the RESINSIM_SLICED_FIXTURE
//! env var matching `suction_detector_integration.rs::RESINSIM_EXTERNAL_CTB_FIXTURE`
//! convention — run with
//!   RESINSIM_SLICED_FIXTURE=/path/to/cube.ctb cargo nextest run --run-ignored=all report_health_time_cli
//! until a committed data/test_cube_10mm.ctb fixture lands. The STL-based
//! tests run unconditionally on the small test_cube STL to exercise the
//! JSON + human output shape without paying for a large slice.
//!
//! Under v4 (print-time-on-reportgenerator), the `inspect thermal` command's
//! legacy single-stage path is OUT OF SCOPE — the orphan's UAT-4 / UAT-4b
//! scenarios asserting that `inspect thermal` requires --printer AND --resin
//! are NOT ported. Those changes live in a separate follow-up issue.
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_resinsim")
}

fn workspace_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .canonicalize()
        .expect("test fixture: workspace data/ exists")
}

fn tmpdir(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "resinsim-rht-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is post-epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).expect("test fixture: create tmp dir");
    d
}

/// Helper: produce a sim.json envelope from an STL via the new `sim`
/// subcommand (ADR-0015). Returns the path to the produced envelope.
///
/// The bin path comes from `env!("CARGO_BIN_EXE_resinsim")` which cargo
/// makes available only when the test crate is built within the same
/// workspace as the bin target.
fn produce_sim_envelope(stl: &Path, data: &Path) -> PathBuf {
    let out_dir = tmpdir("sim_out");
    let out = out_dir.join("envelope.sim.json");
    let result = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(stl)
        .args(["--out"])
        .arg(&out)
        .args([
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
        ])
        .arg(data)
        .output()
        .expect("spawn sim");
    assert!(
        result.status.success(),
        "sim subcommand should succeed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    out
}

#[test]
fn report_health_json_includes_print_time_fields() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let envelope = produce_sim_envelope(&stl, &data);
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&envelope)
        .args(["--json"])
        .output()
        .expect("spawn resinsim");
    assert!(
        out.status.success(),
        "report health --json exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 json output");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let summary = parsed.get("summary").expect("JSON has summary object");
    let total = summary["total_time_sec"]
        .as_f64()
        .expect("total_time_sec is a number");
    let bottom = summary["bottom_time_sec"]
        .as_f64()
        .expect("bottom_time_sec is a number");
    let transition = summary["transition_time_sec"]
        .as_f64()
        .expect("transition_time_sec is a number");
    let normal = summary["normal_time_sec"]
        .as_f64()
        .expect("normal_time_sec is a number");
    assert!(total > 0.0, "total_time_sec should be positive: {total}");
    let sum = bottom + transition + normal;
    let tol = (total.abs() * 1e-3).max(1e-6);
    assert!(
        (sum - total).abs() < tol,
        "phase sum {sum} should equal total {total} within {tol}",
    );
}

#[test]
fn report_health_human_output_has_total_time_line() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let envelope = produce_sim_envelope(&stl, &data);
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&envelope)
        .output()
        .expect("spawn resinsim");
    assert!(
        out.status.success(),
        "report health exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 output");
    assert!(
        stdout.contains("Total time:"),
        "human output should contain 'Total time:' line; got:\n{stdout}",
    );
    assert!(
        stdout.contains("bottom:"),
        "human output should contain 'bottom:' line; got:\n{stdout}",
    );
    assert!(
        stdout.contains("normal:"),
        "human output should contain 'normal:' line; got:\n{stdout}",
    );
}

/// `report health --in <envelope-without-provenance> --json` must emit
/// `null` for the resin field rather than an English placeholder string.
/// This pins the JSON-mode contract: machine consumers can branch on
/// `null` cleanly via `jq .resin // empty`. Per ADR-0015 round-1 review.
///
/// Setup: produce a sim.json envelope via the CLI (which always writes
/// provenance), then strip the `provenance` key by reading-modifying-
/// rewriting the JSON in-place. The resulting envelope mimics what the
/// resinsim-viz Save-Sim path or older tooling would produce.
#[test]
fn report_health_json_emits_null_resin_for_envelope_without_provenance() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let envelope = produce_sim_envelope(&stl, &data);

    // Strip the provenance field in-place so report health sees a
    // legacy / GUI-Save-Sim shape envelope.
    let bytes = std::fs::read_to_string(&envelope).expect("read envelope");
    let mut value: serde_json::Value =
        serde_json::from_str(&bytes).expect("envelope is valid JSON");
    value
        .as_object_mut()
        .expect("envelope root is an object")
        .remove("provenance");
    std::fs::write(
        &envelope,
        serde_json::to_string_pretty(&value).expect("serialize tampered envelope"),
    )
    .expect("write envelope without provenance");

    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&envelope)
        .args(["--json"])
        .output()
        .expect("spawn resinsim");
    assert!(
        out.status.success(),
        "report health --json must succeed even without provenance; stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 json output");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(
        parsed["resin"],
        serde_json::Value::Null,
        "resin must be null in JSON output when envelope has no provenance \
         (NOT a string placeholder); got: {}",
        parsed["resin"]
    );
}

/// Same envelope-without-provenance, but in text mode: the human-readable
/// fallback string `"(unknown)"` must surface for resin/printer/supports.
/// Pins the text/JSON parity contract — text mode keeps a placeholder,
/// JSON mode emits null.
#[test]
fn report_health_text_uses_unknown_placeholder_for_envelope_without_provenance() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let envelope = produce_sim_envelope(&stl, &data);

    let bytes = std::fs::read_to_string(&envelope).expect("read envelope");
    let mut value: serde_json::Value =
        serde_json::from_str(&bytes).expect("envelope is valid JSON");
    value
        .as_object_mut()
        .expect("envelope root is an object")
        .remove("provenance");
    std::fs::write(
        &envelope,
        serde_json::to_string_pretty(&value).expect("serialize tampered envelope"),
    )
    .expect("write envelope without provenance");

    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&envelope)
        .output()
        .expect("spawn resinsim");
    assert!(
        out.status.success(),
        "report health (text) must succeed without provenance; stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 output");
    assert!(
        stdout.contains("Resin: (unknown), Printer: (unknown)"),
        "text mode must surface the (unknown) placeholder; got:\n{stdout}",
    );
    assert!(
        stdout.contains("Supports: (unknown)"),
        "text mode must surface (unknown) for supports; got:\n{stdout}",
    );
    // Defence-in-depth: the verbose pre-fix English placeholder must be gone.
    assert!(
        !stdout.contains("envelope has no provenance metadata"),
        "the pre-fix English placeholder must not appear in output; got:\n{stdout}",
    );
}

/// Optional sliced-data smoke test — reads a user-supplied CTB fixture to
/// exercise the `report health --file` path with pre-sliced input. Gated
/// behind `RESINSIM_SLICED_FIXTURE` env var, matching the
/// `RESINSIM_EXTERNAL_CTB_FIXTURE` convention in
/// `resinsim-core/tests/suction_detector_integration.rs`.
///
/// ```sh
/// RESINSIM_SLICED_FIXTURE=/path/to/cube.ctb \
///   cargo nextest run --run-ignored=all report_health_sliced_ctb
/// ```
#[test]
#[ignore = "optional — requires RESINSIM_SLICED_FIXTURE env var pointing to a CTB file"]
fn report_health_sliced_ctb_json_shape() {
    let fixture = std::env::var("RESINSIM_SLICED_FIXTURE")
        .expect("RESINSIM_SLICED_FIXTURE env var required for this test");
    let data = workspace_data_dir();
    let out_dir = tmpdir("sliced");
    let envelope = out_dir.join("sliced.sim.json");
    let sim = Command::new(bin())
        .args(["sim", "--file", &fixture, "--out"])
        .arg(&envelope)
        .args([
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("ascii path"),
        ])
        .output()
        .expect("spawn sim");
    assert!(
        sim.status.success(),
        "sim subcommand exited non-zero: stderr={}",
        String::from_utf8_lossy(&sim.stderr)
    );
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&envelope)
        .args(["--json"])
        .output()
        .expect("spawn resinsim");
    assert!(
        out.status.success(),
        "sliced report health --json exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 json output");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let total = parsed["summary"]["total_time_sec"]
        .as_f64()
        .expect("sliced run produces total_time_sec");
    assert!(
        total > 0.0,
        "sliced fixture total must be positive: {total}"
    );
}
