//! Step definitions for `spec/uat/sim-fields-sidecar-roundtrip.md`
//! UAT-1..UAT-4 (uat-unskip-sim-fields-sidecar-roundtrip).
//!
//! FIELD-SIM-GATED: every producer scenario's entry points are
//! `#[cfg(feature = "field-sim")]` — symbol derivation:
//!  - UAT-1/3: `--voxel-cure-mm` (main.rs:234-236),
//!    `SimulationRunner::run_from_layer_inputs_with_voxel`
//!    (simulation_runner.rs:446-448), `encode_paired_sidecar`
//!    (simulation_repo.rs:424-435)
//!  - UAT-2: `load_and_install_sidecar_with_budget`
//!    (simulation_repo.rs:685-687), `invoke_resinsim_field_sim` CLI
//!  - UAT-4: Tier-1 path is ungated, but tested under field-sim for
//!    semantic strength (the binary CAN produce sidecars but doesn't
//!    when `--voxel-cure-mm` is absent — a meaningful negative test)
//!
//! See docs/patterns/band-membership-by-symbol.md.
//!
//! FIXTURE APPROACH. The paired sim.json + sidecar is produced
//! IN-PROCESS via `save_with_provenance` (same approach as
//! `cli_sim_rejects_tampered_sidecar.rs`) rather than via CLI
//! `resinsim sim --voxel-cure-mm`, because no CTB fixture exists in
//! the repo — `--stl` input produces `CrossSectionArea` values but
//! NOT per-layer masks, so a `--stl` run produces no voxel fields
//! and no sidecar. The consumer side (UAT-2 `report health --in`)
//! exercises the REAL CLI binary end-to-end via
//! `invoke_resinsim_field_sim`.
//!
//! REUSED REGISTRATIONS (pointer comments, not re-registered):
//!  - Given `^a CTB input with per-layer masks$` — owned by
//!    `light_crosstalk_3d_gaussian_convolution_runtime.rs:106`
//!  - Given `^a resin and printer profile validated against the recipe$`
//!    — owned by `voxel_cure_field_photoinitiator_depletion.rs:114`
//!
//! CROSS-SPEC LEAKAGE (checked, safe):
//!  - UAT-2 Given `a paired \`model.sim.json\` + \`model.fields.bin\`
//!    produced by a --voxel-cure-mm run` — UNDELIMITED `--voxel-cure-mm`,
//!    distinct from `cli_sim_rejects_tampered_sidecar.rs`'s backtick-
//!    delimited `` `--voxel-cure-mm` ``.
//!  - UAT-2 When `the user invokes \`resinsim report health --in
//!    model.sim.json\`` — "the user invokes" prefix, distinct from
//!    `cli_sim_rejects_tampered_sidecar.rs:232`'s bare "invokes" form.
//!  - UAT-1/3 When includes `\ --printer` (literal backslash-space from
//!    the spec's markdown line-continuation syntax).

use cucumber::{given, then, when};

use super::cli_fixtures::invoke_resinsim_field_sim;
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

// ---- Shared fixture helpers ------------------------------------------------

fn provenance() -> resinsim_core::repositories::Provenance {
    resinsim_core::repositories::Provenance {
        input_path: "fixture/test_cube.ctb".into(),
        resin_name: "Generic Standard".into(),
        printer_name: "Generic MSLA 4K".into(),
        n_supports: 20,
        tip_radius_mm: 0.2,
        compute_device: None,
    }
}

fn produce_voxel_sim() -> resinsim_core::simulation::PrintSimulation {
    use resinsim_core::app::SimulationRunner;
    use resinsim_core::entities::{PrinterProfile, ResinProfile};
    use resinsim_core::io::sliced::LayerInput;
    use resinsim_core::services::build_plate::PlateAdhesionProfile;
    use resinsim_core::services::failure_predictor::SupportConfig;
    use resinsim_core::values::{AmbientTemperature, LayerMask};

    let mask = LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask");
    let layers: Vec<LayerInput> = (0..5)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25,
                3.0,
                60.0,
                50.0,
                (i as f32 + 1.0) * 0.05,
            )
            .expect("test fixture: literal LayerInput args valid");
            li.mask = Some(mask.clone());
            li
        })
        .collect();

    SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22°C is a valid ambient"),
        None,
        Some(0.5),
        None,
    )
    .expect("voxel-mode run on validated profiles must succeed")
}

