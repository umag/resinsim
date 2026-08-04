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
//! `report health --in` consumes a sim.json envelope, not a resin TOML, so
//! it cannot go through `profile_loader::load_resin` — but the envelope
//! carries the fact via the top-level `cure_kinetics_ea_is_default` flag
//! (stamped by the producer, `resinsim sim`), and `report health` warns
//! from that flag via `profile_loader::warn_if_envelope_ea_is_default`.
//! Its exactly-once guard is `report_health_warns_exactly_once` below.

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

/// sim-json-envelope-ea-default-flag: `resinsim sim` is the ADR-0015
/// producer for the sim.json envelope; it must stamp the top-level
/// `cure_kinetics_ea_is_default` field from the SAME predicate the stderr
/// warning uses, so a downstream `report health --in` consumer can recover
/// the KB-153 fact without re-loading the resin TOML. RED because `cmd_sim`
/// still calls `save_with_provenance`, which never emits the key.
#[test]
fn sim_stamps_ea_default_true_in_envelope() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("sim-stamps-true");
    let sim_out = out_dir.join("out.sim.json");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim sim");
    assert!(out.status.success(), "sim command failed: {:?}", out);

    let bytes = std::fs::read_to_string(&sim_out).expect("read produced sim.json");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse sim.json");
    assert_eq!(
        value["cure_kinetics_ea_is_default"],
        serde_json::json!(true),
        "sim.json envelope must stamp the flag as true for a default-Ea resin: {value}"
    );
}

/// Negative polarity of the stamp: a measured Ea must be stamped `false`,
/// not left absent — absent means "producer did not record it", which is a
/// different claim.
#[test]
fn sim_stamps_ea_default_false_with_measured_resin() {
    let data = data_dir_with_measured_ea(42.0);
    let stl = workspace_data_dir().join("test_cube.stl");
    let out_dir = tmpdir("sim-stamps-false");
    let sim_out = out_dir.join("out.sim.json");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim sim");
    assert!(out.status.success(), "sim command failed: {:?}", out);

    let bytes = std::fs::read_to_string(&sim_out).expect("read produced sim.json");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse sim.json");
    assert_eq!(
        value["cure_kinetics_ea_is_default"],
        serde_json::json!(false),
        "sim.json envelope must stamp the flag as false for a measured-Ea resin: {value}"
    );
}

/// Anti-drift tie: the stamp and the stderr warning must be computed from
/// the SAME predicate in the SAME run. This is what makes "stamped from the
/// same predicate the warning uses" a testable property rather than a
/// code-review claim — it fails if a future change re-derives the flag from
/// a different source than `resolved.resin.cure_kinetics_ea_is_default()`.
#[test]
fn sim_stamp_agrees_with_stderr_warning() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("sim-stamp-agrees");
    let sim_out = out_dir.join("out.sim.json");
    let out = Command::new(bin())
        .args(["sim", "--stl"])
        .arg(&stl)
        .args(["--resin", "generic_standard", "--printer", "generic_msla_4k"])
        .args(["--data-dir"])
        .arg(&data)
        .args(["--out"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim sim");
    assert!(out.status.success(), "sim command failed: {:?}", out);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let bytes = std::fs::read_to_string(&sim_out).expect("read produced sim.json");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse sim.json");
    assert_eq!(
        stderr.contains("KB-153"),
        value["cure_kinetics_ea_is_default"] == serde_json::json!(true),
        "stderr warning presence and the envelope stamp must agree in a single invocation; \
         stderr:\n{stderr}\nenvelope: {value}"
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

/// sim-json-envelope-ea-default-flag: `report health --in` is the
/// envelope-consumer twin of `load_resin`'s KB-153 emission. It loads no
/// resin TOML, but the sim.json envelope now carries the fact as the
/// top-level `cure_kinetics_ea_is_default` flag, stamped by the producer
/// (`resinsim sim`). RED because `cmd_report_health` prints nothing today.
#[test]
fn report_health_warns_when_envelope_flags_ea_default() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("report-health-warns-default");
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
    // Pin the consumer-context line warn_if_envelope_ea_is_default prints
    // around the shared literal: it names that the fact came from the
    // envelope and that the remedy is re-running `resinsim sim`.
    assert!(
        stderr.contains("sim.json envelope") && stderr.contains("resinsim sim"),
        "stderr must name that the fact came from the envelope and that the \
         remedy is re-running `resinsim sim`:\n{stderr}"
    );
}

/// Double-emission guard on the consumer path, mirroring
/// `sim_warns_exactly_once` / `thermal_warns_exactly_once` — the
/// anti-pattern's mandated per-surface detection shape
/// (docs/patterns/anti/warning-duplicated-per-subcommand.md §Detection).
/// This is the exactly-once pin that REPLACES the retired
/// `report_health_in_does_not_warn` silence pin: the envelope now carries
/// the flag, so the consumer warns — and must warn exactly once.
#[test]
fn report_health_warns_exactly_once() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("report-health-warns-once");
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
    assert_eq!(
        stderr.matches("KB-153").count(),
        1,
        "warning must appear exactly once on the report-health consumer path:\n{stderr}"
    );
}

/// Negative polarity: an envelope stamped `false` (measured Ea) must stay
/// silent on the consumer path too.
#[test]
fn report_health_silent_when_envelope_flags_measured_ea() {
    let data = data_dir_with_measured_ea(42.0);
    let stl = workspace_data_dir().join("test_cube.stl");
    let out_dir = tmpdir("report-health-silent-measured");
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
        "report health --in must stay silent when the envelope flags a \
         measured Ea:\n{stderr}"
    );
}

