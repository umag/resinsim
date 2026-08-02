//! CLI regression tests for the KB-153 Ea_cure default warning across every
//! profile-loading subcommand (`sim`, `inspect thermal`, and — by the same
//! `profile_loader::load_resin` seam — `inspect cure` / `inspect force` /
//! `inspect zaxis` / `inspect calibrate`), plus the --initial-led-temp
//! parse-time validation (step-10 adversarial fix).
//!
//! These tests pair with the ResinProfile.cure_kinetics_ea_kj_mol + CLI arg
//! validation changes. They use std::process::Command + env!("CARGO_BIN_EXE_resinsim")
//! so the full CLI surface (clap parsing → subcommand dispatch → stderr
//! emission) is exercised.
//!
//! Per profile_loader_cli.rs: nextest's CWD for CLI tests is the crate root
//! (crates/resinsim-inspect), so --data-dir must be passed explicitly.
//!
//! As of the KB-153 seam fix (`profile_loader::load_resin` is the single
//! emission point), the warning surfaces once on EVERY subcommand that loads
//! a resin profile whose TOML omits `cure_kinetics_ea_kj_mol` — including
//! `sim` (the ADR-0015 producer, previously silent) and `inspect thermal`
//! without `--printer` (previously silent; the emission used to live inside
//! the two-stage `--printer`-gated branch only). `inspect thermal --json`
//! now also carries the warning on stderr (stdout JSON is untouched).
//! `report health --in` remains a deliberate exception — it consumes a
//! sim.json envelope and loads no TOML at all — pinned below.

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

/// KB-153 regression: `resinsim sim` is the ADR-0015 producer surface and
/// must warn when the loaded resin's TOML omits `cure_kinetics_ea_kj_mol`,
/// same as `inspect thermal` already does. Pre-fix this is RED — `cmd_sim`
/// calls `profile_loader::resolve_profiles` -> `load_resin`, which emits
/// nothing; the warning lived only in `cmd_thermal`'s local `eprintln!`.
#[test]
fn sim_warns_when_cure_kinetics_ea_default_used() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("sim-warns-default");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(out_dir.join("out.sim.json"))
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "sim command failed: {:?}", out);
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

/// KB-153 negative case on the `sim` producer surface: a resin TOML with a
/// measured `cure_kinetics_ea_kj_mol` must not trigger the default-value
/// warning.
#[test]
fn sim_does_not_warn_when_cure_kinetics_ea_measured() {
    let data = data_dir_with_measured_ea(42.0);
    let stl = workspace_data_dir().join("test_cube.stl");
    let out_dir = tmpdir("sim-no-warn-measured");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(out_dir.join("out.sim.json"))
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "sim command failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("30 kJ/mol"),
        "stderr must NOT warn about the default when resin has measured Ea:\n{stderr}"
    );
}

/// Double-emission guard on the producer path: `sim` must warn EXACTLY
/// once, not once per internal call into the loader seam (e.g. if
/// `resolve_profiles` grew its own emission alongside `load_resin`'s).
#[test]
fn sim_warns_exactly_once() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("sim-warns-once");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(out_dir.join("out.sim.json"))
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "sim command failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("KB-153").count(),
        1,
        "warning must appear exactly once on the sim producer path:\n{stderr}"
    );
}

/// Double-emission guard on `inspect thermal` — this must stay green
/// unmodified across the seam move: it pins behaviour that must NOT change
/// (the two-stage `--printer`-gated path already warned exactly once before
/// this issue; it must still warn exactly once, from the new seam, after).
#[test]
fn thermal_warns_exactly_once() {
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
    assert_eq!(
        stderr.matches("KB-153").count(),
        1,
        "warning must appear exactly once:\n{stderr}"
    );
}

/// Behaviour widening (a): `inspect thermal --json` must ALSO carry the
/// warning on stderr (stdout JSON is a separate stream and is untouched —
/// it already carries `cure_kinetics_ea_is_default: true`). Pre-fix this is
/// RED — `cmd_thermal`'s local emission is guarded by `&& !json`.
#[test]
fn thermal_json_warns_on_stderr_and_stdout_stays_valid_json() {
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
            "--json",
            "--data-dir",
        ])
        .arg(&data)
        .output()
        .expect("spawn resinsim");
    assert!(out.status.success(), "command failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("30 kJ/mol")
            && stderr.contains("literature midpoint estimate")
            && stderr.contains("KB-153"),
        "stderr must carry the warning even in --json mode:\n{stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must remain valid JSON even though stderr also emits");
    assert_eq!(
        value["cure_kinetics_ea_is_default"],
        serde_json::json!(true),
        "stdout JSON must still carry the flag: {value}"
    );
}

/// Behaviour widening (b): `inspect thermal --resin X` WITHOUT `--printer`
/// (the legacy single-stage path) must also warn — the resin is still
/// loaded via `profile_loader::load_resin` regardless of whether a printer
/// profile is also supplied. Pre-fix this is RED — the pre-move emission
/// site lived only inside the `--printer`-gated two-stage branch.
#[test]
fn thermal_warns_without_printer_flag() {
    let data = workspace_data_dir();
    let out = Command::new(bin())
        .args([
            "inspect",
            "thermal",
            "--layers",
            "10",
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
        stderr.contains("30 kJ/mol")
            && stderr.contains("literature midpoint estimate")
            && stderr.contains("KB-153"),
        "stderr must warn even without --printer:\n{stderr}"
    );
}

/// Pins the ADR-0015 consumer boundary: `report health --in` loads a
/// sim.json envelope, not a resin TOML, so it CANNOT go through
/// `profile_loader::load_resin` and must stay silent. This is intentional
/// and must NOT regress — a later reader must not "fix" this by threading
/// --resin/--printer flags back into `report health`. Surfacing the flag on
/// this consumer would require `cure_kinetics_ea_is_default` in the
/// sim.json v2 envelope schema; tracked as a follow-up issue,
/// `sim-json-envelope-ea-default-flag` — remove this pin when that lands.
#[test]
fn report_health_in_does_not_warn() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("report-health-no-warn");
    let sim_out = out_dir.join("out.sim.json");
    let sim = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim sim");
    assert!(sim.status.success(), "sim command failed: {:?}", sim);

    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim report health");
    assert!(out.status.success(), "report health failed: {:?}", out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("KB-153") && !stderr.contains("30 kJ/mol"),
        "report health --in loads no resin TOML and must stay silent on the \
         KB-153 warning:\n{stderr}"
    );
}
