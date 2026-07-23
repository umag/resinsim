//! Integration tests for the CLI profile loader (ADR-0004).
//!
//! Uses std::process::Command + env!("CARGO_BIN_EXE_resinsim") to drive the
//! full CLI surface without pulling in assert_cmd.
//!
//! NOTE: nextest's CWD during these tests is the crate root
//! (crates/resinsim-inspect), NOT the workspace root. Therefore these tests
//! MUST pass --data-dir explicitly; stage (c) $CWD/data would resolve to
//! crates/resinsim-inspect/data which does not exist.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_resinsim")
}

fn workspace_data_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/resinsim-inspect; data/ is at ../../data/
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .canonicalize()
        .expect("test fixture: workspace data/ exists")
}

fn tmpdir(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "resinsim-cli-test-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is post-epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).expect("test fixture: create tmp dir");
    d
}

/// Helper: run `resinsim sim --stl <stl> ... --out <sim.json>`. Returns the
/// std::process::Output of the sim invocation. The sim.json producer side of
/// the ADR-0015 pipeline; consumed by `report_health` via `--in`.
///
/// The bin path comes from `env!("CARGO_BIN_EXE_resinsim")` which cargo
/// makes available to integration tests built within the same workspace
/// as the bin target. If you copy this helper to another crate's tests
/// you'll need to adjust the binary-resolution to match that crate's
/// build context.
fn run_sim_stl(
    stl: &Path,
    out: &Path,
    data: &Path,
    printer: &str,
    extra: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(["sim", "--stl"])
        .arg(stl)
        .args(["--out"])
        .arg(out)
        .args(["--data-dir"])
        .arg(data)
        .args(["--printer", printer]);
    for arg in extra {
        cmd.arg(arg);
    }
    cmd.output().expect("spawn sim")
}

// --- Resolver stage tests ---

#[test]
fn stage_a_flag_wins_over_env() {
    // Flag points at workspace data; env points at a bogus path that would
    // cause stage (b) hard error if it were evaluated first.
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--data-dir",
        ])
        .arg(&data)
        .env("RESINSIM_DATA_DIR", "/nonexistent/bogus/path")
        .args(["--json"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stage (a) should win: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1500"),
        "athena_ii z_stiffness=1500 must be sourced (via stage (a) flag): got {stdout}"
    );
}

#[test]
fn stage_b_env_wins_over_cwd() {
    // Flag absent, env set → resolution uses env. CWD stage would not have
    // data/ (nextest CWD is the crate root).
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--json",
        ])
        .env("RESINSIM_DATA_DIR", &data)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stage (b) env should resolve: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1500"));
}

#[test]
fn all_stages_miss_hard_errors() {
    // No flag, no env, CWD without data/, current_exe parent without data/.
    // Stages a-d all miss → hard error.
    let empty = tmpdir("all_miss");
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
        ])
        .current_dir(&empty)
        .env_remove("RESINSIM_DATA_DIR")
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "all-miss should exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("could not resolve profile data directory"),
        "stderr must mention resolution failure: {stderr}"
    );
    assert!(
        stderr.contains("--data-dir") && stderr.contains("RESINSIM_DATA_DIR"),
        "stderr must list both remediation options: {stderr}"
    );
    std::fs::remove_dir_all(&empty).ok();
}

#[test]
fn data_dir_is_file_not_dir_falls_through() {
    // Stage (a) with a regular file — is_dir() is false, so it falls through
    // to later stages. If env/cwd/exe all miss, hard error.
    let tmp = tmpdir("is_file");
    let file = tmp.join("not-a-dir.txt");
    std::fs::write(&file, "").expect("write");
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--data-dir",
        ])
        .arg(&file)
        .current_dir(&tmp)
        .env_remove("RESINSIM_DATA_DIR")
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "file-as-data-dir should fall through then hard error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("could not resolve"));
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn empty_data_dir_reports_no_available_profiles() {
    // Data-dir exists but empty → unknown-profile error lists "(none)".
    let empty = tmpdir("empty");
    std::fs::create_dir_all(empty.join("printers")).expect("mkdir");
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--data-dir",
        ])
        .arg(&empty)
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Available profiles: (none)"),
        "stderr should list no profiles: {stderr}"
    );
    std::fs::remove_dir_all(&empty).ok();
}