/// The accepted false negative, made explicit and executable: an envelope
/// with the flag stripped in-place (mirroring the provenance-strip
/// technique in `report_health_time_cli.rs`) mimics a pre-flag / older
/// producer and must load fine and stay silent — absence is not `false`,
/// per ADR-0002 and the envelope field's doc comment. A `schema_version: 1`
/// file is now hard-rejected (simulation_repo.rs), so tampering an
/// existing v2 envelope is the only way to exercise this state.
#[test]
fn report_health_silent_on_pre_flag_envelope() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("report-health-pre-flag");
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

    let bytes = std::fs::read_to_string(&sim_out).expect("read envelope");
    let mut value: serde_json::Value =
        serde_json::from_str(&bytes).expect("envelope is valid JSON");
    value
        .as_object_mut()
        .expect("envelope root is an object")
        .remove("cure_kinetics_ea_is_default");
    std::fs::write(
        &sim_out,
        serde_json::to_string_pretty(&value).expect("serialize tampered envelope"),
    )
    .expect("write envelope without the flag");

    let out = Command::new(bin())
        .args(["report", "health", "--in"])
        .arg(&sim_out)
        .output()
        .expect("spawn resinsim report health");
    assert!(
        out.status.success(),
        "report health must still exit 0 on a pre-flag envelope: {:?}",
        out
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("KB-153") && !stderr.contains("30 kJ/mol"),
        "report health --in must stay silent (accepted false negative) when \
         the flag is absent:\n{stderr}"
    );
}

/// Pins the documented non-goal: `report health --json` stdout does NOT
/// gain a `cure_kinetics_ea_is_default` key — a `--json` caller already
/// holds the sim.json it passed to `--in` and can read the flag from the
/// envelope directly. stderr still carries the advisory; the streams are
/// separate, so stdout stays parseable.
#[test]
fn report_health_json_stdout_unchanged_by_advisory() {
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out_dir = tmpdir("report-health-json-unchanged");
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
        .args(["--json"])
        .output()
        .expect("spawn resinsim report health");
    assert!(
        out.status.success(),
        "report health --json failed: {:?}",
        out
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must remain valid JSON");
    assert!(
        value.get("cure_kinetics_ea_is_default").is_none(),
        "report health --json stdout must NOT carry cure_kinetics_ea_is_default; got: {value}"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("KB-153"),
        "stderr must still carry the advisory even in --json mode:\n{stderr}"
    );
}
