//! CLI regression tests for the KB-153 Ea_cure default warning and the
//! --initial-led-temp parse-time validation (step-10 adversarial fix).
//!
//! These tests pair with the ResinProfile.cure_kinetics_ea_kj_mol + CLI arg
//! validation changes. They use std::process::Command + env!("CARGO_BIN_EXE_resinsim")
//! so the full CLI surface (clap parsing → subcommand dispatch → stderr
//! emission) is exercised.
//!
//! Per profile_loader_cli.rs: nextest's CWD for CLI tests is the crate root
//! (crates/resinsim-inspect), so --data-dir must be passed explicitly.

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

/// Copy the workspace data/ tree to a tmp dir and overwrite generic_standard.toml
/// with `cure_kinetics_ea_kj_mol = <value>` appended (for the
/// measured-Ea no-warning case).
fn data_dir_with_measured_ea(ea_value: f32) -> PathBuf {
    let src = workspace_data_dir();
    let dst = tmpdir("measured-ea");
    // Recursive copy via fs_extra-like manual walk — avoids an extra dep.
    fn copy_dir(s: &Path, d: &Path) {
        std::fs::create_dir_all(d).expect("mkdir");
        for entry in std::fs::read_dir(s).expect("readdir") {
            let e = entry.expect("entry");
            let target = d.join(e.file_name());
            if e.file_type().expect("filetype").is_dir() {
                copy_dir(&e.path(), &target);
            } else {
                std::fs::copy(e.path(), &target).expect("copy");
            }
        }
    }
    copy_dir(&src, &dst);
    // Append the measured Ea_cure to generic_standard.toml root table.
    let resin_toml = dst.join("resins").join("generic_standard.toml");
    let original = std::fs::read_to_string(&resin_toml).expect("read resin toml");
    // Insert BEFORE the [recipe] table marker so the field lands at root.
    let patched = original.replace(
        "[recipe]",
        &format!("cure_kinetics_ea_kj_mol = {ea_value}\n\n[recipe]"),
    );
    std::fs::write(&resin_toml, patched).expect("write patched resin toml");
    dst
}

#[test]
fn thermal_warns_when_cure_kinetics_ea_default_used() {
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "thermal",
            "--layers",
            "10",
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "generic_standard",
            "--data-dir",
        ])
        .arg(&data)
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "command failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("30 kJ/mol"),
        "stderr must mention the default Ea value:\n{stderr}"
    );
    assert!(
        stderr.contains("literature midpoint estimate"),
        "stderr must mention the estimate framing:\n{stderr}"
    );
    assert!(
        stderr.contains("KB-153"),
        "stderr must cite KB-153:\n{stderr}"
    );
}

#[test]
fn thermal_does_not_warn_when_cure_kinetics_ea_measured() {
    let data = data_dir_with_measured_ea(42.0);
    let out = Command::new(bin())
        .args([
            "inspect",
            "thermal",
            "--layers",
            "10",
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "generic_standard",
            "--data-dir",
        ])
        .arg(&data)
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "command failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("30 kJ/mol"),
        "stderr must NOT warn about the default when resin has measured Ea:\n{stderr}"
    );
}

#[test]
fn thermal_rejects_invalid_initial_led_temp() {
    let data = workspace_data_dir();
    // Use --flag=value form so clap doesn't interpret `-300` as another flag.
    let out = Command::new(bin())
        .args([
            "inspect",
            "thermal",
            "--layers",
            "10",
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "generic_standard",
            "--initial-led-temp=-300",
            "--data-dir",
        ])
        .arg(&data)
        .output()
        .expect("spawn resinsim");
    assert!(
        !out.status.success(),
        "command must fail on --initial-led-temp=-300"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("initial")
            && (stderr.contains("absolute zero") || stderr.to_lowercase().contains("invalid")),
        "stderr must explain the invalid --initial-led-temp:\n{stderr}"
    );
}

#[test]
fn thermal_rejects_nan_initial_led_temp() {
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "thermal",
            "--layers",
            "10",
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "generic_standard",
            "--initial-led-temp",
            "NaN",
            "--data-dir",
        ])
        .arg(&data)
        .output()
        .expect("spawn resinsim");
    assert!(
        !out.status.success(),
        "command must fail on --initial-led-temp NaN"
    );
}
