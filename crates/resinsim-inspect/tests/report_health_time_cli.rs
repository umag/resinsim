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

#[test]
fn report_health_json_includes_print_time_fields() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = Command::new(bin())
        .args([
            "report",
            "health",
            "--stl",
            stl.to_str().expect("ascii path"),
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("ascii path"),
            "--json",
        ])
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
    let out = Command::new(bin())
        .args([
            "report",
            "health",
            "--stl",
            stl.to_str().expect("ascii path"),
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
    let out = Command::new(bin())
        .args([
            "report",
            "health",
            "--file",
            &fixture,
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("ascii path"),
            "--json",
        ])
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
