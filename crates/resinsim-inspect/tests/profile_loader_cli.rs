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
    // The reproduction from the triage report. Pre-fix: deflection = 101.7 µm
    // (generic fallback). Post-fix: ≈ 31.2 µm (Athena II z_stiffness=1500).
    let cube = tmpdir("cube");
    let stl = cube.join("cube-60mm.stl");
    std::fs::write(&stl, cube_60mm_stl()).expect("write stl");
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args(["report", "health", "--stl"])
        .arg(&stl)
        .args(["--data-dir"])
        .arg(&data)
        .args([
            "--printer",
            "athena_ii",
            "--tip-radius",
            "0",
            "--n-supports",
            "0",
            "--json",
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "report health athena_ii should succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // ADR-0005 (2026-04-20): lift_speed_mm_min + ref_lift_speed_mm_min moved off
    // PrinterProfile. With no --resin flag the CLI defaults to resin=generic_standard,
    // whose recipe has lift_speed_mm_min=60 (matching ref_lift_speed_mm_min=60). So
    // speed factor = 1.0, peel ≈ 46.8 N, deflection ≈ 31.2 µm (still gated by Athena II
    // z_stiffness=1500 N/mm — the original bug this test defends against).
    //
    // Pre-ADR-0005: Athena II's baked lift_speed=90 gave speed factor 1.076 → 33.6 µm.
    // The 31.2 µm result is the CORRECT new behaviour — the resin recipe drives lift
    // speed, not the printer; generic_standard at 60 mm/min happens to match ref_lift.
    //
    // Pre-ADR-0004 (the original regression this test defends against): silent fallback
    // to generic_msla_4k k=460 produced 101.7 µm.
    let defl = extract_f64(&stdout, "\"max_z_deflection_um\":");
    assert!(
        (29.0..=33.0).contains(&defl),
        "expected ~31.2 µm Athena II deflection post-ADR-0005, got {defl} \
         (pre-ADR-0004 silent-generic-fallback was 101.7 µm — that regression would re-appear as >100)"
    );
    std::fs::remove_dir_all(&cube).ok();
}

#[test]
fn report_health_unknown_printer_hard_errors_with_available_list() {
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "report",
            "health",
            "--stl",
            "/tmp/no-such-file.stl",
            "--data-dir",
        ])
        .arg(&data)
        .args(["--printer", "bogus_printer_name", "--json"])
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
