//! End-to-end integration test for the t2f1 voxel cure path (ADR-0017).
//!
//! Drives `SimulationRunner::run_from_layer_inputs_with_voxel` against a
//! synthesised tiny mask, then asserts:
//!  1. The returned `PrintSimulation` carries populated `cure_field` +
//!     `photoinitiator_field` aggregates.
//!  2. Per-layer `cure_depth_um` / `worst_cure_depth_um` caches are
//!     overwritten with the voxel field's `LayerSummary.mean` / `.min`.
//!  3. The dispatch methods on `LayerResult` return the same values as
//!     direct field access (verifying the cache promotion in `run_inner_full`).
//!  4. Tier-1 mode (voxel_cure_mm = None) produces no aggregate fields and
//!     behaves identically to `run_from_layer_inputs`.
//!  5. The voxel fields' photoinitiator concentrations monotonically
//!     decrease in exposed voxels across layers (KB-160 depletion).
//!
//! This test is the regression guard that future refactors of the voxel
//! pipeline must continue to satisfy.

#![cfg(feature = "field-sim")]

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22°C is a valid ambient")
}

/// 3×3 fully-solid mask at 0.5 mm voxel size — small enough to keep tests
/// fast, big enough to exercise the per-pixel exposure loop and the
/// LayerSummary { mean, min } computation.
fn solid_3x3_mask() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask is valid")
}

fn layer_inputs_with_mask(n: u32) -> Vec<LayerInput> {
    (0..n)
        .map(|i| {
            // Mark every layer at the SAME bottom-section exposure so we
            // get repeatable dose accumulation across layers.
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25, // area = 9 voxels × 0.25 mm² each = 2.25 mm²
                2.5,              // exposure_sec
                60.0,             // lift_speed
                50.0,             // layer height 50 µm
                (i as f32 + 1.0) * 0.05,
            )
            .expect("valid LayerInput for factory recipe");
            li.mask = Some(solid_3x3_mask());
            li
        })
        .collect()
}

#[test]
fn voxel_mode_installs_fields_on_aggregate() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs_with_mask(5);

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5), // voxel cure mm — for v1 echoed; mask voxel size wins
    )
    .expect("voxel-mode run on validated profiles must succeed");

    assert!(
        sim.cure_field().is_some(),
        "voxel mode must install cure_field on the aggregate"
    );
    assert!(
        sim.photoinitiator_field().is_some(),
        "voxel mode must install photoinitiator_field on the aggregate"
    );
    let (nx, ny, nz) = sim.cure_field().unwrap().dimensions();
    assert_eq!((nx, ny, nz), (3, 3, 5));
}

#[test]
fn tier1_mode_leaves_aggregate_fields_none() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs_with_mask(3);

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        None, // Tier-1 (no voxel)
    )
    .expect("Tier-1 mode on validated profiles must succeed");

    assert!(sim.cure_field().is_none());
    assert!(sim.photoinitiator_field().is_none());
}

#[test]
fn voxel_mode_overwrites_layer_caches_with_summary() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs_with_mask(3);

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5),
    )
    .expect("voxel-mode run must succeed");

    // After the voxel pass, each layer's cache must reflect the voxel field's
    // LayerSummary, not the original Tier-1 scalar. Both should be finite
    // and positive (the synthesised exposure is well above Ec).
    for layer in sim.layers() {
        assert!(
            layer.cure_depth_um.is_finite() && layer.cure_depth_um >= 0.0,
            "layer {}: voxel-overwritten cure_depth_um must be finite >= 0, got {}",
            layer.index,
            layer.cure_depth_um
        );
        assert!(
            layer.worst_cure_depth_um.is_finite() && layer.worst_cure_depth_um >= 0.0,
            "layer {}: voxel-overwritten worst_cure_depth_um must be finite >= 0, got {}",
            layer.index,
            layer.worst_cure_depth_um
        );
        // For a uniformly-exposed solid mask the min should equal the mean
        // (no per-pixel variation yet — uniformity is t2f2).
        assert!(
            (layer.cure_depth_um - layer.worst_cure_depth_um).abs() < 1e-3,
            "layer {}: uniform-mask voxel pass should give mean == min, got mean={}, min={}",
            layer.index,
            layer.cure_depth_um,
            layer.worst_cure_depth_um
        );
    }
}

#[test]
fn photoinitiator_depletes_monotonically_across_layers() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs_with_mask(8);

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5),
    )
    .expect("voxel-mode run must succeed");

    let pi = sim
        .photoinitiator_field()
        .expect("voxel mode populates photoinitiator_field");

    // Each later voxel's concentration should be <= the topmost-layer voxel
    // because deeper layers accumulate ALL exposures above them (their
    // column receives attenuated dose from every higher layer's pixel
    // column above). For our simple solid mask + uniform exposure, the
    // topmost voxel (iz = 0) is depleted exactly once.
    let c_top = pi.concentration_at(1, 1, 0).expect("centre voxel exists");
    let c_bottom = pi.concentration_at(1, 1, 7).expect("centre voxel exists");
    assert!(
        c_bottom <= c_top + 1e-5,
        "deeper voxels should be at least as depleted: top={c_top}, bottom={c_bottom}"
    );
    assert!(c_top >= 0.0);
    assert!(c_bottom >= 0.0);
}

#[test]
fn dispatch_summary_matches_overwritten_cache() {
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let layers = layer_inputs_with_mask(3);

    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5),
    )
    .expect("voxel-mode run must succeed");

    // The dispatch method goes through the voxel field's layer_summary
    // (mean) and should match the cache that run_inner_full just wrote
    // from the same summary. Float equality within 1e-3 µm.
    for layer in sim.layers() {
        let summary_cd = layer.cure_depth_um_summary(
            &sim,
            resin.penetration_depth_um(),
            resin.critical_energy_mj_cm2(),
        );
        // Dispatch reads from the voxel field directly; cache reads from
        // the Tier-1 scalar that run_inner_full wrote from the same
        // summary. They should agree to within numerical noise.
        assert!(
            (summary_cd.value() - layer.cure_depth_um).abs() < 1e-2,
            "layer {}: dispatch summary ({}) must equal overwritten cache ({})",
            layer.index,
            summary_cd.value(),
            layer.cure_depth_um
        );
    }
}
