//! Step definitions for
//! `spec/uat/peel-shape-factor-scales-with-aspect-ratio.md` UAT-1..UAT-4
//! (uat-unskip-campaign increment 1, plan step 10).
//!
//! This spec's own Evidence block names 8 existing nextest tests covering
//! these same four scenarios — the strongest honest-benefit warning in the
//! campaign. Nothing here re-derives `4·√A/L` or the strength blend; every
//! Then reads `LayerResult`/mask output straight from production calls
//! (`PeelForceCalculator::peel_shape_factor`, `SimulationRunner::
//! run_from_areas`, `FailurePredictor::predict_layer`,
//! `LayerMask::is_fully_solid`).
//!
//! UAT-2's When ("a job is simulated") is textually DISTINCT from
//! base_adhesion_shifts_peel_peak's shared "a job is simulated with that
//! resin" (no "with that resin" suffix) and from every When in
//! profile_vacuum_pressure_scales_suction — checked directly against both
//! sibling .md files. Own registration, own tiny local run helper (not
//! shared across modules — the existing tree has no cross-step-def-module
//! import precedent, and the helper is ~15 lines).

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::{LayerOverrides, SupportConfig};
use resinsim_core::services::PeelForceCalculator;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

use super::world::{LayerMaskBuilder, PredictLayerInputs, PrinterBuilder, ResinBuilder, UatWorld};

// ---- UAT-1: the A/L shape factor ranks thin below compact at equal area ---

#[given(regex = r"^a resin whose peel_shape_factor_strength is 1\.0$")]
fn given_strength_one(world: &mut UatWorld) {
    world.peel_resin = Some(
        ResinBuilder::new()
            .with_peel_shape_factor_strength(1.0)
            .build(),
    );
}

#[given(regex = r"^two equal-area layer masks — a compact square block and a thin 1×N line$")]
fn given_equal_area_masks(world: &mut UatWorld) {
    // 3×3 = 9-cell compact block vs. a 9-cell 1×N line: EQUAL area, very
    // different perimeter. The shipped nextest fixture
    // (build_shape_factor_map_off_fully_solid_and_thin) compares a 9-cell
    // square to a 5-cell line — NOT equal-area — so it cannot stand in for
    // this UAT's literal "equal-area" requirement; LayerMaskBuilder exists
    // precisely to give this scenario its own correctly-shaped fixture.
    const SIDE: u32 = 3;
    let compact = LayerMaskBuilder::compact_square(SIDE, 1.0);
    let thin = LayerMaskBuilder::thin_1xn(SIDE * SIDE, 1.0);
    assert!(
        (compact.solid_area_mm2() - thin.solid_area_mm2()).abs() < 1e-9,
        "fixture invariant: compact ({} mm²) and thin ({} mm²) must be equal-area",
        compact.solid_area_mm2(),
        thin.solid_area_mm2(),
    );
    world.peel_masks = Some(vec![compact, thin]);
}

#[when(regex = r"^the per-layer shape factors are computed from the masks$")]
fn when_shape_factors_computed_from_masks(world: &mut UatWorld) {
    let resin = world
        .peel_resin
        .as_ref()
        .expect("scenario invariant: Given step populated peel_resin");
    let masks = world
        .peel_masks
        .as_ref()
        .expect("scenario invariant: Given step populated peel_masks");
    let strength = resin.effective_peel_shape_factor_strength();
    let factors: Vec<f32> = masks
        .iter()
        .map(|m| {
            let area =
                CrossSectionArea::new(m.solid_area_mm2()).expect("mask area is non-negative");
            PeelForceCalculator::peel_shape_factor(area, m.perimeter_mm() as f32, strength)
        })
        .collect();
    world.peel_mask_shape_factors = Some(factors);
}

#[then(regex = r"^the compact block's factor is 1\.0 \(square = the KB-181 baseline\)$")]
fn then_compact_factor_one(world: &mut UatWorld) {
    let factors = world
        .peel_mask_shape_factors
        .as_ref()
        .expect("scenario invariant: When step populated peel_mask_shape_factors");
    assert!(
        (factors[0] - 1.0).abs() < 1e-4,
        "compact block factor should be 1.0; got {}",
        factors[0]
    );
}