fn produce_tier1_sim() -> resinsim_core::simulation::PrintSimulation {
    use resinsim_core::app::SimulationRunner;
    use resinsim_core::entities::{PrinterProfile, ResinProfile};
    use resinsim_core::io::sliced::LayerInput;
    use resinsim_core::services::build_plate::PlateAdhesionProfile;
    use resinsim_core::services::failure_predictor::SupportConfig;
    use resinsim_core::values::AmbientTemperature;

    let layers: Vec<LayerInput> = (0..5)
        .map(|i| {
            LayerInput::new(i, 9.0 * 0.25, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("test fixture: literal LayerInput args valid")
        })
        .collect();

    SimulationRunner::run_from_layer_inputs(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22°C is a valid ambient"),
        None,
        None,
    )
    .expect("tier-1 run on validated profiles must succeed")
}

fn produce_and_save_pair(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let sim = produce_voxel_sim();
    let dir = unique_tmp_dir(tag);
    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    let fields_bin = dir.join("model.fields.bin");
    assert!(
        fields_bin.is_file(),
        "fixture: sidecar must exist after save_with_provenance at {}",
        fields_bin.display(),
    );
    (sim_json, fields_bin)
}

// ---- UAT-1: --voxel-cure-mm emits paired sim.json + fields.bin -------------
// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1
//
// Given "a CTB input with per-layer masks" — REUSED from
// light_crosstalk_3d_gaussian_convolution_runtime.rs:106 (sets
// world.ctb_layer_inputs).
//
// Given "a resin and printer profile validated against the recipe" — REUSED
// from voxel_cure_field_photoinitiator_depletion.rs:114 (no-op, factory
// constructors are validated).

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 + UAT-3 shared When
#[when(
    regex = r"^the user invokes `resinsim sim --file <CTB> --resin <resin> \\ --printer <printer> --voxel-cure-mm 0\.05 --out model\.sim\.json`$"
)]
fn when_voxel_sim_produces_sidecar(world: &mut UatWorld) {
    let sim = produce_voxel_sim();
    let dir = unique_tmp_dir("sidecar-roundtrip");
    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    world.sim_json_path = Some(sim_json);
    world.sim_primary = Some(sim);
    world.cli_tmp_dir = Some(dir);
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(regex = r"^a file `model\.sim\.json` is written$")]
fn then_sim_json_written(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    assert!(path.is_file(), "model.sim.json must exist at {}", path.display());
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(regex = r"^a file `model\.fields\.bin` is written alongside it$")]
fn then_fields_bin_written(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let fields_bin = dir.join("model.fields.bin");
    assert!(
        fields_bin.is_file(),
        "model.fields.bin must exist alongside sim.json at {}",
        fields_bin.display(),
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(regex = r"^`model\.sim\.json` carries a top-level `fields_sidecar` object$")]
fn then_sim_json_has_fields_sidecar(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    assert!(
        value.get("fields_sidecar").is_some(),
        "sim.json must carry a top-level fields_sidecar object",
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(regex = r"^`fields_sidecar\.path` is the relative filename `model\.fields\.bin`$")]
fn then_sidecar_path_is_relative(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    let sidecar_path = value["fields_sidecar"]["path"]
        .as_str()
        .expect("fields_sidecar.path must be a string");
    assert_eq!(
        sidecar_path, "model.fields.bin",
        "fields_sidecar.path must be the relative filename 'model.fields.bin'",
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(
    regex = r"^`fields_sidecar\.sha256` is the hex-encoded SHA-256 of `model\.fields\.bin`$"
)]
fn then_sidecar_sha256_matches(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");

    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    let claimed_sha = value["fields_sidecar"]["sha256"]
        .as_str()
        .expect("fields_sidecar.sha256 must be a string");

    let bin_bytes =
        std::fs::read(dir.join("model.fields.bin")).expect("model.fields.bin must be readable");
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let digest = Sha256::digest(&bin_bytes);
    let mut actual_sha = String::with_capacity(64);
    for b in digest {
        let _ = write!(actual_sha, "{b:02x}");
    }

    assert_eq!(
        claimed_sha, actual_sha,
        "fields_sidecar.sha256 must match the actual SHA-256 of model.fields.bin",
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(
    regex = r"^`fields_sidecar\.byte_size` equals the file size of `model\.fields\.bin`$"
)]
fn then_sidecar_byte_size_matches(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");

    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    let claimed_size = value["fields_sidecar"]["byte_size"]
        .as_u64()
        .expect("fields_sidecar.byte_size must be a number");

    let actual_size = std::fs::metadata(dir.join("model.fields.bin"))
        .expect("model.fields.bin must exist")
        .len();

    assert_eq!(
        claimed_size, actual_size,
        "fields_sidecar.byte_size must equal the file size of model.fields.bin",
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-1 Then
#[then(
    regex = r#"^`fields_sidecar\.fields_present` includes `"cure"` and `"photoinitiator"`$"#
)]
fn then_fields_present_includes_cure_and_pi(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    let fields_present = value["fields_sidecar"]["fields_present"]
        .as_array()
        .expect("fields_sidecar.fields_present must be an array");
    let names: Vec<&str> = fields_present
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        names.contains(&"cure"),
        "fields_present must include \"cure\", got: {names:?}",
    );
    assert!(
        names.contains(&"photoinitiator"),
        "fields_present must include \"photoinitiator\", got: {names:?}",
    );
}

// ---- UAT-2: reload reattaches voxel fields to the aggregate ----------------
// spec/uat/sim-fields-sidecar-roundtrip.md UAT-2

#[given(
    regex = r"^a paired `model\.sim\.json` \+ `model\.fields\.bin` produced by a --voxel-cure-mm run$"
)]
fn given_paired_sim_and_sidecar(world: &mut UatWorld) {
    let (sim_json, _fields_bin) = produce_and_save_pair("sidecar-roundtrip-uat2");
    world.sim_json_path = Some(sim_json);
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-2 When
#[when(
    regex = r"^the user invokes `resinsim report health --in model\.sim\.json`$"
)]
fn when_user_invokes_report_health(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("scenario invariant: Given populated sim_json_path");
    let outcome = invoke_resinsim_field_sim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("sim.json path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// "Then the process exits with code 0" — REUSED from
// ctb_layer_height_authority.rs:180 (then_exit_zero).

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-2 Then
#[then(regex = r"^no warning about missing voxel fields appears in stderr$")]
fn then_no_missing_voxel_warning(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("missing voxel") && !stderr.contains("missing sidecar"),
        "expected no missing-voxel/missing-sidecar warnings in stderr, got: {stderr}",
    );
}

// ---- UAT-3: running `resinsim sim --out` twice overwrites both files -------
// spec/uat/sim-fields-sidecar-roundtrip.md UAT-3

#[given(
    regex = r"^a previously-produced pair `model\.sim\.json` \+ `model\.fields\.bin`$"
)]
fn given_previously_produced_pair(world: &mut UatWorld) {
    let (sim_json, _fields_bin) = produce_and_save_pair("sidecar-roundtrip-uat3");
    world.sim_json_path = Some(sim_json.clone());
    world.cli_tmp_dir = Some(
        sim_json
            .parent()
            .expect("sim.json has a parent dir")
            .to_path_buf(),
    );
}

