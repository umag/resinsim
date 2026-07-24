//! End-to-end integration tests for the Kendall interlayer crack-front
//! knockdown (peel-crack-propagation-tier1) through `SimulationRunner`.
//!
//! Proves the runner threads the REAL per-layer mask perimeter into the crack
//! model (mirroring the ADR-0022 Stage 3 shape-factor threading + the
//! `is_fully_solid` placeholder guard):
//!   - a thin, high-perimeter wall (not fully-solid) records a crack front on
//!     its NORMAL layers and can emit `Delamination`;
//!   - placeholder masks (`run_from_areas`' synthetic 1×1 all-solid, and any
//!     fully-solid mask) apply NO knockdown — `crack_front_fraction` stays
//!     `None`, keeping behaviour byte-identical.
//!
//! Capacity-only: the peel LOAD is never touched, so the force series is
//! unchanged; only the safety factor + `Delamination` move.

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{FailureType, PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea, LayerMask};

const VOXEL_MM: f32 = 0.5;
const BOTTOM_LAYERS: u32 = 6; // PlateAdhesionProfile::default_textured()

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22.0 °C is in AmbientTemperature domain")
}

fn no_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.0,
        n_supports: 0,
    }
}

/// A very thin wall mask: 1 cell wide × `height` cells tall, embedded in a
/// `4 × height` grid so it is NOT fully solid (real per-layer geometry, void
/// margins). At `height = 200` (0.5 mm voxels) this is a 0.5 mm × 100 mm wall:
/// `4√A/P ≈ 0.14`, so the interlayer bond is knocked down well below the peel
/// load.
fn thin_wall_mask(height: u32) -> LayerMask {
    let mut m = LayerMask::new(4, height, VOXEL_MM).expect("valid grid");
    for y in 0..height {
        m.set(0, y).expect("in bounds");
    }
    m
}

/// Build `n` layers of the thin wall. `cross_section_area_mm2` is set to the
/// mask's solid area so the predictor's scalar area is consistent with the mask
/// perimeter feeding the crack ratio.
fn thin_wall_layers(n: u32, height: u32) -> Vec<LayerInput> {
    let mask = thin_wall_mask(height);
    let area = mask.solid_area_mm2();
    let mut layers = Vec::new();
    let mut z = 0.0_f32;
    for idx in 0..n {
        layers.push(
            LayerInput::new(idx, area, 2.5, 60.0, 50.0, z)
                .expect("valid layer input")
                .with_mask(mask.clone()),
        );
        z += 0.05;
    }
    layers
}

/// Build `n` layers of a fully-solid mask (`is_fully_solid` → placeholder →
/// no crack knockdown).
fn solid_layers(n: u32, side: u32) -> Vec<LayerInput> {
    let mask = LayerMask::new_all_solid(side, side, VOXEL_MM).expect("valid solid mask");
    let area = mask.solid_area_mm2();
    let mut layers = Vec::new();
    let mut z = 0.0_f32;
    for idx in 0..n {
        layers.push(
            LayerInput::new(idx, area, 2.5, 60.0, 50.0, z)
                .expect("valid layer input")
                .with_mask(mask.clone()),
        );
        z += 0.05;
    }
    layers
}

fn run(layers: &[LayerInput]) -> resinsim_core::simulation::PrintSimulation {
    SimulationRunner::run_from_layer_inputs(
        layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &no_supports(),
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("validated profiles satisfy run_from_layer_inputs preconditions")
}

#[test]
fn thin_wall_normal_layers_record_a_crack_front() {
    let sim = run(&thin_wall_layers(12, 200));
    let normal_with_crack: Vec<_> = sim
        .layers()
        .iter()
        .filter(|l| l.index >= BOTTOM_LAYERS)
        .filter_map(|l| l.crack_front_fraction)
        .collect();
    assert!(
        !normal_with_crack.is_empty(),
        "thin-wall NORMAL layers must record a crack_front_fraction"
    );
    assert!(
        normal_with_crack.iter().all(|&c| c > 0.5),
        "a 0.5×100 mm wall knocks the bond well down: {normal_with_crack:?}"
    );
    // Per-layer independence: every normal layer shares the identical mask, so
    // the crack front must be IDENTICAL across them — no cross-layer carry /
    // accumulation. A stateful (accumulating) model would drift here.
    let first = normal_with_crack[0];
    assert!(
        normal_with_crack.iter().all(|&c| (c - first).abs() < 1e-6),
        "identical-geometry layers must yield identical cracks (no accumulation): {normal_with_crack:?}"
    );
}

#[test]
fn thin_wall_bottom_layers_never_record_a_crack() {
    let sim = run(&thin_wall_layers(12, 200));
    for l in sim.layers().iter().filter(|l| l.index < BOTTOM_LAYERS) {
        assert_eq!(
            l.crack_front_fraction, None,
            "bottom layer {} must not record a crack (plate adhesion, not interlayer)",
            l.index
        );
    }
}

#[test]
fn thin_wall_emits_delamination_end_to_end() {
    let sim = run(&thin_wall_layers(12, 200));
    let delam: Vec<_> = sim
        .failures()
        .iter()
        .filter(|f| f.failure_type == FailureType::Delamination)
        .collect();
    assert!(
        !delam.is_empty(),
        "a very thin wall's crack-reduced interlayer bond falls below the peel load → Delamination"
    );
}

#[test]
fn fully_solid_masks_apply_no_crack_knockdown() {
    // is_fully_solid placeholder guard: fully-solid masks carry no crack-front
    // signal, so no knockdown and no Delamination — behaviour-preserving.
    let sim = run(&solid_layers(12, 20));
    assert!(
        sim.layers().iter().all(|l| l.crack_front_fraction.is_none()),
        "fully-solid masks must not record any crack front"
    );
    assert!(
        !sim
            .failures()
            .iter()
            .any(|f| f.failure_type == FailureType::Delamination),
        "fully-solid masks must not delaminate"
    );
}

#[test]
fn run_from_areas_never_records_a_crack() {
    // run_from_areas synthesises fully-solid 1×1 masks → placeholder → no crack.
    let areas: Vec<CrossSectionArea> = [2500.0, 2500.0, 2500.0, 2500.0, 2500.0, 2500.0, 2500.0, 2500.0]
        .iter()
        .map(|&a| CrossSectionArea::new(a).expect("valid area"))
        .collect();
    let sim = SimulationRunner::run_from_areas(
        &areas,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &no_supports(),
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("validated profiles satisfy run_from_areas preconditions");
    assert!(
        sim.layers().iter().all(|l| l.crack_front_fraction.is_none()),
        "run_from_areas (synthetic solid masks) must never record a crack"
    );
}