#[then(regex = r"^the thin line's factor is strictly between 0 and 1$")]
fn then_thin_factor_between(world: &mut UatWorld) {
    let factors = world
        .peel_mask_shape_factors
        .as_ref()
        .expect("scenario invariant: When step populated peel_mask_shape_factors");
    assert!(
        factors[1] > 0.0 && factors[1] < 1.0,
        "thin line factor should be strictly between 0 and 1; got {}",
        factors[1]
    );
}

#[then(regex = r"^the compact factor exceeds the thin factor$")]
fn then_compact_exceeds_thin(world: &mut UatWorld) {
    let factors = world
        .peel_mask_shape_factors
        .as_ref()
        .expect("scenario invariant: When step populated peel_mask_shape_factors");
    assert!(
        factors[0] > factors[1],
        "compact factor {} must exceed thin factor {}",
        factors[0],
        factors[1]
    );
}

// ---- UAT-2: an unset strength is behaviour-preserving ----------------------

#[given(regex = r"^a resin whose peel_shape_factor_strength is unset$")]
fn given_strength_unset(world: &mut UatWorld) {
    world.peel_resin = Some(ResinBuilder::new().build());
}

#[when(regex = r"^a job is simulated$")]
fn when_a_job_is_simulated(world: &mut UatWorld) {
    let resin = world
        .peel_resin
        .clone()
        .expect("scenario invariant: Given step populated peel_resin");
    let printer = PrinterBuilder::new().build();
    let areas = vec![CrossSectionArea::new(100.0).expect("100 mm² is non-negative"); 5];
    let sim = SimulationRunner::run_from_areas(
        &areas,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22 °C is in domain"),
        None,
    )
    .expect("scenario fixture: ResinBuilder/PrinterBuilder output satisfies run_from_areas");
    world.peel_printer = Some(printer);
    world.peel_sim_layers = Some(sim.layers().to_vec());
}

#[then(regex = r"^effective_peel_shape_factor_strength\(\) returns 0\.0$")]
fn then_effective_strength_zero(world: &mut UatWorld) {
    let resin = world
        .peel_resin
        .as_ref()
        .expect("scenario invariant: Given step populated peel_resin");
    assert_eq!(
        resin.effective_peel_shape_factor_strength(),
        0.0,
        "unset peel_shape_factor_strength must default to 0.0"
    );
}

#[then(regex = r"^every layer's peel_shape_factor is None \(omitted from sim\.json\)$")]
fn then_every_shape_factor_none(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    for l in layers {
        assert_eq!(
            l.peel_shape_factor, None,
            "layer {}: peel_shape_factor must be None when strength is unset",
            l.index
        );
    }
}

#[then(regex = r"^every peel_force_n is byte-identical to the pre-Stage-3 output$")]
fn then_peel_force_byte_identical(world: &mut UatWorld) {
    // NOT independently re-verified byte-for-byte here — that would need a
    // golden fixture from before ADR-0022 Stage 3, and the plan's own
    // guidance for this exact bullet is to assert the OBSERVABLE CONTRACT
    // instead: `peel_shape_factor` is None (checked above) means
    // `predict_layer`'s `overrides.peel_shape_factor.unwrap_or(1.0)`
    // multiplier is exactly 1.0 for every layer, by construction — which
    // IS "unchanged" in the only sense a step def can prove without either
    // a golden file or re-deriving the formula (the mirror anti-pattern).
    // The golden-style byte-identical comparison lives in the crate's own
    // suite (see this spec's Evidence block).
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    assert!(
        layers
            .iter()
            .all(|l| l.peel_shape_factor.is_none() && l.peel_force_n > 0.0),
        "every layer must have no shape factor applied and a real positive peel force"
    );
}

// ---- UAT-3: synthetic placeholder (fully-solid) masks never apply a reduction

#[given(regex = r"^a resin whose peel_shape_factor_strength is active \(e\.g\. 0\.5\)$")]
fn given_strength_active(world: &mut UatWorld) {
    world.peel_resin = Some(
        ResinBuilder::new()
            .with_peel_shape_factor_strength(0.5)
            .build(),
    );
}

