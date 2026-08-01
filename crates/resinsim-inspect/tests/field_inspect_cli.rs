//! CLI integration tests for `resinsim inspect field`.
//! t2f6-field-inspector, plan step 8.
//!
//! Follows the established `std::process::Command` +
//! `env!("CARGO_BIN_EXE_resinsim")` harness from `profile_loader_cli.rs`
//! (including its "nextest CWD is the crate root" note) and the
//! in-process fixture-building convention from
//! `resinsim-core/tests/sidecar_roundtrip_integration.rs`. There is no
//! committed CTB fixture (`data/test_cube_10mm.ctb.README.md`), so the
//! synthetic in-process pair is the unconditional path; a true
//! `resinsim sim --voxel-cure-mm` pipeline test stays env-gated
//! elsewhere.
//!
//! `layer_height_provenance` cannot be set through
//! `PrintSimulation`'s public API from an external crate
//! (`set_layer_height_provenance` is `pub(crate)`) — it is normally
//! populated by `SimulationRunner::run_from_layer_inputs_with_voxel`
//! during a real CTB run. To exercise the cure field's Z-address-by-mm
//! path end-to-end without driving a full simulation, this file
//! patches the SERIALISED sim.json (the same technique
//! `print_simulation.rs`'s own `validate_returns_err_when_*` tests use
//! for tampered-fixture construction: `serde_json::to_value` +
//! mutate + `serde_json::from_value`), leaving the sidecar itself
//! (produced by the real `save_with_provenance`) untouched.

#![cfg(feature = "field-sim")]

use std::{
    path::{
        Path,
        PathBuf,
    },
    process::{
        Command,
        Output,
    },
};

use ndarray::Array3;
use resinsim_core::{
    entities::{
        PrinterProfile,
        ResinProfile,
    },
    repositories::{
        Provenance,
        save_with_provenance,
    },
    simulation::PrintSimulation,
    values::{
        CureField,
        PhotoinitiatorField,
        StrainField,
        StrainTensor,
        StressField,
        StressTensor,
        ThermalField,
    },
};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_resinsim")
}

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("field-inspect-cli-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: create tmp dir");
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn resinsim")
}

fn provenance() -> Provenance {
    Provenance {
        input_path: "fixture/synth.ctb".into(),
        resin_name: "Generic Standard".into(),
        printer_name: "Linear Test Printer".into(),
        n_supports: 20,
        tip_radius_mm: 0.2,
    }
}

/// 4×4×3 synthetic `PrintSimulation` with all five voxel fields
/// populated deterministically, matching
/// `sidecar_roundtrip_integration.rs`'s convention.
fn build_simulation_with_all_fields() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);

    let (nx, ny, nz) = (4, 4, 3);
    let voxel_size_mm = 0.05;
    let bbox_min_mm = [0.0, 0.0, 0.0];

    let cure_data =
        Array3::<f32>::from_shape_fn((nx, ny, nz), |(x, y, z)| (x + y * 4 + z * 16) as f32 * 0.5);
    let cure = CureField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        voxel_size_mm,
        bbox_min_mm,
        cure_data,
    )
    .expect("cure ctor");

    // Sparse pattern -- known zero fraction (half zero) for the
    // --cured-only dual-scope test.
    let pi_data =
        Array3::<f32>::from_shape_fn(
            (nx, ny, nz),
            |(x, y, z)| {
                if (x + y + z) % 2 == 0 { 0.0 } else { 0.7 }
            },
        );
    let photoinit =
        PhotoinitiatorField::from_persistence_parts(nx as u32, ny as u32, nz as u32, 0.8, pi_data)
            .expect("photoinit ctor");
    sim.set_voxel_fields(cure, photoinit)
        .expect("install voxel fields");

    let strain_data = Array3::<StrainTensor>::from_shape_fn((nx, ny, nz), |(x, y, _z)| {
        let e = (x + y) as f32 * 0.001;
        StrainTensor::new(-e, -e, -e * 2.0, 0.0, 0.0, 0.0).expect("tensor ctor")
    });
    let strain = StrainField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        voxel_size_mm,
        bbox_min_mm,
        strain_data,
    )
    .expect("strain ctor");

    let stress_data = Array3::<StressTensor>::from_shape_fn((nx, ny, nz), |(x, y, z)| {
        let s = (x + y + z) as f32 * 0.5;
        StressTensor::new(s, s, s, 0.0, 0.0, 0.0).expect("tensor ctor")
    });
    let stress = StressField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        voxel_size_mm,
        bbox_min_mm,
        stress_data,
    )
    .expect("stress ctor");
    sim.set_strain_stress_fields(strain, stress)
        .expect("install strain+stress");

    let thermal_dims = (5, 4, 6);
    let thermal_data = Array3::<f32>::from_shape_fn(thermal_dims, |(x, y, z)| {
        22.0 + (x + y * 5 + z * 20) as f32 * 0.1
    });
    let thermal = ThermalField::from_persistence_parts(
        thermal_dims.0 as u32,
        thermal_dims.1 as u32,
        thermal_dims.2 as u32,
        0.5,
        [0.0, 0.0, 0.0],
        thermal_data,
    )
    .expect("thermal ctor");
    sim.set_thermal_field(thermal);

    sim
}

