//! Step definitions for
//! `spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md` UAT-1
//! (uat-unskip-cli-sim-voxel-cure-emits-tier2-thermal-log-impl).
//!
//! SYMBOL VERIFICATION. Every entry point in the scenario is
//! `#[cfg(feature = "field-sim")]`:
//!   - CLI `--voxel-cure-mm` flag (main.rs:234)
//!   - `apply_voxel_thermal_for_layer` (simulation_runner.rs:1520)
//!   - tier-2 thermal info line emission (simulation_runner.rs:1573-1586)
//!   - tier-2 thermal complete line emission (simulation_runner.rs:1250-1261)
//!
//! See docs/patterns/band-membership-by-symbol.md.
//!
//! GIVEN REGISTRATION REUSE. The Given step `a CTB input with per-layer
//! masks` is already registered by
//! `light_crosstalk_3d_gaussian_convolution_runtime.rs:106`, which sets
//! `world.ctb_layer_inputs`. This module does NOT re-register it — that
//! would produce a runtime ambiguous-match error. The side-effect
//! (`ctb_layer_inputs` populated) is harmless: this module's When creates
//! its own NanoDlpJobBuilder fixture independently for the CLI subprocess,
//! ignoring the in-process LayerInputs.
//!
//! FIXTURE APPROACH. Uses `NanoDlpJobBuilder` (fixtures.rs) to synthesise
//! a `.nanodlp` archive with per-layer masks. The voxel path at
//! main.rs:1785 accepts both CTB and NANODLP formats, so a nanodlp
//! archive exercises the same `run_from_layer_inputs_with_voxel` entry
//! point a CTB file would. The printer and resin TOML files are written
//! to a per-scenario temp directory from the committed `data/` fixtures
//! (Mars 5 Ultra, Generic Standard). `invoke_resinsim_field_sim` runs the
//! field-sim binary end-to-end.
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory.
//! The two Givens unique to this module (`^a Mars 5 Ultra printer profile
//! \(with field-sim thermal fields populated\)$` and `^the Generic
//! Standard resin$`) have no collisions. The When's backtick-delimited
//! `resinsim sim --voxel-cure-mm 0.5 --file <CTB>` text is distinct from
//! every other When registration by the combination of `--voxel-cure-mm`
//! + `--file <CTB>` + `runs to completion`. The five Thens are all unique
//! prefixes/tokens not registered elsewhere.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim_field_sim, workspace_data_dir};
use super::fixtures::{unique_tmp_dir, NanoDlpJobBuilder};
use super::world::UatWorld;

// ---- UAT-1: Tier-2 activation emits exactly one info + one summary line -----
// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1