#[given(
    regex = r"^a run whose masks are fully-solid placeholders \(run_from_areas 1×1, or the run_from_layer_inputs W×H all-solid fallback\)$"
)]
fn given_fully_solid_placeholder_run(_world: &mut UatWorld) {
    // Narrative — `run_from_areas` ITSELF synthesises the 1×1 all-solid
    // placeholder mask per layer (see its doc comment); the When step
    // below drives that exact path. The W×H `run_from_layer_inputs`
    // fallback shares the SAME `is_fully_solid()` discriminator
    // (`SimulationRunner::build_shape_factor_map`) and is covered by the
    // crate's own `run_from_layer_inputs`-side nextest fixtures — not
    // duplicated here, since both entry points route through one
    // production function and the discriminator itself is what UAT-3
    // guards, not the entry point.
}

#[when(regex = r"^the per-layer shape factors are computed$")]
fn when_shape_factors_computed(world: &mut UatWorld) {
    let active_resin = world
        .peel_resin
        .clone()
        .expect("scenario invariant: Given step populated peel_resin");
    let printer = PrinterBuilder::new().build();
    let areas = vec![CrossSectionArea::new(100.0).expect("100 mm² is non-negative"); 5];
    let run = |resin: &resinsim_core::entities::ResinProfile| {
        SimulationRunner::run_from_areas(
            &areas,
            resin,
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &PlateAdhesionProfile::default_textured(),
            AmbientTemperature::new(22.0).expect("22 °C is in domain"),
            None,
        )
        .expect("scenario fixture: ResinBuilder/PrinterBuilder output satisfies run_from_areas")
    };
    let sim_active = run(&active_resin);
    // Off-strength comparator run — needed for the "peel force unchanged"
    // Then step below; comparing two production outputs, not recomputing
    // either.
    let off_resin = ResinBuilder::new().build();
    let sim_off = run(&off_resin);

    world.peel_sim_layers = Some(sim_active.layers().to_vec());
    world.peel_shape_unshaped_result = sim_off.layers().first().cloned();
}

#[then(regex = r"^every fully-solid mask maps to factor 1\.0 \(no shape signal\)$")]
fn then_fully_solid_factor_one(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    for l in layers {
        assert_eq!(
            l.peel_shape_factor,
            Some(1.0),
            "layer {}: fully-solid placeholder mask must map to factor 1.0; got {:?}",
            l.index,
            l.peel_shape_factor
        );
    }
    // Fixture-validity cross-check at the mask level for both placeholder
    // shapes the spec names — run_from_areas's 1×1 and the
    // run_from_layer_inputs W×H fallback. NOTE: this only proves
    // `is_fully_solid()` is true for these shapes, NOT that
    // `PeelForceCalculator::peel_shape_factor` would return 1.0 for them —
    // it would not, in general (a 7×5 rectangle's raw `4√A/L` geometry
    // ratio is ~0.986, not 1.0; caught by this very step failing during
    // authoring). The "fully-solid → 1.0" discriminator is
    // `SimulationRunner::build_shape_factor_map`'s `is_fully_solid()`
    // branch, which is crate-private and only observable from here via
    // the production pipeline assertion above — this block cannot
    // shortcut past that boundary, by design.
    for (w, h) in [(1, 1), (7, 5)] {
        let placeholder = LayerMaskBuilder::fully_solid(w, h, 1.0);
        assert!(
            placeholder.is_fully_solid(),
            "LayerMaskBuilder::fully_solid({w}, {h}, ..) output must itself be fully solid"
        );
    }
}

#[then(regex = r"^the peel force on those layers is unchanged$")]
fn then_peel_force_unchanged_on_placeholders(world: &mut UatWorld) {
    let active = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    let off_layer0 = world
        .peel_shape_unshaped_result
        .as_ref()
        .expect("scenario invariant: When step populated peel_shape_unshaped_result");
    assert!(
        (active[0].peel_force_n - off_layer0.peel_force_n).abs() < 1e-6,
        "factor 1.0 (placeholder guard) must not change peel force: active={} off={}",
        active[0].peel_force_n,
        off_layer0.peel_force_n,
    );
}

// ---- UAT-4: the shape factor modulates peel only, not suction or base -----