/// Save `sim` via the real `save_with_provenance` (correct sidecar +
/// sha256 pointer), then patch the written sim.json to add a
/// `layer_height_provenance` block the aggregate's public API cannot
/// set from outside the crate. Layer heights: [50, 30, 20] µm
/// (non-uniform, matching the domain-level mandatory regression test's
/// fixture) — cumulative boundaries (mm): 0, 0.05, 0.08, 0.10.
fn save_fixture_with_layer_heights(dir: &Path, sim: &PrintSimulation) -> PathBuf {
    let sim_json_path = dir.join("model.sim.json");
    save_with_provenance(&sim_json_path, sim, &provenance()).expect("save fixture");

    let contents = std::fs::read_to_string(&sim_json_path).expect("read written sim.json");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&contents).expect("parse written sim.json");
    envelope["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_layer_heights_um": [50.0, 30.0, 20.0],
        "recipe_um": 33.3,
    });
    std::fs::write(
        &sim_json_path,
        serde_json::to_string_pretty(&envelope).expect("re-serialise patched envelope"),
    )
    .expect("write patched sim.json");
    sim_json_path
}

fn sim_json_no_sidecar(dir: &Path) -> PathBuf {
    // Tier-1: no set_voxel_fields call at all, no sidecar.
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let sim = PrintSimulation::new(recipe, printer);
    let path = dir.join("tier1.sim.json");
    save_with_provenance(&path, &sim, &provenance()).expect("save tier1 fixture");
    path
}

// ---- happy path: text + JSON, all five field kinds ----

#[test]
fn happy_text_xy_slice_by_layer_index_emits_stats_and_histogram() {
    let dir = tmp_dir("happy-text");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Stats"),
        "must render a stats section: {stdout}"
    );
    assert!(
        stdout.contains("Histogram"),
        "must render a histogram section: {stdout}"
    );
    assert!(stdout.contains("count"));
    assert!(stdout.contains("nonzero"));
}

#[test]
fn happy_json_shape_is_stable_and_machine_parseable() {
    let dir = tmp_dir("happy-json");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(value["field"], "cure");
    assert_eq!(value["plane"], "xy");
    assert_eq!(value["units"], "mJ/cm²");
    assert_eq!(value["stats_scope"], "all");
    assert!(value["stats"]["count"].is_u64());
    assert!(value["stats"]["nonzero_count"].is_u64());
    assert!(value["stats"]["min"].is_number());
    assert!(value["stats"]["max"].is_number());
    assert!(value["dims"]["nu"].is_u64());
    assert!(value["dims"]["nv"].is_u64());
    assert!(value["histogram"]["bins"].is_array());
    assert!(value.get("values").is_none(), "--values must be opt-in");
}

#[test]
fn all_five_field_kinds_produce_successful_json_output() {
    let dir = tmp_dir("all-five-kinds");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    for kind in ["cure", "photoinitiator", "strain", "stress", "thermal"] {
        let out = run(&[
            "inspect",
            "field",
            "--in",
            sim_json.to_str().expect("utf8 path"),
            "--field",
            kind,
            "--slice",
            "z=0",
            "--json",
        ]);
        assert!(
            out.status.success(),
            "--field {kind} must succeed; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let value: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("--field {kind}: stdout must be valid JSON: {e}"));
        assert_eq!(value["field"], kind);
        if kind == "stress" {
            assert!(
                value.get("model_gap_caveat").is_some(),
                "stress output must carry the KB-162 model-gap caveat"
            );
        } else {
            assert!(
                value.get("model_gap_caveat").is_none(),
                "only stress output carries the model-gap caveat"
            );
        }
    }
}

// ---- --slice z=<N>mm resolves through cumulative layer heights (cure) vs vat envelope (thermal) ----

#[test]
fn slice_z_mm_resolves_through_cumulative_layer_heights_for_cure() {
    let dir = tmp_dir("z-mm-cure");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    // Layer heights [50, 30, 20] um -> cumulative boundaries (mm):
    // 0, 0.05, 0.08, 0.10. z=0.09mm falls in [0.08, 0.10) -> layer index 2.
    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0.09mm",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        value["index"], 2,
        "z=0.09mm must resolve to layer index 2 via cumulative heights [50,30,20]um; got {value}"
    );
}

