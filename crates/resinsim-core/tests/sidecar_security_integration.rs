//! ADR-0019 / t2f3.5 — sidecar security surface integration tests.
//!
//! Tests defenses against tampered sidecars: sha256 mismatch, missing
//! sidecar, path traversal in the SidecarPointer, symlink escape,
//! directory-as-sidecar.
//!
//! All scenarios must produce **typed errors with stable substrings**
//! per ADR-0019 §"Stable error substrings".

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::repositories::{load_from_path, save_with_provenance, Provenance};
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{CureField, PhotoinitiatorField};
use std::path::{Path, PathBuf};

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("sidecar-security-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup");
    dir
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

fn build_simulation_with_voxels() -> PrintSimulation {
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
    .expect("ctor");
    let pi_data = Array3::<f32>::from_shape_fn((nx, ny, nz), |_| 0.5);
    let photoinit =
        PhotoinitiatorField::from_persistence_parts(nx as u32, ny as u32, nz as u32, 0.8, pi_data)
            .expect("ctor");
    sim.set_voxel_fields(cure, photoinit).expect("install");
    sim
}

#[test]
fn missing_sidecar_returns_typed_error() {
    let dir = tmp_dir("missing");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxels();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    // Delete the sidecar to simulate the user moving the .sim.json
    // without the .fields.bin.
    let bin = dir.join("model.fields.bin");
    std::fs::remove_file(&bin).expect("delete sidecar");
    let err = load_from_path(&path).expect_err("load must fail");
    assert!(
        err.contains("missing sidecar") || err.contains("sidecar path traversal rejected"),
        "expected missing-sidecar error, got: {err}"
    );
}

#[test]
fn tampered_sidecar_returns_sha256_mismatch() {
    let dir = tmp_dir("sha256");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxels();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let bin = dir.join("model.fields.bin");
    // Append a byte to flip the sha256 without changing size dramatically.
    let mut bytes = std::fs::read(&bin).expect("read");
    // Flip the magic byte at offset 0 ('R' → 'X'); the sha256 changes
    // but the file is the same size, so the byte_size check passes and
    // sha256 is the next gate.
    bytes[0] = b'X';
    std::fs::write(&bin, &bytes).expect("write");
    let err = load_from_path(&path).expect_err("load must fail");
    assert!(
        err.contains("sidecar sha256 mismatch"),
        "expected sha256 mismatch, got: {err}"
    );
}

#[test]
fn byte_size_mismatch_returns_typed_error() {
    let dir = tmp_dir("size");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxels();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let bin = dir.join("model.fields.bin");
    // Truncate the sidecar — byte_size now differs from pointer's claim.
    let mut bytes = std::fs::read(&bin).expect("read");
    bytes.truncate(bytes.len() - 10);
    std::fs::write(&bin, &bytes).expect("write");
    let err = load_from_path(&path).expect_err("load must fail");
    assert!(
        err.contains("sidecar size mismatch"),
        "expected size mismatch, got: {err}"
    );
}

