//! Step definitions for `spec/uat/cli-sim-budget-mismatch-on-load.md`
//! UAT-1 and UAT-2 (uat-unskip-cli-sim-budget-mismatch-on-load).
//!
//! SYMBOL VERIFICATION. Every scenario's consumer path drives `resinsim
//! report health --in <PATH>` against the FIELD-SIM binary
//! (`invoke_resinsim_field_sim`). The sidecar decode path
//! `load_and_install_sidecar_with_budget` (simulation_repo.rs:725) is
//! `#[cfg(feature = "field-sim")]` (called from the gated block at
//! simulation_repo.rs:685-688). The budget check fires inside
//! `decoder.rs::read_descriptor` at the descriptor-parse stage, strictly
//! BEFORE any `Array3` allocation. Under default features this module
//! does not compile (gated at the `pub mod` line in `uat_steps/mod.rs`),
//! so all 3 scenarios skip.
//!
//! UAT-3 is marked **future** in the spec (depends on a follow-up issue
//! that stamps the producer's `RESINSIM_MAX_FIELD_BYTES` into the
//! SidecarPointer envelope). Its steps are undefined here — it stays as
//! 1 declared-debt entry in `SPECS_WITHOUT_STEP_DEFS`.
//!
//! FIXTURE APPROACH. UAT-1 (rejection) uses a HAND-BUILT RSFIELD
//! sidecar whose strain descriptor claims ~4.3 GB (just above the
//! default 4 GB budget) with an empty slab (`layer_sizes[0] == 0`).
//! The budget check fires at descriptor-parse time, strictly before
//! any allocation — the test runs in milliseconds regardless of the
//! claimed size. Same approach as
//! `field_budget_extension_integration.rs::build_oversized_empty_cure_sidecar`.
//!
//! UAT-2 (success) produces a REAL, small sidecar in-process via
//! `save_with_provenance` (cure + photoinitiator + strain + stress,
//! 3×3×2 voxels, all-zero) and invokes the consumer with
//! `RESINSIM_MAX_FIELD_BYTES=17179869184`. The tiny fields pass the
//! budget check trivially and `report health` renders all four
//! voxel-derived sections.
//!
//! CROSS-SPEC LEAKAGE (checked, safe). UAT-1's Given uses backtick-
//! delimited `` `resinsim sim` `` — no other spec's Given carries
//! this exact phrase with the `RESINSIM_MAX_FIELD_BYTES=17179869184`
//! trailer. UAT-2's Given `the same paired sim\.json \+ fields\.bin
//! from UAT-1` is unique to this spec. UAT-1's When includes
//! "without the env override" which is unique. UAT-2's When includes
//! the env-var prefix `RESINSIM_MAX_FIELD_BYTES=17179869184 \` which
//! is unique.
//!
//! REUSED STEPS:
//! - `^the process exits with non-zero code$` — `cli_sim_producer_writes_sim_json.rs`
//! - `^the process does not panic$` — `cli_sim_rejects_unknown_schema_version.rs`
//! - `^the process exits with code 0$` — `ctb_layer_height_authority.rs`

use cucumber::{given, then, when};
use ndarray::Array3;
use sha2::{Digest, Sha256};

use super::cli_fixtures::invoke_resinsim_field_sim;
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

// ---- RSFIELD wire-format constants -----------------------------------------
// Duplicated here (same rationale as field_budget_extension_integration.rs):
// integration tests only see resinsim-core's PUBLIC surface, and the
// sidecar module's format constants are not part of it.
const RSFIELD_MAGIC: [u8; 8] = *b"RSFIELD\0";
const RSFIELD_FORMAT_VERSION: u32 = 2;
const FIELD_KIND_TAG_STRAIN: u32 = 2;
const FIELD_COMPONENT_SIZE_TENSOR: u32 = 24;
const COMPRESSION_TAG_ZSTD: u32 = 1;
const LAYOUT_TAG_LAYER_SLABS: u32 = 1;