// --- Profile loading tests ---

#[test]
fn report_health_athena_ii_uses_toml_stiffness() {
    // Defends the z_stiffness resolution (Athena II k=1500, not the silent generic
    // k=460 fallback). Rebuilt for ADR-0022 Stage 1: asserts the stiffness RATIO
    // (force/deflection), robust to the KB-116 base term's effect on the absolute
    // first-layer force. See the detailed note at the assertion below.
    //
    // ADR-0015: drives the canonical-interchange pipeline (sim → report
    // health --in). Test both halves so a regression in either producer or
    // consumer surfaces.
    let cube = tmpdir("cube");
    let stl = cube.join("cube-60mm.stl");
    std::fs::write(&stl, cube_60mm_stl()).expect("write stl");
    let data = workspace_data_dir();
    let sim_out = cube.join("cube.sim.json");
    let sim = run_sim_stl(
        &stl,
        &sim_out,
        &data,
        "athena_ii",
        &["--tip-radius", "0", "--n-supports", "0"],
    );
    assert!(
        sim.status.success(),
        "sim subcommand should succeed: stderr={}",
        String::from_utf8_lossy(&sim.stderr)
    );
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&sim_out)
        .args(["--json"])
        .output()
        .expect("spawn report health");
    assert!(
        out.status.success(),
        "report health athena_ii should succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The regression this defends: pre-ADR-0004 the CLI silently fell back to the
    // generic_msla_4k z_stiffness (k=460) instead of Athena II's k=1500, inflating
    // deflection (~101.7 µm on the old peel-only force).
    //
    // Rebuilt for ADR-0022 Stage 1 (KB-116 base adhesion): generic_standard now carries
    // a first-layer base term, so the layer-0 total force — and hence the ABSOLUTE
    // deflection — is much larger than the old peel-only ~31 µm (its former assertion).
    // z_deflection = total_force / z_stiffness, so total_force / deflection == z_stiffness
    // REGARDLESS of the base magnitude. Asserting that ratio survives the base term AND
    // any future Δσ₀ refit, while k≈460 still fails loudly. max_peel_force_n is the max
    // TOTAL force (print_simulation summary), at the same layer as max deflection.
    let defl_um = extract_f64(&stdout, "\"max_z_deflection_um\":");
    let force_n = extract_f64(&stdout, "\"max_peel_force_n\":");
    assert!(defl_um > 0.0, "deflection should be positive, got {defl_um}");
    let stiffness_n_per_mm = force_n / (defl_um / 1000.0);
    assert!(
        (1400.0..=1600.0).contains(&stiffness_n_per_mm),
        "expected Athena II z_stiffness ≈ 1500 N/mm (max total force {force_n} N / \
         deflection {defl_um} µm = {stiffness_n_per_mm:.0}); a value near 460 means the \
         pre-ADR-0004 silent generic_msla_4k fallback has regressed"
    );
    std::fs::remove_dir_all(&cube).ok();
}

#[test]
fn report_health_unknown_printer_hard_errors_with_available_list() {
    // The unknown-printer guard moves to the SIM (producer) side after
    // ADR-0015 — `report health` no longer touches profiles. Drive the
    // sim subcommand against a bogus printer and assert the same
    // available-list error shape; the consumer half is decoupled from
    // profile resolution by design.
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args(["sim", "--stl", "/tmp/no-such-file.stl", "--data-dir"])
        .arg(&data)
        .args(["--printer", "bogus_printer_name"])
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bogus_printer_name"),
        "error mentions the bogus name: {stderr}"
    );
    assert!(
        stderr.contains("Available profiles"),
        "error lists available: {stderr}"
    );
    assert!(
        stderr.contains("athena_ii"),
        "error lists athena_ii among available: {stderr}"
    );
}

