//! ADR-0019 / t2f3.5 — end-to-end sidecar roundtrip integration test.
//!
//! Builds a small synthetic `PrintSimulation` with populated voxel
//! fields, saves via `save_with_provenance` (which writes a paired
//! `<stem>.sim.json` + `<stem>.fields.bin`), reloads via
//! `load_from_path`, and asserts the reloaded aggregate's voxel
//! fields are byte-identical to the source.
//!
//! Uses tiny dimensions (4×4×3) to keep test runtime + fixtures small.

#![cfg(feature = "field-sim")]

use ndarray::Array3;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::repositories::{load_from_path, save_with_provenance, Provenance};
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{
    CureField, PhotoinitiatorField, StrainField, StrainTensor, StressField, StressTensor,
};
use std::path::{Path, PathBuf};

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("sidecar-roundtrip-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: must create dir");
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

/// Build a tiny PrintSimulation with all four voxel fields populated
/// with deterministic but non-uniform values. Used to verify byte-
/// identity roundtrip.
fn build_simulation_with_voxel_fields() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);

    let (nx, ny, nz) = (4, 4, 3);
    let voxel_size_mm = 0.05;
    let bbox_min_mm = [1.0, 2.0, 3.0];

    // CureField with a deterministic gradient.
    let cure_data =
        Array3::<f32>::from_shape_fn((nx, ny, nz), |(x, y, z)| (x + y * 4 + z * 16) as f32 * 0.01);
    let cure = CureField::from_persistence_parts(
        nx as u32,
        ny as u32,
        nz as u32,
        voxel_size_mm,
        bbox_min_mm,
        cure_data,
    )
    .expect("cure ctor");

    // PhotoinitiatorField — same shape, different scaling.
    let pi_data =
        Array3::<f32>::from_shape_fn((nx, ny, nz), |(x, y, z)| 0.5 + (x + y + z) as f32 * 0.001);
    let photoinit = PhotoinitiatorField::from_persistence_parts(
        nx as u32, ny as u32, nz as u32, 0.8, // initial_concentration upper bound
        pi_data,
    )
    .expect("photoinit ctor");

    sim.set_voxel_fields(cure, photoinit)
        .expect("install voxel fields");

    // StrainField — populated with a small per-voxel gradient.
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

    sim
}

#[test]
fn save_then_load_preserves_cure_field_bytes() {
    let dir = tmp_dir("cure");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxel_fields();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let bin_path = dir.join("model.fields.bin");
    assert!(bin_path.is_file(), "sidecar must be written");
    let loaded = load_from_path(&path).expect("load");
    let original_cure = sim.cure_field().expect("source has cure");
    let loaded_cure = loaded.cure_field().expect("loaded has cure");
    assert_eq!(loaded_cure.dimensions(), original_cure.dimensions());
    assert_eq!(loaded_cure.voxel_size_mm(), original_cure.voxel_size_mm());
    assert_eq!(loaded_cure.bbox_min_mm(), original_cure.bbox_min_mm());
    let (nx, ny, nz) = loaded_cure.dimensions();
    for iz in 0..nz {
        for iy in 0..ny {
            for ix in 0..nx {
                let a = original_cure.dose_at(ix, iy, iz).expect("a");
                let b = loaded_cure.dose_at(ix, iy, iz).expect("b");
                assert_eq!(a.to_bits(), b.to_bits(), "cure ({ix},{iy},{iz})");
            }
        }
    }
}

#[test]
fn save_then_load_preserves_photoinit_field_bytes() {
    let dir = tmp_dir("photoinit");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxel_fields();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let loaded = load_from_path(&path).expect("load");
    let original = sim.photoinitiator_field().expect("source has pi");
    let l = loaded.photoinitiator_field().expect("loaded has pi");
    let (nx, ny, nz) = l.dimensions();
    assert_eq!(l.dimensions(), original.dimensions());
    for iz in 0..nz {
        for iy in 0..ny {
            for ix in 0..nx {
                let a = original.concentration_at(ix, iy, iz).expect("a");
                let b = l.concentration_at(ix, iy, iz).expect("b");
                assert_eq!(a.to_bits(), b.to_bits(), "pi ({ix},{iy},{iz})");
            }
        }
    }
}

#[test]
fn save_then_load_preserves_strain_field_bytes() {
    let dir = tmp_dir("strain");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxel_fields();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let loaded = load_from_path(&path).expect("load");
    let original = sim.strain_field().expect("source has strain");
    let l = loaded.strain_field().expect("loaded has strain");
    let (nx, ny, nz) = l.dimensions();
    assert_eq!(l.dimensions(), original.dimensions());
    for iz in 0..nz {
        for iy in 0..ny {
            for ix in 0..nx {
                let a = original.strain_at(ix, iy, iz).expect("a");
                let b = l.strain_at(ix, iy, iz).expect("b");
                assert_eq!(
                    a.components().map(|f| f.to_bits()),
                    b.components().map(|f| f.to_bits()),
                    "strain ({ix},{iy},{iz})"
                );
            }
        }
    }
}

#[test]
fn save_then_load_preserves_stress_field_bytes() {
    let dir = tmp_dir("stress");
    let path = dir.join("model.sim.json");
    let sim = build_simulation_with_voxel_fields();
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let loaded = load_from_path(&path).expect("load");
    let original = sim.stress_field().expect("source has stress");
    let l = loaded.stress_field().expect("loaded has stress");
    let (nx, ny, nz) = l.dimensions();
    for iz in 0..nz {
        for iy in 0..ny {
            for ix in 0..nx {
                let a = original.stress_at(ix, iy, iz).expect("a");
                let b = l.stress_at(ix, iy, iz).expect("b");
                assert_eq!(
                    a.components().map(|f| f.to_bits()),
                    b.components().map(|f| f.to_bits()),
                    "stress ({ix},{iy},{iz})"
                );
            }
        }
    }
}

#[test]
fn tier1_simulation_has_no_sidecar_pointer() {
    // A simulation without voxel fields should round-trip without
    // a sidecar — the bin file must NOT exist post-save and the
    // envelope's fields_sidecar must serialise as absent.
    let dir = tmp_dir("tier1");
    let path = dir.join("tier1.sim.json");
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let sim = PrintSimulation::new(recipe, printer);
    save_with_provenance(&path, &sim, &provenance()).expect("save");
    let bin_path = dir.join("tier1.fields.bin");
    assert!(
        !bin_path.exists(),
        "Tier-1 simulation must not write a sidecar"
    );
    let json = std::fs::read_to_string(&path).expect("read sim.json");
    assert!(
        !json.contains("fields_sidecar"),
        "Tier-1 envelope must not embed fields_sidecar pointer"
    );
    // Verify load also works.
    let _ = load_from_path(&path).expect("load tier-1 envelope");
}
