//! End-to-end integration tests for `SuctionDetector` → `CavityDetector` via
//! `SimulationRunner`. Reproduces the triage empirical evidence for the
//! suction-detector-raft-false-positive lifecycle.
//!
//! Per plan v6 Step 1; bodies filled in at Phase B (Step 7).

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{FailureType, PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::LayerMask;

fn solid_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new_all_solid(w, h, voxel).expect("valid all-solid mask")
}

fn void_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new(w, h, voxel).expect("valid all-void mask")
}

/// The lilith-torso repro topology expressed as a self-contained LayerMask
/// stack (CI-default — no external fixture needed). Pattern:
/// - Layer 0-22: fully-solid raft plate (25×25 at 1mm voxel = 625 mm²).
/// - Layer 23-30: 4 discrete 3×3 support columns with inter-column gaps
///   touching the bbox lateral edges → should NOT be flagged as suction.
/// - Layer 31-40: tapering model body above supports (solid block).
fn lilith_torso_synthetic_layers(
    exposure_sec: f32,
    layer_height_um: f32,
    lift_speed_mm_min: f32,
) -> Vec<LayerInput> {
    let bed = 25u32;
    let voxel = 1.0_f32;

    // Four support columns at (4..7), (4..7) / (4..7), (17..20) / (17..20), (4..7) / (17..20), (17..20)
    let mut column_mask = void_mask(bed, bed, voxel);
    for (cx, cy) in [(4, 4), (4, 17), (17, 4), (17, 17)] {
        for dx in 0..3 {
            for dy in 0..3 {
                column_mask.set(cx + dx, cy + dy).expect("in bounds");
            }
        }
    }
    let column_area = column_mask.solid_area_mm2();
    let raft_mask = solid_mask(bed, bed, voxel);
    let raft_area = raft_mask.solid_area_mm2();
    let model_mask = solid_mask(bed, bed, voxel);
    let model_area = model_mask.solid_area_mm2();

    let mut layers: Vec<LayerInput> = Vec::new();
    let mut idx: u32 = 0;
    let layer_height_mm = layer_height_um / 1000.0;
    let mut z_mm = 0.0_f32;
    // Raft
    for _ in 0..23 {
        layers.push(
            LayerInput::new(idx, raft_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                .expect("valid")
                .with_mask(raft_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    // Columns
    for _ in 0..8 {
        layers.push(
            LayerInput::new(idx, column_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                .expect("valid")
                .with_mask(column_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    // Model body
    for _ in 0..10 {
        layers.push(
            LayerInput::new(idx, model_area, exposure_sec, lift_speed_mm_min, layer_height_um, z_mm)
                .expect("valid")
                .with_mask(model_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    layers
}

#[test]
fn lilith_torso_synthetic_no_suction_critical() {
    let layers = lilith_torso_synthetic_layers(2.5, 50.0, 60.0);
    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        },
        &PlateAdhesionProfile::default_textured(),
        22.0,
    )
    .expect("valid profiles + bounded inputs satisfy run_from_layer_inputs preconditions");

    let suction_criticals: Vec<_> = sim
        .failures()
        .iter()
        .filter(|f| {
            f.failure_type == FailureType::SuctionCup
                && f.severity == resinsim_core::entities::Severity::Critical
        })
        .collect();

    assert!(
        suction_criticals.is_empty(),
        "lilith-torso synthetic topology must emit zero SuctionCup criticals — \
         the fix's load-bearing empirical claim. Got: {suction_criticals:#?}"
    );
}

/// Optional end-to-end test against the real lilith-torso.ctb fixture, gated
/// behind the `RESINSIM_REAL_CTB_FIXTURE` env var. Not part of default CI;
/// documents how to reproduce the original triage evidence against the real
/// RLE-decoded mask path.
///
/// Example:
/// ```sh
/// RESINSIM_REAL_CTB_FIXTURE=/Users/mag1/Documents/3d/lilith-torso.ctb \
///   cargo nextest run --run-ignored=all lilith_torso_real
/// ```
#[test]
#[ignore = "optional — requires RESINSIM_REAL_CTB_FIXTURE env var + real CTB fixture"]
fn lilith_torso_real_ctb_no_suction_critical() {
    let fixture_path = std::env::var("RESINSIM_REAL_CTB_FIXTURE")
        .expect("RESINSIM_REAL_CTB_FIXTURE env var required for this test");
    let (_info, layers) = resinsim_core::io::ctb::parse_ctb(std::path::Path::new(&fixture_path))
        .expect("fixture parses");

    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::elegoo_mars5_ultra(),
        &SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        },
        &PlateAdhesionProfile::default_textured(),
        22.0,
    )
    .expect("fixture + profiles run successfully");

    let suction_criticals: Vec<_> = sim
        .failures()
        .iter()
        .filter(|f| {
            f.failure_type == FailureType::SuctionCup
                && f.severity == resinsim_core::entities::Severity::Critical
        })
        .collect();

    assert!(
        suction_criticals.is_empty(),
        "real lilith-torso.ctb must emit zero SuctionCup criticals — reproduces \
         the triage fix. Got: {suction_criticals:#?}"
    );
}