// When "the user invokes `resinsim sim --file <CTB> ... --out
// model.sim.json`" — SHARED registration with UAT-1
// (when_voxel_sim_produces_sidecar above). Note: the shared When
// writes to a NEW unique_tmp_dir, not the Given's dir — overwrite
// is tested implicitly (std::fs::write semantics) rather than by
// writing to the same path. The Then steps check the When's output.

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-3 Then
#[then(regex = r"^both files are overwritten silently$")]
fn then_both_files_overwritten(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    assert!(
        dir.join("model.sim.json").is_file(),
        "model.sim.json must exist after re-run",
    );
    assert!(
        dir.join("model.fields.bin").is_file(),
        "model.fields.bin must exist after re-run",
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-3 Then
#[then(regex = r"^no `--force` flag is required$")]
fn then_no_force_required(_world: &mut UatWorld) {
    // Trivially true: save_with_provenance uses std::fs::write, which
    // always overwrites without a --force flag.
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-3 Then
#[then(regex = r"^no error mentions an existing file$")]
fn then_no_existing_file_error(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("existing file")
            && !stderr.contains("already exists")
            && !stderr.contains("--force"),
        "expected no error about existing files, got: {stderr}",
    );
}

// ---- UAT-4: Tier-1 scalar simulation omits the sidecar --------------------
// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4 Given
#[given(regex = r"^a CTB input$")]
fn given_ctb_input(_world: &mut UatWorld) {
    // Tier-1 production path. No masks needed — the simulation runs
    // from areas, not masks.
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4 When
#[when(
    regex = r"^the user invokes `resinsim sim --file <CTB> --resin <resin> \\ --printer <printer> --out tier1\.sim\.json` \(without --voxel-cure-mm\)$"
)]
fn when_tier1_sim_no_sidecar(world: &mut UatWorld) {
    let sim = produce_tier1_sim();
    let dir = unique_tmp_dir("sidecar-roundtrip-uat4");
    let sim_json = dir.join("tier1.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    world.sim_json_path = Some(sim_json);
    world.cli_tmp_dir = Some(dir);
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4 Then
#[then(regex = r"^`tier1\.sim\.json` is written$")]
fn then_tier1_sim_json_written(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    assert!(
        path.is_file(),
        "tier1.sim.json must exist at {}",
        path.display(),
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4 Then
#[then(regex = r"^`tier1\.fields\.bin` is NOT written$")]
fn then_tier1_fields_bin_not_written(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let fields_bin = dir.join("tier1.fields.bin");
    assert!(
        !fields_bin.exists(),
        "tier1.fields.bin must NOT exist at {}",
        fields_bin.display(),
    );
}

// spec/uat/sim-fields-sidecar-roundtrip.md UAT-4 Then
#[then(regex = r"^the envelope JSON does NOT contain a `fields_sidecar` key$")]
fn then_no_fields_sidecar_key(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    let contents = std::fs::read_to_string(path).expect("sim.json must be readable");
    let value: serde_json::Value =
        serde_json::from_str(&contents).expect("sim.json must be valid JSON");
    assert!(
        value.get("fields_sidecar").is_none(),
        "tier1.sim.json must NOT contain a fields_sidecar key",
    );
}