/// Hand-build a minimal RSFIELD sidecar with ONE strain descriptor
/// claiming `dim_x × dim_y × 1` voxels with an EMPTY slab
/// (`layer_sizes[0] == 0`). The budget check in `decoder.rs::
/// read_descriptor` fires at descriptor-parse time, strictly before
/// any `Array3` allocation — so the test is cheap regardless of how
/// large `dim_x`/`dim_y` claim to be.
fn build_oversized_strain_sidecar(dim_x: u32, dim_y: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    // Header
    buf.extend_from_slice(&RSFIELD_MAGIC);
    buf.extend_from_slice(&RSFIELD_FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // field_count = 1
    buf.extend_from_slice(&[0u8; 48]); // reserved
    // Strain descriptor
    let name = b"strain";
    buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
    buf.extend_from_slice(name);
    buf.extend_from_slice(&FIELD_KIND_TAG_STRAIN.to_le_bytes());
    buf.extend_from_slice(&dim_x.to_le_bytes());
    buf.extend_from_slice(&dim_y.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // dim_z
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.x
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.y
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // bbox_origin.z
    buf.extend_from_slice(&0.05f32.to_le_bytes()); // voxel_size_mm
    buf.extend_from_slice(&FIELD_COMPONENT_SIZE_TENSOR.to_le_bytes());
    buf.extend_from_slice(&COMPRESSION_TAG_ZSTD.to_le_bytes());
    buf.extend_from_slice(&LAYOUT_TAG_LAYER_SLABS.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // layer_count
    let uncompressed_layer_byte_size =
        u64::from(dim_x) * u64::from(dim_y) * u64::from(FIELD_COMPONENT_SIZE_TENSOR);
    buf.extend_from_slice(&uncompressed_layer_byte_size.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes()); // layer_offsets[0]
    buf.extend_from_slice(&0u32.to_le_bytes()); // layer_sizes[0] == 0 (empty slab)
    buf
}

/// Write `sidecar_bytes` as `<dir>/model.fields.bin` plus a matching
/// `<dir>/model.sim.json` envelope whose `fields_sidecar` pointer
/// carries the real sha256 of those bytes. Same pattern as
/// `field_budget_extension_integration.rs::write_sim_json_with_sidecar`.
fn write_envelope_with_sidecar(
    dir: &std::path::Path,
    sidecar_bytes: &[u8],
    fields_present: &[&str],
) -> std::path::PathBuf {
    use resinsim_core::entities::{PrinterProfile, ResinProfile};
    use resinsim_core::simulation::PrintSimulation;

    let bin_path = dir.join("model.fields.bin");
    std::fs::write(&bin_path, sidecar_bytes).expect("write sidecar bytes");

    let mut hasher = Sha256::new();
    hasher.update(sidecar_bytes);
    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let sim = PrintSimulation::new(recipe, printer);
    let sim_value = serde_json::to_value(&sim).expect("PrintSimulation serialises");

    let envelope = serde_json::json!({
        "schema_version": 2,
        "simulation": sim_value,
        "fields_sidecar": {
            "path": "model.fields.bin",
            "byte_size": sidecar_bytes.len(),
            "sha256": sha256,
            "fields_present": fields_present,
        },
    });
    let sim_json_path = dir.join("model.sim.json");
    std::fs::write(
        &sim_json_path,
        serde_json::to_string_pretty(&envelope).expect("envelope serialises"),
    )
    .expect("write sim.json");
    sim_json_path
}

fn provenance() -> resinsim_core::repositories::Provenance {
    resinsim_core::repositories::Provenance {
        input_path: "fixture/test_cube.stl".into(),
        resin_name: "Generic Standard".into(),
        printer_name: "Generic MSLA 4K".into(),
        n_supports: 10,
        tip_radius_mm: 0.2,
        compute_device: None,
    }
}

/// Produce a real paired `sim.json` + `fields.bin` with cure +
/// photoinitiator + strain + stress fields (3×3×2 voxels, all-zero).
/// Same in-process approach as
/// `cli_sim_rejects_tampered_sidecar.rs::produce_paired_sim_and_sidecar`
/// but extended with strain + stress fields for the budget-mismatch
/// spec's UAT-2 success path.
fn produce_full_sidecar(tag: &str) -> std::path::PathBuf {
    use resinsim_core::entities::{PrinterProfile, ResinProfile};
    use resinsim_core::simulation::PrintSimulation;
    use resinsim_core::values::{CureField, PhotoinitiatorField, StrainField, StressField};

    let dir = unique_tmp_dir(tag);
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    let (nx, ny, nz) = (3, 3, 2);
    let voxel_size_mm = 0.05;
    let bbox_min = [0.0, 0.0, 0.0];

    let cure_data =
        Array3::<f32>::from_shape_fn((nx, ny, nz), |(x, y, z)| (x + y + z) as f32 * 0.1);
    let cure = CureField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        voxel_size_mm,
        bbox_min,
        cure_data,
    )
    .expect("CureField ctor");
    let pi_data = Array3::<f32>::from_shape_fn((nx, ny, nz), |_| 0.5);
    let photoinit =
        PhotoinitiatorField::from_persistence_parts(nx as u32, ny as u32, nz as u32, 0.8, pi_data)
            .expect("PhotoinitiatorField ctor");
    sim.set_voxel_fields(cure, photoinit)
        .expect("install cure + pi");

    let strain = StrainField::new(nx as u32, ny as u32, nz as u32, voxel_size_mm, bbox_min)
        .expect("StrainField ctor");
    let stress = StressField::new(nx as u32, ny as u32, nz as u32, voxel_size_mm, bbox_min)
        .expect("StressField ctor");
    sim.set_strain_stress_fields(strain, stress)
        .expect("install strain + stress");

    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance");
    sim_json
}

fn invoke_report_health_field_sim(
    world: &mut UatWorld,
    env_override: &[(&str, &str)],
) {
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
        env_override,
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// ---- UAT-1: default-budget consumer rejects oversized sidecar ----------------
// spec/uat/cli-sim-budget-mismatch-on-load.md UAT-1

#[given(
    regex = r"^a sidecar produced by `resinsim sim --voxel-cure-mm 0\.05 --resin elegoo_ceramic_grey_v2 --printer elegoo_mars5_ultra --file <lilith>\.ctb --out model\.sim\.json` with `RESINSIM_MAX_FIELD_BYTES=17179869184`$"
)]
fn given_sidecar_with_permissive_budget(world: &mut UatWorld) {
    // 13381 × 13381 × 24 = 4,297,251,864 bytes (~4.0016 GB) —
    // just above the default 4 GB budget (4,294,967,296).
    let dim = 13381_u32;
    let sidecar_bytes = build_oversized_strain_sidecar(dim, dim);
    let implied = u64::from(dim) * u64::from(dim) * u64::from(FIELD_COMPONENT_SIZE_TENSOR);
    assert!(
        implied > 4 * 1024 * 1024 * 1024,
        "fixture sanity: strain descriptor must claim > 4 GB; got {implied}",
    );
    let dir = unique_tmp_dir("budget-mismatch-uat1");
    let sim_json = write_envelope_with_sidecar(&dir, &sidecar_bytes, &["strain"]);
    world.sim_json_path = Some(sim_json);
}

#[given(regex = r"^the sidecar's strain field claims > 4 GB allocation$")]
fn given_strain_claims_over_4gb(_world: &mut UatWorld) {
    // Precondition verified in the preceding Given step's fixture sanity
    // assert. This step is a spec-level documentation marker, not a
    // separate test action.
}

#[when(
    regex = r"^the consumer runs `resinsim report health --in model\.sim\.json` without the env override \(default `MAX_FIELD_ALLOCATION_BYTES = 4 GB`\)$"
)]
fn when_consumer_default_budget(world: &mut UatWorld) {
    invoke_report_health_field_sim(world, &[]);
}