#[test]
fn slice_z_mm_resolves_through_voxel_size_for_thermal_not_layer_heights() {
    let dir = tmp_dir("z-mm-thermal");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    // Thermal voxel_size_mm=0.5, bbox_min_z=0.0: z=0.09mm -> floor(0.09/0.5)=0.
    // Same z=0.09mm input as the cure test above resolves to a
    // DIFFERENT index (0, not 2) -- the CLI-level end-to-end proof of
    // the two-Z-semantics split (domain-level mandatory regression
    // test already covers this in services::field_slicer::tests).
    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "thermal",
        "--slice",
        "z=0.09mm",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        value["index"], 0,
        "thermal z=0.09mm must resolve via bbox_min_z + iz*voxel_size_mm (0.5mm), NOT cumulative \
         layer heights; got {value}"
    );
}

// ---- --values dense array round-trip ----

#[test]
fn values_flag_round_trips_element_count() {
    let dir = tmp_dir("values-roundtrip");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
        "--json",
        "--values",
    ]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let values = value["values"].as_array().expect("values must be an array");
    let nu = value["dims"]["nu"].as_u64().expect("nu");
    let nv = value["dims"]["nv"].as_u64().expect("nv");
    assert_eq!(values.len() as u64, nu * nv);
}

// ---- --cured-only dual-scope stats ----

#[test]
fn cured_only_flag_reports_nonzero_scope_with_both_counts() {
    let dir = tmp_dir("cured-only");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    // photoinitiator z=0 slab: (x+y+0)%2==0 -> 0.0 else 0.7, over a
    // 4x4 grid -> exactly half zero, half 0.7 (known fraction).
    let out_all = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8"),
        "--field",
        "photoinitiator",
        "--slice",
        "z=0",
        "--json",
    ]);
    assert!(out_all.status.success());
    let all_value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out_all.stdout)).expect("valid JSON");

    let out_cured = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8"),
        "--field",
        "photoinitiator",
        "--slice",
        "z=0",
        "--json",
        "--cured-only",
    ]);
    assert!(out_cured.status.success());
    let cured_value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out_cured.stdout)).expect("valid JSON");

    assert_eq!(all_value["stats_scope"], "all");
    assert_eq!(cured_value["stats_scope"], "nonzero");
    let total_count = all_value["stats"]["count"].as_u64().expect("count");
    let all_nonzero = all_value["stats"]["nonzero_count"]
        .as_u64()
        .expect("nonzero_count");
    let cured_count = cured_value["stats"]["count"].as_u64().expect("count");
    let cured_nonzero = cured_value["stats"]["nonzero_count"]
        .as_u64()
        .expect("nonzero_count");
    assert_eq!(total_count, 16, "4x4 XY slice has 16 voxels");
    assert_eq!(
        all_nonzero, 8,
        "exactly half the 4x4 checkerboard is nonzero"
    );
    assert_eq!(
        cured_count, 8,
        "nonzero-scope population excludes the zeros"
    );
    assert_eq!(
        cured_nonzero, 8,
        "nonzero_count is always present regardless of scope"
    );
}

// ---- missing sidecar / no voxel fields ----

#[test]
fn tier1_sim_json_with_no_sidecar_produces_actionable_error_not_a_panic() {
    let dir = tmp_dir("tier1-no-sidecar");
    let sim_json = sim_json_no_sidecar(&dir);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
    ]);
    assert!(
        !out.status.success(),
        "Tier-1 sim.json must fail, not silently succeed"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("stack backtrace"),
        "must be a user-facing error, not a Rust panic: {stderr}"
    );
    assert!(
        stderr.contains("no cure voxel field")
            || stderr.contains("no voxel field")
            || stderr.contains("cure"),
        "error must name the missing field: {stderr}"
    );
    assert!(out.stdout.is_empty(), "stdout must stay empty on error");
}