// --- Override precedence tests ---

#[test]
fn explicit_stiffness_overrides_profile() {
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--stiffness",
            "200",
            "--data-dir",
        ])
        .arg(&data)
        .args(["--json"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let k = extract_f64(&stdout, "\"stiffness_n_per_mm\":");
    assert!(
        (k - 200.0).abs() < 0.1,
        "explicit --stiffness=200 must win: got {k}"
    );
}

#[test]
fn no_profile_flag_skips_resolution_even_with_bogus_env() {
    // Assert ADR-0004 §Decision(b): resolution only triggered by --printer/--resin.
    // Bogus env would cause a hard error if resolution were attempted
    // unconditionally.
    let out = Command::new(bin())
        .args(["inspect", "zaxis", "--force", "46.8", "--json"])
        .env("RESINSIM_DATA_DIR", "/nonexistent/should/not/be/resolved")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "no --printer flag means no resolution: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let k = extract_f64(&stdout, "\"stiffness_n_per_mm\":");
    assert!(
        (k - 460.0).abs() < 0.1,
        "built-in default 460 must apply: got {k}"
    );
}

// --- Byte-identity regression guards (issue: reportgenerator-extraction) ---
//
// These two tests are the byte-identity acceptance check for extracting report
// assembly out of cmd_report_health into a ReportGenerator application service.
// They were written and committed BEFORE the extraction touched any production
// code, so they pass against the unmodified CLI and become the regression
// guard: any drift in stdout (text or JSON) that the existing field-grep tests
// at :194 / :245 cannot catch will be caught here.
//
// The goldens contain `__STL_PATH__` as a placeholder for the per-run tmpdir
// path; the test substitutes the actual path before comparing.

/// Regenerate a report-health byte-identity golden when
/// `RESINSIM_REGENERATE_REPORT_GOLDEN=1` (mirrors sim_golden's regenerate mode).
/// Substitutes the per-run STL path back to the `__STL_PATH__` placeholder.
/// Returns true when a golden was written (the caller then skips the assert).
fn maybe_regenerate_report_golden(golden_name: &str, stdout: &str, stl: &Path) -> bool {
    if std::env::var("RESINSIM_REGENERATE_REPORT_GOLDEN").is_err() {
        return false;
    }
    let placeholdered = stdout.replace(stl.to_str().expect("stl path is utf-8"), "__STL_PATH__");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(golden_name);
    std::fs::write(&path, placeholdered).expect("write golden");
    eprintln!("regenerated {}", path.display());
    true
}

#[test]
fn report_health_athena_ii_text_byte_identity() {
    let cube = tmpdir("text_byte_id");
    let stl = cube.join("cube-60mm.stl");
    std::fs::write(&stl, cube_60mm_stl()).expect("write stl");
    let data = workspace_data_dir();
    let sim_out = cube.join("cube.sim.json");
    let sim = run_sim_stl(
        &stl,
        &sim_out,
        &data,
        "athena_ii",
        &[
            "--resin",
            "generic_standard",
            "--tip-radius",
            "0",
            "--n-supports",
            "0",
        ],
    );
    assert!(
        sim.status.success(),
        "sim should succeed: stderr={}",
        String::from_utf8_lossy(&sim.stderr)
    );
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&sim_out)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "report health text mode should succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    if maybe_regenerate_report_golden("report_health_athena_ii.text.golden", &stdout, &stl) {
        std::fs::remove_dir_all(&cube).ok();
        return;
    }
    let golden = include_str!("fixtures/report_health_athena_ii.text.golden")
        .replace("__STL_PATH__", stl.to_str().expect("stl path is utf-8"));
    assert_eq!(
        stdout, golden,
        "text-mode stdout must be byte-identical to fixtures/report_health_athena_ii.text.golden"
    );
    std::fs::remove_dir_all(&cube).ok();
}