// "Given a CTB input with per-layer masks" — REUSED from
// light_crosstalk_3d_gaussian_convolution_runtime.rs:106. Not re-registered.
// Sets world.ctb_layer_inputs (harmless side-effect; this module ignores it).

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[given(
    regex = r"^a Mars 5 Ultra printer profile \(with field-sim thermal fields populated\)$"
)]
fn given_mars5_ultra_printer(world: &mut UatWorld) {
    let dir = unique_tmp_dir("thermal-log-uat1");
    world.cli_tmp_dir = Some(dir);
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[given(regex = r"^the Generic Standard resin$")]
fn given_generic_standard_resin(_world: &mut UatWorld) {
    // Narrative — profile resolution goes through --data-dir +
    // --resin/--printer NAME pairs, not file paths. The data/ directory
    // already contains generic_standard.toml and elegoo_mars5_ultra.toml.
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[when(
    regex = r"^`resinsim sim --voxel-cure-mm 0\.5 --file <CTB> --resin <resin> \\ --printer <printer> --initial-led-temp 27 --ambient 22 \\ --out model\.sim\.json` runs to completion$"
)]
fn when_resinsim_sim_voxel_cure(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: Given populated cli_tmp_dir")
        .clone();

    let nanodlp = NanoDlpJobBuilder::new().build("thermal-log-uat1-ctb");
    let data_dir = workspace_data_dir();
    let out_path = dir.join("model.sim.json");

    let outcome = invoke_resinsim_field_sim(
        &[
            "sim",
            "--voxel-cure-mm",
            "0.5",
            "--file",
            nanodlp.path.to_str().expect("nanodlp path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "elegoo_mars5_ultra",
            "--data-dir",
            data_dir.to_str().expect("data dir is UTF-8"),
            "--initial-led-temp",
            "27",
            "--ambient",
            "22",
            "--out",
            out_path.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "resinsim sim --voxel-cure-mm must exit 0, got {}.\nstderr: {}",
        outcome.exit_code, outcome.stderr,
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
    world.sim_json_path = Some(out_path);
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[then(regex = r"^exactly one line on stderr starts with `tier-2 thermal: voxel_size=`$")]
fn then_exactly_one_info_line(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When populated cli_stderr");
    let count = stderr
        .lines()
        .filter(|l| l.starts_with("tier-2 thermal: voxel_size="))
        .count();
    assert_eq!(
        count, 1,
        "expected exactly 1 line starting with 'tier-2 thermal: voxel_size=', found {count}.\nstderr:\n{stderr}",
    );
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[then(
    regex = r"^that line carries `α=`, `k_resin=`, `h_top=`, `h_side\(lumped\)=` tokens for operator-side calibration debugging$"
)]
fn then_info_line_carries_calibration_tokens(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When populated cli_stderr");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("tier-2 thermal: voxel_size="))
        .expect("info line must exist (previous Then asserted it)");
    for token in &["α=", "k_resin=", "h_top=", "h_side(lumped)="] {
        assert!(
            line.contains(token),
            "info line must carry token '{token}', got: {line}",
        );
    }
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[then(
    regex = r"^exactly one line on stderr starts with `tier-2 thermal complete: total_substeps=`$"
)]
fn then_exactly_one_complete_line(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When populated cli_stderr");
    let count = stderr
        .lines()
        .filter(|l| l.starts_with("tier-2 thermal complete: total_substeps="))
        .count();
    assert_eq!(
        count, 1,
        "expected exactly 1 line starting with 'tier-2 thermal complete: total_substeps=', found {count}.\nstderr:\n{stderr}",
    );
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[then(
    regex = r"^the complete line carries `max_T=`, `volume_mean=`, `wall_clock=` tokens$"
)]
fn then_complete_line_carries_tokens(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When populated cli_stderr");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("tier-2 thermal complete: total_substeps="))
        .expect("complete line must exist (previous Then asserted it)");
    for token in &["max_T=", "volume_mean=", "wall_clock="] {
        assert!(
            line.contains(token),
            "complete line must carry token '{token}', got: {line}",
        );
    }
}

// spec/uat/cli-sim-voxel-cure-emits-tier2-thermal-log.md UAT-1
#[then(
    regex = r"^neither line appears when `--voxel-cure-mm` is absent \(Tier-1 path\)$"
)]
fn then_tier1_path_has_no_thermal_lines(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: Given populated cli_tmp_dir")
        .clone();
    let data_dir = workspace_data_dir();
    let tier1_out = dir.join("tier1.sim.json");

    let nanodlp = NanoDlpJobBuilder::new().build("thermal-log-uat1-tier1");
    let outcome = invoke_resinsim_field_sim(
        &[
            "sim",
            "--file",
            nanodlp.path.to_str().expect("nanodlp path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "elegoo_mars5_ultra",
            "--data-dir",
            data_dir.to_str().expect("data dir is UTF-8"),
            "--initial-led-temp",
            "27",
            "--ambient",
            "22",
            "--out",
            tier1_out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "Tier-1 resinsim sim (no --voxel-cure-mm) must exit 0, got {}.\nstderr: {}",
        outcome.exit_code, outcome.stderr,
    );
    assert!(
        !outcome.stderr.contains("tier-2 thermal: voxel_size="),
        "Tier-1 path must NOT emit the tier-2 thermal info line.\nstderr:\n{}",
        outcome.stderr,
    );
    assert!(
        !outcome
            .stderr
            .contains("tier-2 thermal complete: total_substeps="),
        "Tier-1 path must NOT emit the tier-2 thermal complete line.\nstderr:\n{}",
        outcome.stderr,
    );
}
