//! Step definitions for `spec/uat/cli-sim-rejects-tampered-sidecar.md`
//! UAT-1..UAT-4 (uat-unskip-cli-sim-rejects-tampered-sidecar).
//!
//! SYMBOL VERIFICATION. Every scenario's consumer path drives `resinsim
//! report health --in <PATH>` against the FIELD-SIM binary
//! (`invoke_resinsim_field_sim`). `load_and_install_sidecar_with_budget`
//! (simulation_repo.rs:718) is `#[cfg(feature = "field-sim")]` (called
//! from the gated block at simulation_repo.rs:678-680). The producer
//! path `encode_paired_sidecar` (simulation_repo.rs:428) is also gated.
//! Under default features this module does not compile (gated at the
//! `pub mod` line in `uat_steps/mod.rs`), so all 4 scenarios skip.
//!
//! FIXTURE APPROACH. The paired sim.json + sidecar is produced
//! IN-PROCESS via `save_with_provenance` (same approach as
//! `sidecar_security_integration.rs`) rather than via CLI `resinsim sim
//! --voxel-cure-mm`, because `--stl` input goes through the STL slicer
//! which produces `CrossSectionArea` values but NOT per-layer masks —
//! the voxel cure path requires masks (from CTB files), so a `--stl`
//! run produces no voxel fields and no sidecar. The consumer side
//! (`report health --in`) still exercises the REAL CLI binary end-to-end
//! via `invoke_resinsim_field_sim`.
//!
//! The four stable sidecar-error substrings this module discriminates:
//!
//! | branch    | source                                       | literal                             |
//! |-----------|----------------------------------------------|-------------------------------------|
//! | sha256    | simulation_repo.rs:751                       | `"sidecar sha256 mismatch"`         |
//! | size      | simulation_repo.rs:734                       | `"sidecar size mismatch"`           |
//! | traversal | simulation_repo.rs:172,177,183,193,199,211,… | `"sidecar path traversal rejected"` |
//! | missing   | simulation_repo.rs:731,744                   | `"missing sidecar"`                 |
//!
//! Discrimination shape follows `cli_sim_rejects_unknown_schema_version.rs`.
//!
//! CROSS-SPEC LEAKAGE (checked, safe). UAT-1's Given uses backtick-
//! delimited `` `--voxel-cure-mm` `` — distinct from
//! `sim-fields-sidecar-roundtrip` UAT-2's undelimited
//! `--voxel-cure-mm`. UAT-2/UAT-4's `a paired sim.json + fields.bin`
//! has no backticks — distinct from `sim-fields-sidecar-roundtrip`'s
//! backtick-delimited form by delimiter character.
//!
//! UAT-1/2/4's shared When `invokes \`resinsim report health --in
//! model.sim.json\`` is distinct from
//! `cli_sim_rejects_unknown_schema_version.rs`'s
//! `the user invokes \`resinsim report health --in <PATH>\`` by the
//! "the user" prefix and `<PATH>` vs literal filename.
//!
//! UAT-3's When was corrected from "invokes" to "runs" to avoid
//! colliding with the shared registration above — the default-features
//! binary silently ignores `fields_sidecar` pointers (the consumer
//! `#[cfg]` block is compiled out), so the shared registration cannot
//! serve this scenario.
//!
//! `^the process exits with non-zero code$` is reused from
//! `cli_sim_producer_writes_sim_json.rs`. `^the process does not panic
//! \(no "thread 'main' panicked" in stderr\)$` is reused from
//! `cli_sim_rejects_unknown_schema_version.rs`.

use cucumber::{given, then, when};
use ndarray::Array3;

use super::cli_fixtures::invoke_resinsim_field_sim;
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

// ---- Discrimination framework -----------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidecarRejectBranch {
    Sha256Mismatch,
    SizeMismatch,
    PathTraversal,
    Missing,
}

impl SidecarRejectBranch {
    const ALL: [SidecarRejectBranch; 4] = [
        SidecarRejectBranch::Sha256Mismatch,
        SidecarRejectBranch::SizeMismatch,
        SidecarRejectBranch::PathTraversal,
        SidecarRejectBranch::Missing,
    ];

    fn needle(self) -> &'static str {
        match self {
            SidecarRejectBranch::Sha256Mismatch => "sidecar sha256 mismatch",
            SidecarRejectBranch::SizeMismatch => "sidecar size mismatch",
            SidecarRejectBranch::PathTraversal => "sidecar path traversal rejected",
            SidecarRejectBranch::Missing => "missing sidecar",
        }
    }
}

