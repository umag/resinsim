//! Step definitions for
//! `spec/uat/thermal-field-sidecar-roundtrip.md` (all 3 scenarios;
//! uat-unskip-thermal-fields).
//!
//! FIELD-SIM-GATED: every scenario's entry points are `#[cfg(feature =
//! "field-sim")]` — symbol derivation:
//!  - UAT-1: `SimulationRunner::run_from_layer_inputs_with_voxel`
//!    (simulation_runner.rs:446-448), `save_with_provenance` →
//!    `encode_paired_sidecar` (simulation_repo.rs), `PrintSimulation::
//!    thermal_field()` (print_simulation.rs:321)
//!  - UAT-2: `load_envelope` → `load_and_install_sidecar_with_budget`
//!    (simulation_repo.rs), `PrintSimulation::thermal_field()`
//!    (print_simulation.rs:321)
//!  - UAT-3: `load_envelope` → `load_and_install_sidecar_with_budget`
//!    → sidecar decoder (sidecar/mod.rs:36 `#![cfg(feature =
//!    "field-sim")]`), `invoke_resinsim_field_sim` CLI
//!
//! See docs/patterns/band-membership-by-symbol.md.
//!
//! Fixture approach: produce a real paired `sim.json` + `fields.bin`
//! via in-process `save_with_provenance` (same approach as
//! `cli_sim_rejects_tampered_sidecar.rs`). UAT-3 overwrites the
//! `fields.bin` with a forged v1-header sidecar.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

use super::cli_fixtures::invoke_resinsim_field_sim;
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22°C is a valid ambient")
}

fn solid_3x3_mask() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask is valid")
}

fn default_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 20,
    }
}

fn layer_inputs_with_mask(n: u32) -> Vec<LayerInput> {
    (0..n)
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
            li.mask = Some(solid_3x3_mask());
            li
        })
        .collect()
}

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
    let layers = layer_inputs_with_mask(5);
    SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5),
        None,
    )
    .expect("voxel-mode run on validated profiles must succeed")
}

// ---------------------------------------------------------------------------
// UAT-1: --voxel-cure-mm emits a thermal sidecar payload
// ---------------------------------------------------------------------------

// `Given a CTB input with per-layer masks` is already registered by
// light_crosstalk_3d_gaussian_convolution_runtime.rs — reuse it (cucumber
// step defs are global; a second registration would cause an ambiguity
// error). The existing registration populates world.ctb_layer_inputs; the
// When step below calls produce_voxel_sim() directly.

#[given(
    regex = r"^a resin and printer profile validated against the recipe \(under field-sim\)$"
)]
fn given_resin_printer_validated_field_sim(_world: &mut UatWorld) {
    // Uses generic_standard() + generic_msla_4k(), both factory-validated.
}

#[when(
    regex = r"^the user invokes `resinsim sim --file <CTB> --resin <resin> \\ --printer <printer> --voxel-cure-mm 0\.05 --out model\.sim\.json`$"
)]
fn when_voxel_sim_produces_sidecar(world: &mut UatWorld) {
    let sim = produce_voxel_sim();
    let dir = unique_tmp_dir("thermal-sidecar-uat1");
    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    world.sim_json_path = Some(sim_json);
    world.sim_primary = Some(sim);
    world.cli_tmp_dir = Some(dir);
}

#[then(regex = r"^a file `model\.fields\.bin` is written alongside `model\.sim\.json`$")]
fn then_fields_bin_exists(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let fields_bin = dir.join("model.fields.bin");
    assert!(
        fields_bin.is_file(),
        "fields.bin must exist alongside sim.json at {}",
        fields_bin.display()
    );
}

#[then(
    regex = r#"^`model\.sim\.json` carries `fields_sidecar\.fields_present` including `"thermal"`$"#
)]
fn then_sim_json_carries_thermal_present(world: &mut UatWorld) {
    let sim_json = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: When step populated sim_json_path");
    let contents =
        std::fs::read_to_string(sim_json).expect("sim.json must be readable");
    assert!(
        contents.contains("\"thermal\""),
        "sim.json must carry \"thermal\" in fields_present, got: {}",
        &contents[..contents.len().min(500)]
    );
}