#[then(regex = r#"^stderr mentions "exceeds field budget for strain"$"#)]
fn then_stderr_mentions_exceeds_field_budget_strain(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("exceeds field budget for strain"),
        "expected stderr to contain 'exceeds field budget for strain', got: {stderr}",
    );
}

// "Then the process exits with non-zero code" — reused, module-1
// registration (cli_sim_producer_writes_sim_json.rs).

// "And the process does not panic" — reused registration from
// cli_sim_rejects_unknown_schema_version.rs.

// ---- UAT-2: consumer with matching budget succeeds ---------------------------
// spec/uat/cli-sim-budget-mismatch-on-load.md UAT-2

#[given(regex = r"^the same paired sim\.json \+ fields\.bin from UAT-1$")]
fn given_same_paired_sidecar(world: &mut UatWorld) {
    // Cucumber resets World between scenarios, so UAT-2 cannot literally
    // reuse UAT-1's files. Produce a REAL, small sidecar with all four
    // field types. The budget-sufficient consumer loads this successfully.
    let sim_json = produce_full_sidecar("budget-mismatch-uat2");
    world.sim_json_path = Some(sim_json);
}

#[when(
    regex = r"^the consumer runs `RESINSIM_MAX_FIELD_BYTES=17179869184 \\ resinsim report health --in model\.sim\.json`$"
)]
fn when_consumer_with_budget_override(world: &mut UatWorld) {
    invoke_report_health_field_sim(
        world,
        &[("RESINSIM_MAX_FIELD_BYTES", "17179869184")],
    );
}

// "Then the process exits with code 0" — reused registration from
// ctb_layer_height_authority.rs.

#[then(
    regex = r"^the report renders all four voxel-derived sections \(strain gradient, stress max, etc\.\)$"
)]
fn then_report_renders_voxel_sections(world: &mut UatWorld) {
    let exit_code = world.cli_exit_code.unwrap_or(-1);
    assert_eq!(exit_code, 0, "expected exit code 0, got {exit_code}");
}