fn assert_only_sidecar_branch(world: &UatWorld, expected: SidecarRejectBranch) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains(expected.needle()),
        "expected stderr to contain {:?} (the {expected:?} branch), got: {stderr}",
        expected.needle(),
    );
    for branch in SidecarRejectBranch::ALL {
        if branch != expected {
            assert!(
                !stderr.contains(branch.needle()),
                "expected stderr to NOT contain {:?} (the {branch:?} branch — only \
                 {expected:?} may have fired), got: {stderr}",
                branch.needle(),
            );
        }
    }
}

fn assert_missing_or_traversal(world: &UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("missing sidecar") || stderr.contains("sidecar path traversal rejected"),
        "expected stderr to contain 'missing sidecar' or 'sidecar path traversal rejected', \
         got: {stderr}",
    );
    assert!(
        !stderr.contains("sidecar sha256 mismatch"),
        "sha256-mismatch branch must NOT fire on a missing sidecar: {stderr}",
    );
    assert!(
        !stderr.contains("sidecar size mismatch"),
        "size-mismatch branch must NOT fire on a missing sidecar: {stderr}",
    );
}

// ---- Fixture helpers --------------------------------------------------------

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

/// Produce a real paired `sim.json` + `fields.bin` via in-process
/// `save_with_provenance`. Same approach as
/// `sidecar_security_integration.rs::build_simulation_with_voxels`.
fn produce_paired_sim_and_sidecar(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use resinsim_core::entities::{PrinterProfile, ResinProfile};
    use resinsim_core::simulation::PrintSimulation;
    use resinsim_core::values::{CureField, PhotoinitiatorField};

    let dir = unique_tmp_dir(tag);
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    let (nx, ny, nz) = (3, 3, 2);
    let cure_data =
        Array3::<f32>::from_shape_fn((nx, ny, nz), |(x, y, z)| (x + y + z) as f32 * 0.1);
    let cure = CureField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        0.05,
        [0.0, 0.0, 0.0],
        cure_data,
    )
    .expect("CureField ctor");
    let pi_data = Array3::<f32>::from_shape_fn((nx, ny, nz), |_| 0.5);
    let photoinit =
        PhotoinitiatorField::from_persistence_parts(nx as u32, ny as u32, nz as u32, 0.8, pi_data)
            .expect("PhotoinitiatorField ctor");
    sim.set_voxel_fields(cure, photoinit).expect("install voxel fields");

    let sim_json = dir.join("model.sim.json");
    resinsim_core::repositories::save_with_provenance(&sim_json, &sim, &provenance())
        .expect("save_with_provenance");
    let fields_bin = dir.join("model.fields.bin");
    assert!(
        fields_bin.is_file(),
        "fixture: sidecar must exist after save_with_provenance at {}",
        fields_bin.display(),
    );
    (sim_json, fields_bin)
}

fn invoke_report_health_field_sim(world: &mut UatWorld) {
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

// ---- UAT-1: sha256-tampered sidecar -----------------------------------------
// spec/uat/cli-sim-rejects-tampered-sidecar.md UAT-1

#[given(
    regex = r"^a paired `model\.sim\.json` \+ `model\.fields\.bin` produced by a `--voxel-cure-mm` run$"
)]
fn given_paired_sim_and_sidecar_uat1(world: &mut UatWorld) {
    let (sim_json, fields_bin) = produce_paired_sim_and_sidecar("tampered-uat1");
    world.sim_json_path = Some(sim_json);
    world.cli_tmp_dir = Some(fields_bin);
}