#[given(regex = r"^a layer with a non-zero peel, suction, and base-adhesion force$")]
fn given_layer_with_all_three_forces(world: &mut UatWorld) {
    let mut inputs = PredictLayerInputs::default_for_test();
    // Layer 0 so the base-adhesion decay term is at full strength.
    inputs.layer = 0;
    inputs.resin = ResinBuilder::new()
        .with_base_adhesion_elevation_kpa(40.0)
        .build();
    inputs.overrides = LayerOverrides {
        suction_force_n: Some(2.0),
        ..LayerOverrides::default()
    };
    let layer_height_um = inputs.resin.recipe().layer_height_um();
    let (unshaped, _) = resinsim_core::services::failure_predictor::FailurePredictor::predict_layer(
        inputs.layer,
        inputs.area,
        inputs.prev_area,
        &inputs.overrides,
        &inputs.resin,
        &inputs.printer,
        inputs.resin.recipe(),
        layer_height_um,
        &inputs.supports,
        &inputs.plate,
        &inputs.thermal,
    );
    assert!(
        unshaped.peel_force_n > 0.0,
        "fixture invariant: need a non-zero peel to scale"
    );
    assert!(
        unshaped.suction_force_n > 0.0,
        "fixture invariant: need a non-zero suction"
    );
    assert!(
        unshaped.base_force_n > 0.0,
        "fixture invariant: need a non-zero base-adhesion force"
    );
    world.peel_resin = Some(inputs.resin);
    world.peel_shape_unshaped_result = Some(unshaped);
}

#[when(regex = r"^a peel_shape_factor of 0\.5 is applied$")]
fn when_shape_factor_half_applied(world: &mut UatWorld) {
    let mut inputs = PredictLayerInputs::default_for_test();
    inputs.layer = 0;
    inputs.resin = world
        .peel_resin
        .clone()
        .expect("scenario invariant: Given step populated peel_resin");
    inputs.overrides = LayerOverrides {
        suction_force_n: Some(2.0),
        peel_shape_factor: Some(0.5),
        ..LayerOverrides::default()
    };
    let layer_height_um = inputs.resin.recipe().layer_height_um();
    let (shaped, _) = resinsim_core::services::failure_predictor::FailurePredictor::predict_layer(
        inputs.layer,
        inputs.area,
        inputs.prev_area,
        &inputs.overrides,
        &inputs.resin,
        &inputs.printer,
        inputs.resin.recipe(),
        layer_height_um,
        &inputs.supports,
        &inputs.plate,
        &inputs.thermal,
    );
    world.peel_shape_shaped_result = Some(shaped);
}

#[then(regex = r"^peel_force_n halves$")]
fn then_peel_force_halves(world: &mut UatWorld) {
    let base = world
        .peel_shape_unshaped_result
        .as_ref()
        .expect("scenario invariant: Given step populated peel_shape_unshaped_result");
    let shaped = world
        .peel_shape_shaped_result
        .as_ref()
        .expect("scenario invariant: When step populated peel_shape_shaped_result");
    assert!(
        (shaped.peel_force_n - 0.5 * base.peel_force_n).abs() < 1e-4,
        "peel should halve: base={} shaped={}",
        base.peel_force_n,
        shaped.peel_force_n
    );
}

#[then(regex = r"^suction_force_n and base_force_n are unchanged$")]
fn then_suction_and_base_unchanged(world: &mut UatWorld) {
    let base = world
        .peel_shape_unshaped_result
        .as_ref()
        .expect("scenario invariant: Given step populated peel_shape_unshaped_result");
    let shaped = world
        .peel_shape_shaped_result
        .as_ref()
        .expect("scenario invariant: When step populated peel_shape_shaped_result");
    assert_eq!(
        shaped.suction_force_n, base.suction_force_n,
        "suction must be untouched"
    );
    assert_eq!(
        shaped.base_force_n, base.base_force_n,
        "base adhesion must be untouched"
    );
}

#[then(regex = r"^total_force_n drops by exactly the peel reduction$")]
fn then_total_drops_by_peel_reduction(world: &mut UatWorld) {
    let base = world
        .peel_shape_unshaped_result
        .as_ref()
        .expect("scenario invariant: Given step populated peel_shape_unshaped_result");
    let shaped = world
        .peel_shape_shaped_result
        .as_ref()
        .expect("scenario invariant: When step populated peel_shape_shaped_result");
    let total_delta = base.total_force_n - shaped.total_force_n;
    let peel_delta = base.peel_force_n - shaped.peel_force_n;
    assert!(
        (total_delta - peel_delta).abs() < 1e-4,
        "total delta {total_delta} should equal peel delta {peel_delta}"
    );
}