#[then(regex = r"^the sidecar's RSFIELD header carries `format_version = 2`$")]
fn then_sidecar_format_version_2(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .as_ref()
        .expect("scenario invariant: When step populated cli_tmp_dir");
    let fields_bin = dir.join("model.fields.bin");
    let bytes = std::fs::read(&fields_bin).expect("fields.bin must be readable");
    assert!(
        bytes.len() >= 12,
        "sidecar must be at least 12 bytes (magic + version)"
    );
    assert_eq!(
        &bytes[..8],
        b"RSFIELD\0",
        "sidecar must start with RSFIELD magic"
    );
    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    assert_eq!(
        version, 2,
        "sidecar format_version must be 2, got {version}"
    );
}

#[then(
    regex = r"^the sidecar's descriptor stream carries a kind_tag=4 entry whose `dim_x × dim_y × dim_z × voxel_size_mm` matches the printer's `build_envelope_mm` \(NOT the part bbox the other four kinds use\)$"
)]
fn then_thermal_descriptor_matches_envelope(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let tf = sim
        .thermal_field()
        .expect("voxel-mode sim must populate thermal_field");
    let (nx, ny, nz) = tf.dimensions();
    assert!(
        nx > 0 && ny > 0 && nz > 0,
        "thermal field dims must be positive: ({nx}, {ny}, {nz})"
    );
    let voxel_mm = tf.voxel_size_mm();
    let envelope = PrinterProfile::generic_msla_4k()
        .build_envelope_mm()
        .expect("generic_msla_4k has build_envelope_mm");
    let expected_physical_x = nx as f32 * voxel_mm;
    let expected_physical_y = ny as f32 * voxel_mm;
    assert!(
        (expected_physical_x - envelope.width_mm).abs() < voxel_mm + 0.01
            && (expected_physical_y - envelope.depth_mm).abs() < voxel_mm + 0.01,
        "thermal field physical extent ({expected_physical_x}×{expected_physical_y} mm) \
         must approximate the build envelope ({w}×{d} mm) within one voxel",
        w = envelope.width_mm,
        d = envelope.depth_mm,
    );
}

// ---------------------------------------------------------------------------
// UAT-2: reload reattaches the thermal field with byte-identical values
// ---------------------------------------------------------------------------

#[given(
    regex = r"^a previously-saved `<stem>\.sim\.json` \+ `<stem>\.fields\.bin` pair from a voxel-mode run with a populated thermal_field$"
)]
fn given_saved_pair(world: &mut UatWorld) {
    let sim = produce_voxel_sim();
    assert!(
        sim.thermal_field().is_some(),
        "voxel-mode sim must have thermal_field"
    );
    let dir = unique_tmp_dir("thermal-sidecar-uat2");
    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    world.sim_json_path = Some(sim_json);
    world.sim_primary = Some(sim);
    world.cli_tmp_dir = Some(dir);
}

#[when(
    regex = r"^the user invokes `resinsim report health --in <stem>\.sim\.json`$"
)]
fn when_load_envelope(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: Given populated sim_json_path");
    let loaded = resinsim_core::repositories::load_envelope(path)
        .expect("load_envelope must succeed on a valid pair");
    world.sim_alt = Some(loaded.simulation);
}

#[then(
    regex = r"^the loaded `PrintSimulation` has `sim\.thermal_field\(\)\.is_some\(\)`$"
)]
fn then_loaded_thermal_field_some(world: &mut UatWorld) {
    let loaded = world
        .sim_alt
        .as_ref()
        .expect("scenario invariant: When step populated sim_alt");
    assert!(
        loaded.thermal_field().is_some(),
        "loaded sim must have thermal_field reattached from sidecar"
    );
}

#[then(
    regex = r"^the loaded thermal_field's dimensions, voxel_size_mm, and bbox_min_mm match the pre-save values byte-for-byte$"
)]
fn then_dims_match(world: &mut UatWorld) {
    let original = world
        .sim_primary
        .as_ref()
        .expect("sim_primary is the pre-save sim")
        .thermal_field()
        .expect("pre-save sim has thermal_field");
    let loaded = world
        .sim_alt
        .as_ref()
        .expect("sim_alt is the loaded sim")
        .thermal_field()
        .expect("loaded sim has thermal_field");
    assert_eq!(
        original.dimensions(),
        loaded.dimensions(),
        "dimensions must match"
    );
    assert_eq!(
        original.voxel_size_mm().to_bits(),
        loaded.voxel_size_mm().to_bits(),
        "voxel_size_mm must be bit-exact"
    );
    assert_eq!(
        original.bbox_min_mm(),
        loaded.bbox_min_mm(),
        "bbox_min_mm must match"
    );
}

