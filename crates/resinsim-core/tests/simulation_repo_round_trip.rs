//! Property test: `SimulationRepository` save+load preserves the aggregate
//! across arbitrary layer counts.
//!
//! Pairs the `simulation_repo.rs` unit tests (which fix specific scenarios)
//! with shape-drift coverage: any random small `PrintSimulation` aggregate
//! must round-trip through JSON without changing its public projection
//! (SimSummary) or its layer / failure counts.
//!
//! Two complementary cases:
//!
//! 1. `save_load_round_trips_preserves_aggregate` (proptest, 256 cases by
//!    default) — uses manual `dummy_layer` + `add_layer` construction
//!    (mirrors `sim_summary_time_properties.rs`'s pattern) so each case is
//!    sub-millisecond. This covers shape drift across arbitrary layer
//!    counts 0..50, the short-print clamp regimes, and arbitrary failure
//!    distributions.
//!
//! 2. `run_from_areas_aggregate_round_trips` (single end-to-end case) —
//!    builds an aggregate via `SimulationRunner::run_from_areas` (the
//!    production simulation path) and round-trips through the repository.
//!    Locks in compatibility with realistic simulation output that the
//!    proptest's synthetic layers can't reach (real cure depths, real
//!    forces, real failures). See ADR-0009.

use proptest::prelude::*;
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{
    FailureEvent, FailureType, LayerResult, PrinterProfile, Recipe, ResinProfile, Severity,
};
use resinsim_core::repositories::SimulationRepository;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea, InitialLedTemperature};
use std::path::{Path, PathBuf};

fn dummy_layer(index: u32) -> LayerResult {
    LayerResult {
        index,
        cure_depth_um: 100.0,
        peel_force_n: 1.0,
        suction_force_n: 0.0,
        total_force_n: 1.0,
        support_capacity_n: 10.0,
        safety_factor: 10.0,
        cross_section_area_mm2: 100.0,
        area_delta_mm2: 0.0,
        vat_temperature_c: 22.0,
        viscosity_mpa_s: 200.0,
        z_deflection_um: 2.0,
        effective_layer_height_um: 48.0,
        worst_cure_depth_um: 100.0,
        strain_magnitude_max: None,
        stress_von_mises_max_mpa: None,
        strain_gradient_max_frac: None,
        voxel_yield_fraction: None,
    }
}

fn build_sim(n: u32, with_failures_at: &[u32]) -> PrintSimulation {
    let recipe: Recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    for i in 0..n {
        let failures = if with_failures_at.contains(&i) {
            vec![FailureEvent {
                layer: i,
                failure_type: FailureType::SupportOverload,
                severity: Severity::Warning,
                message: format!("synthetic failure at layer {i}"),
            }]
        } else {
            vec![]
        };
        sim.add_layer(dummy_layer(i), failures)
            .expect("test fixture: sequential index i in 0..n satisfies add_layer's contiguity precondition");
    }
    sim
}

/// Per-test directory under workspace `target/test-tmp/`. Cleaned at start.
fn property_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dir = Path::new(manifest_dir).join("../../target/test-tmp/sim-repo-property");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: must be able to create property test dir");
    dir
}

proptest! {
    /// Save then load an arbitrary small simulation; SimSummary projection
    /// + layers().len() + failures().len() must match the saved aggregate.
    /// Layer counts 0..50 cover the short-print clamp regimes
    /// (n=0, n inside bottom phase, n inside transition, n in normal).
    #[test]
    fn save_load_round_trips_preserves_aggregate(
        n in 0u32..50,
        case in 0u32..1_000_000,
    ) {
        let dir = property_dir();
        let repo = SimulationRepository::new(&dir);
        // Inject failures at a deterministic-from-case subset of layer
        // indices to exercise the failures Vec serialisation path.
        let failure_layers: Vec<u32> = (0..n).filter(|i| (case ^ *i) % 5 == 0).collect();
        let saved = build_sim(n, &failure_layers);
        let name = format!("case-{case}");
        repo.save(&name, &saved).expect("save must succeed");
        let loaded = repo.load(&name).expect("load must succeed");

        let s = saved.summary();
        let l = loaded.summary();
        prop_assert_eq!(s.total_layers, l.total_layers);
        prop_assert!(
            (s.max_peel_force_n - l.max_peel_force_n).abs() < 1e-6,
            "max_peel_force_n drifted: saved={} loaded={}",
            s.max_peel_force_n, l.max_peel_force_n
        );
        prop_assert!(
            (s.total_time_sec - l.total_time_sec).abs() < 1e-3,
            "total_time_sec drifted: saved={} loaded={}",
            s.total_time_sec, l.total_time_sec
        );
        prop_assert_eq!(s.critical_failures, l.critical_failures);
        prop_assert_eq!(s.warnings, l.warnings);

        prop_assert_eq!(saved.layers().len(), loaded.layers().len());
        prop_assert_eq!(saved.failures().len(), loaded.failures().len());
    }
}

/// Realistic-physics round-trip: aggregate built via
/// `SimulationRunner::run_from_areas` (the production simulation path), saved,
/// loaded, and projection compared. Pairs the proptest above (which uses
/// synthetic dummy_layer for speed) with one end-to-end case to lock in
/// compatibility with real simulation output. See ADR-0009.
#[test]
fn run_from_areas_aggregate_round_trips() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dir = Path::new(manifest_dir).join("../../target/test-tmp/sim-repo-runner-roundtrip");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: must be able to create dir");
    let repo = SimulationRepository::new(&dir);

    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let areas: Vec<CrossSectionArea> = (0..10)
        .map(|_| CrossSectionArea::new(100.0).expect("100 mm² is a valid CrossSectionArea"))
        .collect();

    let saved = SimulationRunner::run_from_areas(
        &areas,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(23.0).expect("23.0 °C is valid"),
        Some(InitialLedTemperature::new(27.0).expect("27.0 °C is valid")),
    )
    .expect("validated factory profile + 10 cube layers must run");

    repo.save("runner-case", &saved)
        .expect("save must succeed for SimulationRunner-built aggregate");
    let loaded = repo
        .load("runner-case")
        .expect("load must succeed; validate() must pass on a freshly run aggregate");

    let s = saved.summary();
    let l = loaded.summary();
    assert_eq!(s.total_layers, 10);
    assert_eq!(s.total_layers, l.total_layers);
    assert!((s.max_peel_force_n - l.max_peel_force_n).abs() < 1e-6);
    assert!((s.min_safety_factor - l.min_safety_factor).abs() < 1e-6);
    assert!((s.max_temperature_c - l.max_temperature_c).abs() < 1e-6);
    assert!((s.total_time_sec - l.total_time_sec).abs() < 1e-3);
    assert_eq!(saved.layers().len(), loaded.layers().len());
    assert_eq!(saved.failures().len(), loaded.failures().len());
}
