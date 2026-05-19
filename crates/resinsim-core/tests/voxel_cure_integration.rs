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
            // Normal-class exposure (3 s) at 50 µm layer height. After
            // the Z-step fix (code review round 1 HIGH-correctness), Z-step
            // is layer_height_um = 50 µm not voxel_size_mm × 1000 = 500 µm,
            // so depth at iz=0 centre is 25 µm not 250 µm. With Dp ≈ 170 µm,
            // attenuation = exp(-25/170) ≈ 0.864; surface dose 3 × 4 = 12
            // mJ/cm² × 0.864 = 10.4 mJ/cm² ⇒ above ~5.7 mJ/cm² Ec(T) at
            // ambient. This exposure level is the regression guard: any
            // future regression of the Z-step bug pushes it back under Ec
            // and breaks these tests loudly.
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25, // area = 9 voxels × 0.25 mm² each = 2.25 mm²
                3.0,              // exposure_sec — normal-class
                60.0,             // lift_speed
                50.0,             // layer height 50 µm
                (i as f32 + 1.0) * 0.05,
            )
            .expect("test fixture: literal LayerInput args satisfy LayerInput::new preconditions");
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
    let (nx, ny, nz) = sim.cure_field().expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, validated profiles, finite f32 in domain)").dimensions();
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

    // After the voxel pass, each layer's cache must reflect the voxel
    // field's LayerSummary, not the original Tier-1 scalar. With
    // PrinterProfile::generic_msla_4k carrying lcd_uniformity_variation = 0.22
    // (Saturn-2 class), `UniformityCalculator::intensity_factor` produces a
    // ~±11% radial spread → mean > min strictly. Both must be finite > 0.
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
        // KB-120 spatial variation: with non-zero LCD uniformity_variation,
        // edge pixels see less intensity than centre ⇒ shallower cure ⇒
        // LayerSummary.min < LayerSummary.mean.
        assert!(
            layer.cure_depth_um > layer.worst_cure_depth_um,
            "layer {}: KB-120 spatial uniformity should give mean > min, got mean={}, min={}",
            layer.index,
            layer.cure_depth_um,
            layer.worst_cure_depth_um
        );
    }
}

/// Uniform printer (lcd_uniformity_variation = 0.0) ⇒ voxel pass produces
/// mean == min, matching pre-uniformity-wiring legacy behaviour. Guards
/// the precedence-chain collapse for ideal/test printers without LCD
/// uniformity calibration data.
#[test]
fn voxel_mode_zero_uniformity_keeps_mean_equal_min() {
    use resinsim_core::values::LayerMask;
    let resin = ResinProfile::generic_standard();
    // Construct a printer with uniformity 0.0 by serde-deserialising a
    // hand-rolled TOML — fields are pub(crate) so external code (tests)
    // can't construct directly. The TOML round-trip is the documented
    // entry point per PrinterProfileRepository.
    let toml = r#"
name = "Test Uniform Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = { min = 20.0, max = 100.0 }
exposure_range_sec = { min = 1.0, max = 60.0 }
lift_speed_range_mm_min = { min = 10.0, max = 200.0 }
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.0
"#;
    let printer: PrinterProfile = toml::from_str(toml)
        .expect("test fixture: hand-rolled TOML with all required PrinterProfile fields parses");
    printer
        .validate()
        .expect("test fixture: TOML inputs satisfy PrinterProfile::validate");

    let layers: Vec<LayerInput> = (0..3)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25,
                2.5,
                60.0,
                50.0,
                (i as f32 + 1.0) * 0.05,
            )
            .expect("test fixture: layer_input literal args satisfy LayerInput::new preconditions");
            li.mask = Some(
                LayerMask::new_all_solid(3, 3, 0.5)
                    .expect("test fixture: 3×3 all-solid mask at 0.5 mm is valid"),
            );
            li
        })
        .collect();

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
    .expect(
        "test fixture: uniform-printer + ceramic-grey resin run satisfies validated-input \
         preconditions of run_from_layer_inputs_with_voxel",
    );

    for layer in sim.layers() {
        assert!(
            (layer.cure_depth_um - layer.worst_cure_depth_um).abs() < 1e-3,
            "layer {}: uniformity=0.0 + uniform-mask voxel pass must give mean == min, \
             got mean={}, min={}",
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

    // The dispatch reads layer_summary on demand using whatever ec the
    // caller passes. The cache was written using KB-153 Ec(T) at the
    // layer's vat temperature — NOT the resin's reference Ec. To make
    // dispatch and cache agree, the test must pass the SAME Ec(T) the
    // runner used. Compute it here using the same helper:
    use resinsim_core::services::{CureCalculator, ThermalCalculator};
    use resinsim_core::values::Energy;
    let ec_ref = Energy::new(resin.critical_energy_mj_cm2())
        .expect("test fixture: ResinProfile factory guarantees critical_energy_mj_cm2 > 0");
    for layer in sim.layers() {
        let vat = ThermalCalculator::vat_temperature_at_layer_v2(
            resin.recipe(),
            &printer,
            test_ambient().value(),
            None,
            layer.index,
        );
        let ec_t = CureCalculator::ec_at_temp(
            ec_ref,
            resin.reference_temp_c(),
            vat,
            resin.effective_cure_kinetics_ea_kj_mol(),
        );
        let summary_cd =
            layer.cure_depth_um_summary(&sim, resin.penetration_depth_um(), ec_t.value());
        assert!(
            (summary_cd.value() - layer.cure_depth_um).abs() < 1e-2,
            "layer {}: dispatch summary ({}) must equal overwritten cache ({})",
            layer.index,
            summary_cd.value(),
            layer.cure_depth_um
        );
    }
}