#[then(
    regex = r"^every voxel's f32 temperature matches the pre-save value byte-for-byte \(zstd is lossless; the encoder pins the level explicitly for cross-run determinism\)$"
)]
fn then_voxel_temps_byte_exact(world: &mut UatWorld) {
    let original = world
        .sim_primary
        .as_ref()
        .expect("sim_primary is the pre-save sim")
        .thermal_field()
        .expect("pre-save sim has thermal_field");
    let loaded = world
        .sim_alt
        .as_ref()
        .expect("sim_alt is the loaded sim")
        .thermal_field()
        .expect("loaded sim has thermal_field");
    let (nx, ny, nz) = original.dimensions();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let orig_t = original
                    .temperature_at(ix, iy, iz)
                    .expect("original voxel in bounds");
                let load_t = loaded
                    .temperature_at(ix, iy, iz)
                    .expect("loaded voxel in bounds");
                assert_eq!(
                    orig_t.to_bits(),
                    load_t.to_bits(),
                    "temperature at ({ix},{iy},{iz}) must be bit-exact: \
                     original={orig_t}, loaded={load_t}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UAT-3: legacy format_version = 1 sidecars are rejected
// ---------------------------------------------------------------------------

#[given(
    regex = r"^a `model\.sim\.json` whose `model\.fields\.bin` carries the legacy RSFIELD `format_version = 1` header$"
)]
fn given_v1_sidecar(world: &mut UatWorld) {
    let sim = produce_voxel_sim();
    let dir = unique_tmp_dir("thermal-sidecar-uat3");
    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance must succeed");
    let fields_bin = dir.join("model.fields.bin");
    assert!(fields_bin.is_file(), "sidecar must exist after save");

    // Forge a v1-header sidecar: RSFIELD magic + format_version=1 + padding.
    let mut forged = Vec::new();
    forged.extend_from_slice(b"RSFIELD\0");
    forged.extend_from_slice(&1u32.to_le_bytes()); // format_version = 1
    forged.extend_from_slice(&0u32.to_le_bytes()); // field_count = 0
    forged.resize(64, 0u8); // pad to SIDECAR_HEADER_LEN
    std::fs::write(&fields_bin, &forged)
        .expect("overwrite fields.bin with v1 header");

    // Update the sim.json's sidecar pointer so sha256/byte_size match the
    // forged bytes — the integrity check runs BEFORE the format_version
    // check, so a stale pointer would trigger "sha256 mismatch" instead
    // of the desired "unknown sidecar format_version" error.
    use sha2::{Digest, Sha256};
    let sha256 = format!("{:x}", Sha256::digest(&forged));
    let json_str = std::fs::read_to_string(&sim_json).expect("read sim.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&json_str).expect("parse sim.json");
    if let Some(sidecar) = json.get_mut("fields_sidecar") {
        sidecar["sha256"] = serde_json::Value::String(sha256);
        sidecar["byte_size"] = serde_json::Value::Number(
            serde_json::Number::from(forged.len() as u64),
        );
    }
    let updated_json =
        serde_json::to_string_pretty(&json).expect("serialize updated sim.json");
    std::fs::write(&sim_json, updated_json).expect("write updated sim.json");

    world.sim_json_path = Some(sim_json);
    world.cli_tmp_dir = Some(dir);
}

#[when(
    regex = r"^the user invokes `resinsim report health --in model\.sim\.json`$"
)]
fn when_report_health_v1_sidecar(world: &mut UatWorld) {
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

#[then(regex = r"^the load fails with a non-zero exit code$")]
fn then_nonzero_exit(world: &mut UatWorld) {
    let code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    assert_ne!(code, 0, "v1 sidecar must produce non-zero exit code");
}

#[then(regex = r#"^stderr names `"unknown sidecar format_version"`$"#)]
fn then_stderr_names_format_version(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When step populated cli_stderr");
    assert!(
        stderr.contains("unknown sidecar format_version"),
        "stderr must contain 'unknown sidecar format_version', got: {stderr}"
    );
}

#[then(regex = r"^stderr surfaces the actual `got=1, expected=2` numbers$")]
fn then_stderr_surfaces_numbers(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When step populated cli_stderr");
    assert!(
        stderr.contains("1") && stderr.contains("2"),
        "stderr must surface got=1, expected=2 numbers, got: {stderr}"
    );
}

#[then(regex = r"^no partial PrintSimulation is constructed$")]
fn then_no_partial_sim(world: &mut UatWorld) {
    let code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    assert_ne!(
        code, 0,
        "non-zero exit code proves no partial simulation was constructed \
         (the CLI would have printed a report otherwise)"
    );
}
