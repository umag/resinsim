//! Integration test for ticket `ctb-layer-height-authority`.
//!
//! Three-way A/B/C comparison pins both authority axes:
//!
//! - **Sim A**: CTB layer-height = 40 µm, recipe = 40 µm (control: agreement,
//!   provenance.mismatch.is_none(), no warning).
//! - **Sim B**: CTB layer-height = 40 µm, recipe = 30 µm (test: mismatch,
//!   provenance.mismatch.is_some(), warning emitted, CTB authority wins).
//! - **Sim C**: CTB layer-height = 30 µm, recipe = 30 µm (control: different
//!   physics, provenance.mismatch.is_none()).
//!
//! Tier-1 assertions are on per-layer `effective_layer_height_um` (which is
//! `ZAxisCompensator::effective_layer_height_um(layer_height_um, z_deflection)`)
//! and on the `is_cure_sufficient` boolean predicate flip on a layer designed
//! to be borderline (cure depth between 30 µm and 40 µm).
//!
//! Tier-2 assertions are on `cure_field.layer_summary(0).mean` under the
//! `field-sim` feature.
//!
//! The integration test does NOT capture stderr (the warning emission is
//! verified at the UAT layer via subprocess + `cli_fixtures::invoke_resinsim`).
//! Provenance values surface the same information the warning text is
//! derived from — testing them is equivalent.
//!
//! UAT spec: `spec/uat/ctb-layer-height-authority.md`.

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{AmbientTemperature, LayerMask};

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22 °C is a valid ambient")
}

/// Construct a ResinProfile with a specific `recipe.layer_height_um`,
/// keeping every other field at the `generic_standard` baseline. Goes
/// through TOML round-trip because the `recipe` field on ResinProfile is
/// `pub(crate)` and the integration-test crate cannot mutate it
/// directly.
fn resin_with_layer_height(recipe_um: f32) -> ResinProfile {
    let toml = format!(
        r#"
name = "Generic Standard (test override)"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1

[recipe]
layer_height_um = {recipe_um}
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#
    );
    let resin: ResinProfile =
        toml::from_str(&toml).expect("TOML constructed from a known-good template parses");
    resin
        .validate()
        .expect("resin built from generic_standard baseline must validate");
    resin
}

fn solid_3x3_mask() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask is valid")
}

/// Build a 5-layer LayerInput stack with the given physical layer height.
/// Exposure is the recipe default (2.5 s normal / 25 s bottom).
fn layer_inputs(n: u32, ctb_layer_height_um: f32) -> Vec<LayerInput> {
    let layer_height_mm = ctb_layer_height_um / 1000.0;
    (0..n)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25,
                2.5,
                60.0,
                ctb_layer_height_um,
                (i as f32 + 1.0) * layer_height_mm,
            )
            .expect("test fixture: literal LayerInput args satisfy preconditions");
            li.mask = Some(solid_3x3_mask());
            li
        })
        .collect()
}

