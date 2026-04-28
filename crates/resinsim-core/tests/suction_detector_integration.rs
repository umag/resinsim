//! End-to-end integration tests for `SuctionDetector` → `CavityDetector` via
//! `SimulationRunner`. Abstract topology-regression tests; no dependency on
//! specific external fixtures.
//!
//! Per plan v6 Step 1.

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{FailureType, PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22.0 °C is in AmbientTemperature domain")
}

fn solid_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new_all_solid(w, h, voxel).expect("valid all-solid mask")
}

fn void_mask(w: u32, h: u32, voxel: f32) -> LayerMask {
    LayerMask::new(w, h, voxel).expect("valid all-void mask")
}

/// Raft-plus-fluid-permeable-supports topology: a fully-solid raft plate
/// followed by discrete support columns arranged so the inter-column gaps
/// reach the lateral bbox edges, then a solid model body above. This is the
/// canonical false-positive pattern that the area-drop heuristic mis-flagged
/// as a sealed cavity.
///
/// Expected: zero `SuctionCup` critical failures. The inter-column void is
/// exterior-connected (drains to the vat laterally) and must never emit.
fn raft_plus_columns_layer_inputs(
    exposure_sec: f32,
    layer_height_um: f32,
    lift_speed_mm_min: f32,
) -> Vec<LayerInput> {
    let bed = 25u32;
    let voxel = 1.0_f32;

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
    for _ in 0..23 {
        layers.push(
            LayerInput::new(
                idx,
                raft_area,
                exposure_sec,
                lift_speed_mm_min,
                layer_height_um,
                z_mm,
            )
            .expect("valid")
            .with_mask(raft_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    for _ in 0..8 {
        layers.push(
            LayerInput::new(
                idx,
                column_area,
                exposure_sec,
                lift_speed_mm_min,
                layer_height_um,
                z_mm,
            )
            .expect("valid")
            .with_mask(column_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    for _ in 0..10 {
        layers.push(
            LayerInput::new(
                idx,
                model_area,
                exposure_sec,
                lift_speed_mm_min,
                layer_height_um,
                z_mm,
            )
            .expect("valid")
            .with_mask(model_mask.clone()),
        );
        idx += 1;
        z_mm += layer_height_mm;
    }
    layers
}

#[test]
fn raft_plus_fluid_permeable_supports_emits_no_suction_critical() {
    let layers = raft_plus_columns_layer_inputs(2.5, 50.0, 60.0);
    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("validated profiles satisfy run_from_layer_inputs preconditions");

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
        "raft + fluid-permeable supports must emit zero SuctionCup criticals — \
         the false-positive reproduction the fix is targeting. Got: {suction_criticals:#?}"
    );
}

/// Optional end-to-end regression against an external CTB fixture, gated
/// behind the `RESINSIM_EXTERNAL_CTB_FIXTURE` env var. Not part of default
/// CI; documents how to verify the fix against a concrete real-world print.
///
/// ```sh
/// RESINSIM_EXTERNAL_CTB_FIXTURE=/path/to/any.ctb \
///   cargo nextest run --run-ignored=all external_ctb
/// ```
#[test]
#[ignore = "optional — requires RESINSIM_EXTERNAL_CTB_FIXTURE env var"]
fn external_ctb_emits_no_suction_critical() {
    let fixture_path = std::env::var("RESINSIM_EXTERNAL_CTB_FIXTURE")
        .expect("RESINSIM_EXTERNAL_CTB_FIXTURE env var required for this test");
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
        test_ambient(),
        None,
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
        "external CTB must emit zero SuctionCup criticals for a successful real-world print. \
         Got: {suction_criticals:#?}"
    );
}