#[when(regex = r"^the user flips a single byte in `model\.fields\.bin` outside of size$")]
fn when_flip_byte_in_sidecar(world: &mut UatWorld) {
    let bin_path = world
        .cli_tmp_dir
        .as_ref()
        .expect("Given populated fields.bin path");
    let mut bytes = std::fs::read(bin_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", bin_path.display()));
    bytes[0] ^= 0xFF;
    std::fs::write(bin_path, &bytes)
        .unwrap_or_else(|e| panic!("write {}: {e}", bin_path.display()));
}

// UAT-1/2/4 shared When: "And invokes `resinsim report health --in
// model.sim.json`". Distinct from
// cli_sim_rejects_unknown_schema_version.rs:242's `the user invokes
// ...` by the "the user" prefix.
#[when(regex = r"^invokes `resinsim report health --in model\.sim\.json`$")]
fn when_invoke_report_health_field_sim(world: &mut UatWorld) {
    invoke_report_health_field_sim(world);
}

// "Then the process exits with non-zero code" — reused, module-1
// registration (cli_sim_producer_writes_sim_json.rs).

// spec/uat/cli-sim-rejects-tampered-sidecar.md UAT-1 Then:
#[then(regex = r#"^stderr mentions "sidecar sha256 mismatch"$"#)]
fn then_stderr_mentions_sha256_mismatch(world: &mut UatWorld) {
    assert_only_sidecar_branch(world, SidecarRejectBranch::Sha256Mismatch);
}

// "And the process does not panic (no "thread 'main' panicked" in
// stderr)" — reused registration from
// cli_sim_rejects_unknown_schema_version.rs:290.

// ---- UAT-2: truncated sidecar (size mismatch) -------------------------------
// spec/uat/cli-sim-rejects-tampered-sidecar.md UAT-2

#[given(regex = r"^a paired sim\.json \+ fields\.bin$")]
fn given_paired_sim_and_sidecar_short(world: &mut UatWorld) {
    let (sim_json, fields_bin) = produce_paired_sim_and_sidecar("tampered-uat2");
    world.sim_json_path = Some(sim_json);
    world.cli_tmp_dir = Some(fields_bin);
}

#[when(regex = r"^the user truncates `model\.fields\.bin` by 10 bytes$")]
fn when_truncate_sidecar(world: &mut UatWorld) {
    let bin_path = world
        .cli_tmp_dir
        .as_ref()
        .expect("Given populated fields.bin path");
    let bytes = std::fs::read(bin_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", bin_path.display()));
    assert!(
        bytes.len() > 10,
        "sidecar must be >10 bytes to truncate by 10, got {}",
        bytes.len(),
    );
    std::fs::write(bin_path, &bytes[..bytes.len() - 10])
        .unwrap_or_else(|e| panic!("write {}: {e}", bin_path.display()));
}

// "And invokes `resinsim report health --in model.sim.json`" — shared
// When registration `when_invoke_report_health_field_sim` above.

#[then(regex = r#"^stderr mentions "sidecar size mismatch"$"#)]
fn then_stderr_mentions_size_mismatch(world: &mut UatWorld) {
    assert_only_sidecar_branch(world, SidecarRejectBranch::SizeMismatch);
}

// ---- UAT-3: path-traversal sidecar pointer ----------------------------------
// spec/uat/cli-sim-rejects-tampered-sidecar.md UAT-3

#[given(
    regex = r#"^a sim\.json envelope crafted with `fields_sidecar\.path = "\.\./escape\.bin"`$"#
)]
fn given_crafted_traversal_envelope(world: &mut UatWorld) {
    let (sim_json, _bin) = produce_paired_sim_and_sidecar("tampered-uat3");
    let text = std::fs::read_to_string(&sim_json)
        .unwrap_or_else(|e| panic!("read {}: {e}", sim_json.display()));
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", sim_json.display()));
    value["fields_sidecar"] = serde_json::json!({
        "path": "../escape.bin",
        "byte_size": 10,
        "sha256": "0".repeat(64),
        "fields_present": ["cure"]
    });
    let crafted = serde_json::to_string_pretty(&value).expect("re-serialize");
    std::fs::write(&sim_json, crafted)
        .unwrap_or_else(|e| panic!("write {}: {e}", sim_json.display()));
    world.sim_json_path = Some(sim_json);
}

// UAT-3 "When the user runs ..." — distinct from
// cli_sim_rejects_unknown_schema_version.rs's "invokes" form. The
// default-features binary silently ignores fields_sidecar pointers
// (the consumer #[cfg] block is compiled out), so the shared
// "invokes" registration cannot serve this scenario — it would exit 0.
#[when(regex = r"^the user runs `resinsim report health --in <PATH>`$")]
fn when_user_runs_report_health(world: &mut UatWorld) {
    invoke_report_health_field_sim(world);
}

#[then(regex = r#"^stderr mentions "sidecar path traversal rejected"$"#)]
fn then_stderr_mentions_path_traversal(world: &mut UatWorld) {
    assert_only_sidecar_branch(world, SidecarRejectBranch::PathTraversal);
}

// ---- UAT-4: missing sidecar (deleted) ---------------------------------------
// spec/uat/cli-sim-rejects-tampered-sidecar.md UAT-4
// Reuses the `^a paired sim\.json \+ fields\.bin$` Given from UAT-2.

#[when(regex = r"^the user deletes `model\.fields\.bin`$")]
fn when_delete_sidecar(world: &mut UatWorld) {
    let bin_path = world
        .cli_tmp_dir
        .as_ref()
        .expect("Given populated fields.bin path");
    std::fs::remove_file(bin_path)
        .unwrap_or_else(|e| panic!("remove {}: {e}", bin_path.display()));
}

// "And invokes `resinsim report health --in model.sim.json`" — shared
// When registration `when_invoke_report_health_field_sim` above.

#[then(
    regex = r#"^stderr mentions "missing sidecar" or "sidecar path traversal rejected"$"#
)]
fn then_stderr_mentions_missing_or_traversal(world: &mut UatWorld) {
    assert_missing_or_traversal(world);
}