#[test]
fn tier1_sim_json_error_is_identical_under_json_flag() {
    let dir = tmp_dir("tier1-no-sidecar-json");
    let sim_json = sim_json_no_sidecar(&dir);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
        "--json",
    ]);
    assert!(!out.status.success());
    assert!(
        out.stdout.is_empty(),
        "--json error path must still leave stdout empty, not emit a JSON error envelope"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "stderr must carry the prose error");
}

// ---- out-of-range slice index ----

#[test]
fn out_of_range_slice_index_exits_2_naming_valid_range() {
    let dir = tmp_dir("out-of-range");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    // Cure field is 4x4x3; z=10mm is far past the 0.10mm layer-stack top.
    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=10mm",
    ]);
    assert_eq!(out.status.code(), Some(2), "out-of-range slice must exit 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("outside") || stderr.contains("range"),
        "stderr must name the valid range: {stderr}"
    );
}

// ---- unknown --field rejected by clap ----

#[test]
fn unknown_field_value_rejected_by_clap() {
    let dir = tmp_dir("unknown-field");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "not-a-real-field",
        "--slice",
        "z=0",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not-a-real-field") || stderr.contains("invalid value"),
        "clap must reject the unknown --field value: {stderr}"
    );
}

// ---- output-shape pinning goldens (RESINSIM_REGENERATE_FIELD_GOLDEN escape hatch) ----
//
// Follows resinsim-inspect/tests/sim_golden.rs's convention. The `file`
// field/header line embeds the fixture's absolute path (nondeterministic
// across machines/checkouts), so it is redacted to a stable placeholder
// before comparison — the ONLY normalisation applied; every other byte
// is compared exactly.

const REGENERATE_ENV: &str = "RESINSIM_REGENERATE_FIELD_GOLDEN";

fn golden_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("field_inspect")
}

fn redact_path(output: &str, path: &Path) -> String {
    output.replace(&path.display().to_string(), "<SIM_JSON_PATH>")
}

fn assert_or_regenerate_golden(label: &str, actual: &str) {
    let golden_path = golden_fixtures_dir().join(label);
    if std::env::var(REGENERATE_ENV).is_ok() {
        std::fs::create_dir_all(golden_fixtures_dir()).expect("mkdir fixtures");
        std::fs::write(&golden_path, actual).expect("write golden");
        eprintln!(
            "regenerated {} ({} bytes)",
            golden_path.display(),
            actual.len()
        );
        return;
    }
    let expected = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
        panic!(
            "missing golden fixture {} ({e}). Regenerate with `{REGENERATE_ENV}=1 cargo nextest \
             run --no-capture field_inspect_cli`.",
            golden_path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "byte drift between produced output and golden {}. If intentional, regenerate via \
         `{REGENERATE_ENV}=1 cargo nextest run --no-capture field_inspect_cli`.",
        golden_path.display()
    );
}

#[test]
fn text_render_matches_golden() {
    let dir = tmp_dir("golden-text");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
        "--bins",
        "5",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = redact_path(&String::from_utf8_lossy(&out.stdout), &sim_json);
    assert_or_regenerate_golden("cure_xy_z0.text.golden", &stdout);
}

#[test]
fn json_render_matches_golden() {
    let dir = tmp_dir("golden-json");
    let sim = build_simulation_with_all_fields();
    let sim_json = save_fixture_with_layer_heights(&dir, &sim);

    let out = run(&[
        "inspect",
        "field",
        "--in",
        sim_json.to_str().expect("utf8 path"),
        "--field",
        "cure",
        "--slice",
        "z=0",
        "--bins",
        "5",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = redact_path(&String::from_utf8_lossy(&out.stdout), &sim_json);
    assert_or_regenerate_golden("cure_xy_z0.json.golden", &stdout);
    // Pin stats_scope explicitly (review-ux binding condition): the
    // JSON schema addition must not silently drop out of the golden.
    assert!(
        stdout.contains("\"stats_scope\": \"all\""),
        "golden must pin stats_scope"
    );
}

// ---- feature-off surface (this file only compiles under field-sim;
// the counterpart default-build test lives in
// field_inspect_feature_off_cli.rs) ----