#[test]
fn report_health_athena_ii_json_byte_identity() {
    let cube = tmpdir("json_byte_id");
    let stl = cube.join("cube-60mm.stl");
    std::fs::write(&stl, cube_60mm_stl()).expect("write stl");
    let data = workspace_data_dir();
    let sim_out = cube.join("cube.sim.json");
    let sim = run_sim_stl(
        &stl,
        &sim_out,
        &data,
        "athena_ii",
        &[
            "--resin",
            "generic_standard",
            "--tip-radius",
            "0",
            "--n-supports",
            "0",
        ],
    );
    assert!(
        sim.status.success(),
        "sim should succeed: stderr={}",
        String::from_utf8_lossy(&sim.stderr)
    );
    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&sim_out)
        .args(["--json"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "report health json mode should succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    if maybe_regenerate_report_golden("report_health_athena_ii.json.golden", &stdout, &stl) {
        std::fs::remove_dir_all(&cube).ok();
        return;
    }
    let golden = include_str!("fixtures/report_health_athena_ii.json.golden")
        .replace("__STL_PATH__", stl.to_str().expect("stl path is utf-8"));
    assert_eq!(
        stdout, golden,
        "json-mode stdout must be byte-identical to fixtures/report_health_athena_ii.json.golden"
    );
    std::fs::remove_dir_all(&cube).ok();
}

// --- Helpers ---

fn extract_f64(json: &str, key: &str) -> f64 {
    let after = &json[json
        .find(key)
        .unwrap_or_else(|| panic!("key '{key}' not in json: {json}"))
        + key.len()..];
    let end = after.find([',', '}']).expect("end of number");
    after[..end].trim().parse().expect("parse f64")
}

fn cube_60mm_stl() -> String {
    String::from(
        "solid cube60\n\
  facet normal 0 0 -1\n    outer loop\n      vertex 0 0 0\n      vertex 60 60 0\n      vertex 60 0 0\n    endloop\n  endfacet\n\
  facet normal 0 0 -1\n    outer loop\n      vertex 0 0 0\n      vertex 0 60 0\n      vertex 60 60 0\n    endloop\n  endfacet\n\
  facet normal 0 0 1\n    outer loop\n      vertex 0 0 60\n      vertex 60 0 60\n      vertex 60 60 60\n    endloop\n  endfacet\n\
  facet normal 0 0 1\n    outer loop\n      vertex 0 0 60\n      vertex 60 60 60\n      vertex 0 60 60\n    endloop\n  endfacet\n\
  facet normal -1 0 0\n    outer loop\n      vertex 0 0 0\n      vertex 0 60 60\n      vertex 0 60 0\n    endloop\n  endfacet\n\
  facet normal -1 0 0\n    outer loop\n      vertex 0 0 0\n      vertex 0 0 60\n      vertex 0 60 60\n    endloop\n  endfacet\n\
  facet normal 1 0 0\n    outer loop\n      vertex 60 0 0\n      vertex 60 60 0\n      vertex 60 60 60\n    endloop\n  endfacet\n\
  facet normal 1 0 0\n    outer loop\n      vertex 60 0 0\n      vertex 60 60 60\n      vertex 60 0 60\n    endloop\n  endfacet\n\
  facet normal 0 -1 0\n    outer loop\n      vertex 0 0 0\n      vertex 60 0 0\n      vertex 60 0 60\n    endloop\n  endfacet\n\
  facet normal 0 -1 0\n    outer loop\n      vertex 0 0 0\n      vertex 60 0 60\n      vertex 0 0 60\n    endloop\n  endfacet\n\
  facet normal 0 1 0\n    outer loop\n      vertex 0 60 0\n      vertex 0 60 60\n      vertex 60 60 60\n    endloop\n  endfacet\n\
  facet normal 0 1 0\n    outer loop\n      vertex 0 60 0\n      vertex 60 60 60\n      vertex 60 60 0\n    endloop\n  endfacet\n\
endsolid cube60\n",
    )
}