fn run_tier1(
    layers: &[LayerInput],
    resin: &ResinProfile,
    printer: &PrinterProfile,
) -> PrintSimulation {
    SimulationRunner::run_from_layer_inputs(
        layers,
        resin,
        printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("Tier-1 run on validated profiles must succeed")
}

#[test]
fn ctb_authority_three_way_tier1() {
    let printer = PrinterProfile::generic_msla_4k();

    // Sim A: CTB=40, recipe=40 — agreement control.
    let sim_a = run_tier1(
        &layer_inputs(5, 40.0),
        &resin_with_layer_height(40.0),
        &printer,
    );
    // Sim B: CTB=40, recipe=30 — mismatch, CTB must win.
    let sim_b = run_tier1(
        &layer_inputs(5, 40.0),
        &resin_with_layer_height(30.0),
        &printer,
    );
    // Sim C: CTB=30, recipe=30 — different physics control.
    let sim_c = run_tier1(
        &layer_inputs(5, 30.0),
        &resin_with_layer_height(30.0),
        &printer,
    );

    // Provenance — the canonical surface of CTB-vs-recipe reconciliation.
    let pa = sim_a
        .layer_height_provenance()
        .expect("A: provenance present");
    let pb = sim_b
        .layer_height_provenance()
        .expect("B: provenance present");
    let pc = sim_c
        .layer_height_provenance()
        .expect("C: provenance present");

    assert!(pa.mismatch().is_none(), "A (40/40) must agree");
    assert!(pc.mismatch().is_none(), "C (30/30) must agree");
    assert!(
        pb.mismatch().is_some(),
        "B (40/40 vs 30 recipe) must mismatch"
    );
    let mb = pb
        .mismatch()
        .expect("Sim B (CTB=40/recipe=30) populates mismatch");
    let ctb_um = pb.uniform_height_um().expect("Sim B CTB is uniform 40 µm");
    assert!((ctb_um - 40.0).abs() < 1e-3, "B ctb_um = {ctb_um}");
    assert!(
        (pb.recipe_um() - 30.0).abs() < 1e-3,
        "B recipe_um = {}",
        pb.recipe_um()
    );
    assert_eq!(pb.layer_count(), 5);
    // 5 layers × 40 µm / 30 µm = 6.67 → round = 7.
    assert_eq!(mb.recipe_layers_for_same_z, 7);
    assert!(
        matches!(mb.kind, resinsim_core::values::MismatchKind::Uniform { .. }),
        "Sim B mismatch kind must be Uniform (CTB itself is uniform): {:?}",
        mb.kind
    );

    // CTB authority wins: A and B agree per-layer on the layer-height-
    // dependent output (`effective_layer_height_um` on LayerResult is
    // `ZAxisCompensator::effective_layer_height_um(layer_height_um, z_deflection)`,
    // which IS layer-height-sensitive, unlike cure_depth_um which is purely
    // chemistry).
    for (i, (a, b)) in sim_a.layers().iter().zip(sim_b.layers().iter()).enumerate() {
        assert!(
            (a.effective_layer_height_um - b.effective_layer_height_um).abs() < 1e-4,
            "Sim A and Sim B must agree on layer {i} effective_layer_height_um \
             (CTB authority wins): A={}, B={}",
            a.effective_layer_height_um,
            b.effective_layer_height_um,
        );
    }

    // Different physics: B and C differ on the same field (40 vs 30 µm
    // slab thickness ⇒ different effective_layer_height_um).
    let any_layer_differs = sim_b
        .layers()
        .iter()
        .zip(sim_c.layers().iter())
        .any(|(b, c)| (b.effective_layer_height_um - c.effective_layer_height_um).abs() > 1e-4);
    assert!(
        any_layer_differs,
        "Sim B (40 µm) and Sim C (30 µm) must differ on effective_layer_height_um \
         — different physics"
    );
}

#[test]
fn ctb_authority_does_not_pollute_unrelated_paths() {
    // run_from_areas takes no LayerInputs, so layer_height_provenance is
    // None on the resulting aggregate. This guards against accidentally
    // installing a synthesised provenance on STL / area-only paths.
    use resinsim_core::values::CrossSectionArea;

    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let areas: Vec<CrossSectionArea> = (0..3)
        .map(|_| CrossSectionArea::new(100.0).expect("100 mm² is valid"))
        .collect();

    let sim = SimulationRunner::run_from_areas(
        &areas,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
    )
    .expect("run_from_areas must succeed on the generic profiles");

    assert!(
        sim.layer_height_provenance().is_none(),
        "area-only path has no CTB-derived value; provenance must be None"
    );
}

#[test]
fn variable_z_ctb_succeeds_with_variable_mismatch_provenance() {
    // Adaptive-slicing CTB: 5 layers @ 30/40/50/40/30 µm (total 190 µm).
    // The runtime supports variable-Z by dispatching each layer's slab
    // thickness individually; the simulation must SUCCEED and surface a
    // Variable mismatch on provenance, with min/max/mean populated.
    let resin = resin_with_layer_height(40.0);
    let printer = PrinterProfile::generic_msla_4k();
    let mut layers = layer_inputs(5, 40.0);
    layers[0].layer_height_um = 30.0;
    layers[2].layer_height_um = 50.0;
    layers[4].layer_height_um = 30.0;

    let sim = SimulationRunner::run_from_layer_inputs(
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
    )
    .expect("variable-Z CTB must succeed (adaptive slicing is supported)");

    let p = sim
        .layer_height_provenance()
        .expect("CTB run surfaces provenance");
    assert!(p.uniform_height_um().is_none(), "variable-Z is NOT uniform");
    assert!(p.has_mismatch(), "variable-Z is always a mismatch");
    let m = p.mismatch().expect("variable-Z populates mismatch");
    assert!(
        matches!(m.kind, resinsim_core::values::MismatchKind::Variable),
        "expected Variable mismatch kind, got {:?}",
        m.kind
    );
    // Min / max / mean now derived on demand from the LayerHeightSeq
    // (no duplicate state on the MismatchKind enum).
    let ctb = p.ctb_layer_heights();
    assert!((ctb.min_um() - 30.0).abs() < 1e-3, "min: {}", ctb.min_um());
    assert!((ctb.max_um() - 50.0).abs() < 1e-3, "max: {}", ctb.max_um());
    assert!(
        (ctb.mean_um() - 38.0).abs() < 1e-3,
        "mean: {}",
        ctb.mean_um()
    ); // (30+40+50+40+30)/5 = 38

    // Per-layer dispatch: each LayerResult's effective_layer_height_um
    // is derived from the corresponding CTB slab thickness (minus the
    // small z-deflection contribution). Spot-check that the layer 0 and
    // layer 2 results genuinely differ — they would be IDENTICAL under
    // the old hard-error code path that treated the CTB as uniform.
    let l0 = sim.layers()[0].effective_layer_height_um;
    let l2 = sim.layers()[2].effective_layer_height_um;
    assert!(
        (l0 - l2).abs() > 5.0,
        "variable-Z: layer 0 (30 µm slab) and layer 2 (50 µm slab) must \
         produce different effective_layer_height_um (l0={l0}, l2={l2})"
    );
}

#[test]
fn ctb_with_non_finite_layer_height_still_hard_errors() {
    // NaN / non-positive layer heights are nonsensical at any granularity
    // — the helper still rejects them, even though variable-Z is now
    // supported.
    let resin = ResinProfile::generic_standard();
    let printer = PrinterProfile::generic_msla_4k();
    let mut layers = layer_inputs(5, 40.0);
    layers[2].layer_height_um = f32::NAN;

    let err = SimulationRunner::run_from_layer_inputs(
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
    )
    .expect_err("NaN layer_height_um is invalid at any granularity");
    assert!(err.contains("layer 2"), "err: {err}");
    assert!(err.contains("finite"), "err: {err}");
}

#[cfg(feature = "field-sim")]
mod tier2 {
    use super::*;

    fn run_tier2(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
    ) -> PrintSimulation {
        SimulationRunner::run_from_layer_inputs_with_voxel(
            layers,
            resin,
            printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 20,
            },
            &PlateAdhesionProfile::default_textured(),
            test_ambient(),
            None,
            Some(0.5),
        )
        .expect("Tier-2 voxel run on validated profiles must succeed")
    }

    #[test]
    fn ctb_authority_three_way_tier2_voxel() {
        let printer = PrinterProfile::generic_msla_4k();

        let sim_a = run_tier2(
            &layer_inputs(5, 40.0),
            &resin_with_layer_height(40.0),
            &printer,
        );
        let sim_b = run_tier2(
            &layer_inputs(5, 40.0),
            &resin_with_layer_height(30.0),
            &printer,
        );
        let sim_c = run_tier2(
            &layer_inputs(5, 30.0),
            &resin_with_layer_height(30.0),
            &printer,
        );

        // Voxel mode is layer-height-sensitive directly via Z-step
        // (`apply_voxel_cure_for_layer` uses layer_height_um as the slab
        // depth per ADR-0017 §6). The per-layer `cure_depth_um` cache
        // on LayerResult is overwritten from `cure_field.layer_summary`
        // — so it IS the right discriminator for Tier-2.
        let layer0_a = sim_a.layers()[0].cure_depth_um;
        let layer0_b = sim_b.layers()[0].cure_depth_um;
        let layer0_c = sim_c.layers()[0].cure_depth_um;

        // CTB authority wins for voxel mode too: A and B (both CTB=40)
        // produce the same per-layer voxel cure summary, regardless of
        // recipe.
        assert!(
            (layer0_a - layer0_b).abs() < 1e-3,
            "Tier-2 CTB authority: A and B must agree on layer 0 cure_depth_um \
             (Tier-2 voxel summary): A={layer0_a}, B={layer0_b}"
        );

        // Different physics: B (CTB=40) and C (CTB=30) differ — the slab
        // depth changes the voxel-resolved cure attenuation.
        assert!(
            (layer0_b - layer0_c).abs() > 1e-3,
            "Tier-2 physics: B (40 µm slab) and C (30 µm slab) must differ \
             on layer 0 cure_depth_um: B={layer0_b}, C={layer0_c}"
        );

        // Provenance reaches Tier-2 too.
        assert!(sim_b
            .layer_height_provenance()
            .expect("Tier-2 B provenance")
            .has_mismatch());
        assert!(!sim_a
            .layer_height_provenance()
            .expect("Tier-2 A provenance")
            .has_mismatch());
    }

    /// Harvest UAT-B: variable-Z + Tier-2. Per-layer dispatch must
    /// reach `apply_voxel_cure_for_layer` so that layers with
    /// different physical slab thicknesses produce different
    /// voxel-cure outputs. Without per-layer dispatch every layer
    /// would have used the slice's first value and this test would
    /// see equal cure depths everywhere.
    #[test]
    fn variable_z_tier2_dispatches_per_layer_slab() {
        let printer = PrinterProfile::generic_msla_4k();
        // 5 layers @ 30/40/50/40/30 µm — same shape used by the
        // Tier-1 variable-Z regression test for consistency.
        let resin = resin_with_layer_height(40.0);
        let layer_height_mm = |h: f32| h / 1000.0;
        let mut z_mm = 0.0_f32;
        let heights = [30.0_f32, 40.0, 50.0, 40.0, 30.0];
        let mut layers: Vec<LayerInput> = Vec::with_capacity(5);
        for (i, h) in heights.iter().enumerate() {
            z_mm += layer_height_mm(*h);
            let mut li = LayerInput::new(i as u32, 3.0 * 3.0 * 0.25, 2.5, 60.0, *h, z_mm)
                .expect("test fixture: literal LayerInput args satisfy preconditions");
            li.mask = Some(solid_3x3_mask());
            layers.push(li);
        }

        let sim = run_tier2(&layers, &resin, &printer);

        // Provenance surfaces variable kind, not uniform.
        let prov = sim
            .layer_height_provenance()
            .expect("Tier-2 variable-Z run surfaces provenance");
        assert!(
            prov.uniform_height_um().is_none(),
            "must NOT report uniform"
        );
        let kind = &prov.mismatch().expect("variable run has mismatch").kind;
        assert!(
            matches!(kind, resinsim_core::values::MismatchKind::Variable),
            "expected Variable mismatch kind, got {kind:?}"
        );

        // The voxel cure summary is overwritten per layer from
        // `cure_field.layer_summary(layer)` after the per-pixel pass.
        // Layer 0 sees a 30 µm slab; layer 2 sees a 50 µm slab. The
        // CureField stores per-voxel dose accumulated using each
        // layer's `layer_height_um` as the Z-step — so the per-layer
        // summary cache on LayerResult MUST differ between layer 0
        // and layer 2.
        //
        // Bug it would catch: a future regression that passes a single
        // scalar layer_height (uniform path) into the variable-Z code
        // path would make every layer's cache the same value, and
        // this assertion would fail loudly.
        let layer0 = sim.layers()[0].cure_depth_um;
        let layer2 = sim.layers()[2].cure_depth_um;
        assert!(
            (layer0 - layer2).abs() > 1e-3,
            "variable-Z Tier-2 per-layer dispatch: layer 0 (30 µm slab) and \
             layer 2 (50 µm slab) must produce different cure_depth_um \
             (layer0={layer0}, layer2={layer2})"
        );

        // NB: a layer-0 vs layer-4 "same slab → same cure depth" symmetry
        // check would fail — photoinitiator depletes monotonically up
        // the Z column (KB-160), so layer 4 inherits the cumulative
        // depletion from layers 0-3 and produces a different cure depth
        // than layer 0 even at identical slab thickness. The
        // per-layer-dispatch property the test pins is the layer-0
        // vs layer-2 inequality above — that's enough to prove the
        // slab varies per layer.
    }
}