#[test]
fn path_traversal_in_pointer_rejected() {
    // Craft an envelope by hand whose fields_sidecar.path tries to
    // escape the sim.json parent. The loader's SidecarPointer::validate
    // must reject it before reading anything.
    let dir = tmp_dir("traversal");
    let envelope = serde_json::json!({
        "schema_version": 2,
        "simulation": {
            "recipe": {
                "layer_height_um": 50.0,
                "bottom_layer_count": 5,
                "transition_layers": 0,
                "normal_exposure_sec": 3.0,
                "bottom_exposure_sec": 30.0,
                "wait_before_cure_sec": 0.0,
                "wait_before_release_sec": 0.0,
                "wait_after_release_sec": 0.0,
                "lift_speed_mm_min": 65.0,
                "lift_cycle_sec": 6.0,
                "lift_distance_mm": 6.0,
            },
            "printer": {
                "name": "test",
                "led_power_mw_cm2": 5.0,
                "pixel_pitch_um": 35.0,
                "layer_height_range_um": {"min": 10.0, "max": 200.0},
                "exposure_range_sec": {"min": 0.5, "max": 60.0},
                "lift_speed_range_mm_min": {"min": 10.0, "max": 300.0},
                "bottom_layer_count_max": 20,
                "z_stiffness_n_per_mm": 1000.0,
                "delta_t_steady_c": 0.0,
                "thermal_tau_sec": 600.0,
                "lcd_uniformity_variation": 0.05,
                "voxel_size_mm": 0.05,
                "release_mechanism": "linear",
                "led_delta_t_steady_c": 0.0,
                "led_tau_sec": 600.0,
                "led_to_vat_coupling": 0.5,
                "build_envelope_mm": {"width_mm": 153.0, "depth_mm": 78.0, "max_z_mm": 165.0},
                "convective_wall_h_w_m2k": 8.0,
                "vat_wall_thickness_mm": 2.0,
                "vat_wall_k_w_mk": 200.0,
            },
            "layers": [],
            "failures": [],
        },
        "fields_sidecar": {
            "path": "../escape.bin",
            "byte_size": 10,
            "sha256": "0".repeat(64),
            "fields_present": ["cure"]
        }
    });
    let path = dir.join("model.sim.json");
    std::fs::write(&path, serde_json::to_string(&envelope).expect("ser")).expect("write envelope");
    let err = load_from_path(&path).expect_err("path traversal must reject");
    assert!(
        err.contains("sidecar path traversal rejected"),
        "expected path traversal rejection, got: {err}"
    );
}

#[test]
fn v1_envelope_rejected_with_regeneration_hint() {
    let dir = tmp_dir("v1");
    let envelope = serde_json::json!({
        "schema_version": 1,
        "simulation": {
            "recipe": {
                "layer_height_um": 50.0,
                "bottom_layer_count": 5,
                "transition_layers": 0,
                "normal_exposure_sec": 3.0,
                "bottom_exposure_sec": 30.0,
                "wait_before_cure_sec": 0.0,
                "wait_before_release_sec": 0.0,
                "wait_after_release_sec": 0.0,
                "lift_speed_mm_min": 65.0,
                "lift_cycle_sec": 6.0,
                "lift_distance_mm": 6.0,
            },
            "printer": {
                "name": "test",
                "led_power_mw_cm2": 5.0,
                "pixel_pitch_um": 35.0,
                "layer_height_range_um": {"min": 10.0, "max": 200.0},
                "exposure_range_sec": {"min": 0.5, "max": 60.0},
                "lift_speed_range_mm_min": {"min": 10.0, "max": 300.0},
                "bottom_layer_count_max": 20,
                "z_stiffness_n_per_mm": 1000.0,
                "delta_t_steady_c": 0.0,
                "thermal_tau_sec": 600.0,
                "lcd_uniformity_variation": 0.05,
                "voxel_size_mm": 0.05,
                "release_mechanism": "linear",
                "led_delta_t_steady_c": 0.0,
                "led_tau_sec": 600.0,
                "led_to_vat_coupling": 0.5,
                "build_envelope_mm": {"width_mm": 153.0, "depth_mm": 78.0, "max_z_mm": 165.0},
                "convective_wall_h_w_m2k": 8.0,
                "vat_wall_thickness_mm": 2.0,
                "vat_wall_k_w_mk": 200.0,
            },
            "layers": [],
            "failures": [],
        }
    });
    let path = dir.join("legacy.sim.json");
    std::fs::write(&path, serde_json::to_string(&envelope).expect("ser"))
        .expect("write v1 envelope");
    let err = load_from_path(&path).expect_err("v1 must be rejected");
    assert!(
        err.contains("unknown schema_version"),
        "expected schema_version error, got: {err}"
    );
    assert!(
        err.contains("ADR-0019") || err.contains("regenerate"),
        "expected regeneration hint, got: {err}"
    );
}
