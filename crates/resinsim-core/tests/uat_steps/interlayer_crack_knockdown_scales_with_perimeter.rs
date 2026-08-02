//! Step definitions for
//! `spec/uat/interlayer-crack-knockdown-scales-with-perimeter.md` UAT-1..UAT-4
//! (uat-unskip-campaign increment A2).
//!
//! Every Then reads a production entry point — `CrackPropagator::
//! effective_bonded_fraction`, `SupportAnalyzer::assess(..).plate_capacity_n`,
//! `SimulationRunner::run_from_layer_inputs` -> `LayerResult` fields, and
//! `PrintSimulation::failures()` filtered on `FailureType::Delamination` —
//! never a re-typed `min(1, 4√A/P)`
//! (docs/patterns/anti/test-mirrors-production-formula.md). This spec's own
//! Evidence block names ~15 existing nextest tests covering these same four
//! scenarios (the same honest-benefit caveat increment 1's three modules
//! carried); the value here is traceability + register shrinkage, not new
//! defect-finding power.
//!
//! DISTINCTNESS CHECK (as increment 1's three modules record): every Given/
//! When/Then regex below was checked directly against
//! `base-adhesion-shifts-peel-peak.md`, `profile-vacuum-pressure-scales-
//! suction.md`, and `peel-shape-factor-scales-with-aspect-ratio.md` — none
//! collide, with exactly ONE exception. UAT-3's second Given ("a run whose
//! masks are fully-solid placeholders (run_from_areas 1×1, or the
//! run_from_layer_inputs W×H all-solid fallback)") is CHARACTER-IDENTICAL to
//! `peel_shape_factor_scales_with_aspect_ratio.rs`'s
//! `given_fully_solid_placeholder_run` registration (spec line 51 there,
//! line 53 here) — NOT re-registered here; see the pointer comment at UAT-3
//! below (docs/patterns/anti/cucumber-step-regex-ambiguity.md). Re-
//! registering it would fail ONLY at runtime ("Step match is ambiguous"),
//! with no compile-time signal.
//!
//! UAT-4's vacuous-assertion risk (findings-a2-adversarial, binding): the
//! "still holds" branch has no existing test precedent (every shipped
//! nextest/integration fixture pins the FIRES side of the Delamination
//! gate), so both When steps additionally call `SupportAnalyzer::assess`
//! directly and assert the actual `reduced_interlayer_n` vs the actual
//! shaped peel load from the SAME run — proving the two fixtures sit on
//! genuinely OPPOSITE sides of `crack.value() > 0 && reduced_interlayer_n <
//! peel.value()`, not merely that an event happened or didn't.
//!
//! All resin/printer/mask construction goes through `ResinBuilder` /
//! `PrinterBuilder` / `LayerMaskBuilder` — no hand-copied TOML
//! (docs/patterns/anti/fixture-copy-of-shared-builder.md).

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::FailureType;
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::services::{CrackPropagator, SupportAnalyzer};
use resinsim_core::values::{
    AmbientTemperature, CrackFront, CrossSectionArea, LayerMask, PeelForce,
};

use super::world::{LayerMaskBuilder, PrinterBuilder, ResinBuilder, UatWorld};

/// `PlateAdhesionProfile::default_textured()`'s `bottom_layer_count` (6),
/// mirrored here (not re-read from the built plate) only as the layer-count
/// bound for building a run long enough to reach one NORMAL layer — the
/// VALUE every Then reads is always `LayerResult` / `SupportAssessment`
/// output, never recomputed.
const BOTTOM_LAYER_COUNT: u32 = 6;
/// The first NORMAL layer index in a `BOTTOM_LAYER_COUNT + 1`-layer run.
const FIRST_NORMAL_LAYER: u32 = BOTTOM_LAYER_COUNT;

fn ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22 °C is in AmbientTemperature domain")
}

/// No tips at all — the risk this module's plan step flags: with the
/// default 10 supports, total capacity can still hold and the Delamination
/// gate would silently not fire, turning UAT-4's "fires" branch vacuous.
fn no_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.0,
        n_supports: 0,
    }
}

/// Matches the peel-physics band's shared default (`tip_radius_mm: 0.2,
/// n_supports: 10`) — used everywhere the support count is not itself under
/// test (the Delamination gate compares `plate_capacity_n` to the peel
/// load, never the total capacity, so support count cannot mask a real
/// crossing the way it can for `SupportOverload`).
fn default_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 10,
    }
}

/// `FIRST_NORMAL_LAYER + 1` `LayerInput`s sharing one mask, so the resulting
/// run reaches exactly one NORMAL layer (index `FIRST_NORMAL_LAYER`) —
/// mirrors `tests/crack_propagation_runner.rs`'s `thin_wall_layers` helper.
/// `exposure_sec`/`lift_speed_mm_min`/`layer_height_um` track
/// `ResinBuilder`'s recipe defaults (2.5, 60.0, 50.0) so
/// `run_from_layer_inputs`'s per-layer overrides agree with what
/// `ResinBuilder::new().build()` would apply anyway.
fn layers_sharing_mask(mask: &LayerMask) -> Vec<LayerInput> {
    let area = mask.solid_area_mm2();
    (0..=FIRST_NORMAL_LAYER)
        .map(|idx| {
            LayerInput::new(idx, area, 2.5, 60.0, 50.0, idx as f32 * 0.05)
                .expect("valid layer input")
                .with_mask(mask.clone())
        })
        .collect()
}

/// Run `layers` through the real `SimulationRunner::run_from_layer_inputs`
/// entry point with `ResinBuilder`/`PrinterBuilder` default fixtures.
fn run(
    layers: &[LayerInput],
    supports: &SupportConfig,
) -> resinsim_core::simulation::PrintSimulation {
    SimulationRunner::run_from_layer_inputs(
        layers,
        &ResinBuilder::new().build(),
        &PrinterBuilder::new().build(),
        supports,
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
    )
    .expect(
        "scenario fixture: ResinBuilder/PrinterBuilder output satisfies \
         run_from_layer_inputs preconditions",
    )
}

// ---- UAT-1: the A/P knockdown scales capacity with perimeter --------------

#[given(regex = r"^a NORMAL layer with a real per-layer perimeter$")]
fn given_normal_layer_with_perimeter(_world: &mut UatWorld) {
    // Narrative — the concrete masks (and the NORMAL-layer index they run
    // at) are built in the When step below, from the same
    // `LayerMaskBuilder` factories the peel-shape sibling module already
    // established (`compact_square` / `thin_1xn`).
}

#[given(
    regex = r"^a compact \(square\) reference and a thin \(high-perimeter\) variant at equal area$"
)]
fn given_compact_and_thin_masks(_world: &mut UatWorld) {
    // Narrative — see above. `LayerMaskBuilder::compact_square(3, 1.0)` /
    // `thin_1xn(9, 1.0)` are the exact equal-area (9 mm²) pair the plan's
    // step 2 verified: compact raw 4√9/12 = 1.0 (no crack), thin raw
    // 4√9/20 = 0.6 (crack 0.4).
}

#[when(regex = r"^each layer is assessed$")]
fn when_each_layer_is_assessed(world: &mut UatWorld) {
    const SIDE: u32 = 3;
    let compact = LayerMaskBuilder::compact_square(SIDE, 1.0);
    let thin = LayerMaskBuilder::thin_1xn(SIDE * SIDE, 1.0);
    assert!(
        (compact.solid_area_mm2() - thin.solid_area_mm2()).abs() < 1e-9,
        "fixture invariant: compact and thin masks must be equal-area"
    );

    // CrackPropagator::effective_bonded_fraction, compact-then-thin.
    world.crack_bonded_fractions = Some(vec![
        CrackPropagator::effective_bonded_fraction(
            compact.solid_area_mm2(),
            compact.perimeter_mm(),
        ),
        CrackPropagator::effective_bonded_fraction(thin.solid_area_mm2(), thin.perimeter_mm()),
    ]);

    // SupportAnalyzer::assess(..).plate_capacity_n, same order and fed the
    // SAME area/perimeter/layer as the full run below, so the two entry
    // points agree by construction rather than by coincidence.
    let resin = ResinBuilder::new().build();
    let plate = PlateAdhesionProfile::default_textured();
    let supports = default_supports();
    let mut capacities = Vec::new();
    for mask in [&compact, &thin] {
        let crack =
            CrackPropagator::crack_from_geometry(mask.solid_area_mm2(), mask.perimeter_mm());
        let area = CrossSectionArea::new(mask.solid_area_mm2()).expect("mask area is non-negative");
        let assessment = SupportAnalyzer::assess(
            FIRST_NORMAL_LAYER,
            area,
            PeelForce::new(0.0).expect("0.0 N is a valid PeelForce"),
            &resin,
            &supports,
            &plate,
            crack,
        );
        capacities.push(assessment.plate_capacity_n);
    }
    world.crack_interlayer_capacity_n = Some(capacities);

    // SimulationRunner::run_from_layer_inputs, full LayerResult stacks.
    world.crack_compact_layers = Some(
        run(&layers_sharing_mask(&compact), &supports)
            .layers()
            .to_vec(),
    );
    world.crack_thin_layers = Some(run(&layers_sharing_mask(&thin), &supports).layers().to_vec());
}

#[then(regex = r"^the compact layer's effective bonded fraction is 1\.0 \(square = 4√A/P\)$")]
fn then_compact_fraction_one(world: &mut UatWorld) {
    let fractions = world
        .crack_bonded_fractions
        .as_ref()
        .expect("scenario invariant: When step populated crack_bonded_fractions");
    assert!(
        (fractions[0] - 1.0).abs() < 1e-9,
        "compact square must be 1.0, got {}",
        fractions[0]
    );
}

#[then(regex = r"^the thin layer's effective bonded fraction is strictly between 0 and 1$")]
fn then_thin_fraction_between(world: &mut UatWorld) {
    let fractions = world
        .crack_bonded_fractions
        .as_ref()
        .expect("scenario invariant: When step populated crack_bonded_fractions");
    assert!(
        fractions[1] > 0.0 && fractions[1] < 1.0,
        "thin fraction should be strictly between 0 and 1, got {}",
        fractions[1]
    );
}

#[then(regex = r"^the thin layer's interlayer capacity and safety factor are strictly lower$")]
fn then_thin_capacity_and_sf_lower(world: &mut UatWorld) {
    let capacities = world
        .crack_interlayer_capacity_n
        .as_ref()
        .expect("scenario invariant: When step populated crack_interlayer_capacity_n");
    assert!(
        capacities[1] < capacities[0],
        "thin interlayer capacity {} must be strictly lower than compact {}",
        capacities[1],
        capacities[0]
    );

    let compact_layers = world
        .crack_compact_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_compact_layers");
    let thin_layers = world
        .crack_thin_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_thin_layers");
    let compact_sf = compact_layers[FIRST_NORMAL_LAYER as usize].safety_factor;
    let thin_sf = thin_layers[FIRST_NORMAL_LAYER as usize].safety_factor;
    assert!(
        thin_sf < compact_sf,
        "thin safety factor {thin_sf} must be strictly lower than compact {compact_sf}"
    );
}

#[then(
    regex = r"^the thin layer records a crack_front_fraction Some\(>0\); the compact records None$"
)]
fn then_crack_front_fraction_some_vs_none(world: &mut UatWorld) {
    let compact_layers = world
        .crack_compact_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_compact_layers");
    let thin_layers = world
        .crack_thin_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_thin_layers");
    assert_eq!(
        compact_layers[FIRST_NORMAL_LAYER as usize].crack_front_fraction, None,
        "compact (square) layer must record no crack"
    );
    let cf = thin_layers[FIRST_NORMAL_LAYER as usize]
        .crack_front_fraction
        .expect("thin NORMAL layer must record Some(crack_front_fraction)");
    assert!(cf > 0.0, "thin crack_front_fraction must be > 0, got {cf}");
}

// ---- UAT-2: the knockdown is CAPACITY-ONLY — the peel/total LOAD is unchanged

#[given(regex = r"^a NORMAL layer simulated with and without a real perimeter$")]
fn given_normal_layer_with_and_without_perimeter(_world: &mut UatWorld) {
    // Narrative — both runs are built in the When step below: a thin
    // (real-perimeter) mask and an EQUAL-AREA fully-solid (placeholder,
    // no-perimeter) mask, so `peel_force_n`/`total_force_n` (area-driven
    // only) are byte-identical while the crack knockdown differs.
}

#[when(regex = r"^the crack knockdown is applied$")]
fn when_crack_knockdown_is_applied(world: &mut UatWorld) {
    const SIDE: u32 = 3;
    let with_perimeter = LayerMaskBuilder::thin_1xn(SIDE * SIDE, 1.0);
    let without_perimeter = LayerMaskBuilder::fully_solid(SIDE, SIDE, 1.0);
    assert!(
        (with_perimeter.solid_area_mm2() - without_perimeter.solid_area_mm2()).abs() < 1e-9,
        "fixture invariant: both masks must be equal-area so peel_force_n/total_force_n \
         stay comparable across the two runs"
    );
    assert!(
        !with_perimeter.is_fully_solid(),
        "fixture invariant: the real-perimeter mask must not be fully solid"
    );
    assert!(
        without_perimeter.is_fully_solid(),
        "fixture invariant: the placeholder mask must be fully solid"
    );

    let supports = default_supports();
    world.crack_thin_layers = Some(
        run(&layers_sharing_mask(&with_perimeter), &supports)
            .layers()
            .to_vec(),
    );
    world.crack_compact_layers = Some(
        run(&layers_sharing_mask(&without_perimeter), &supports)
            .layers()
            .to_vec(),
    );
}

#[then(regex = r"^peel_force_n and total_force_n are byte-identical between the two runs$")]
fn then_forces_byte_identical(world: &mut UatWorld) {
    let with_perim = &world
        .crack_thin_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_thin_layers")
        [FIRST_NORMAL_LAYER as usize];
    let without_perim = &world
        .crack_compact_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_compact_layers")
        [FIRST_NORMAL_LAYER as usize];
    assert_eq!(
        with_perim.peel_force_n, without_perim.peel_force_n,
        "peel LOAD must be crack-invariant"
    );
    assert_eq!(
        with_perim.total_force_n, without_perim.total_force_n,
        "total LOAD must be crack-invariant"
    );
}

#[then(regex = r"^only the safety factor \(and any Delamination\) changes$")]
fn then_only_safety_factor_changes(world: &mut UatWorld) {
    let with_perim = &world
        .crack_thin_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_thin_layers")
        [FIRST_NORMAL_LAYER as usize];
    let without_perim = &world
        .crack_compact_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_compact_layers")
        [FIRST_NORMAL_LAYER as usize];
    assert!(
        with_perim.safety_factor < without_perim.safety_factor,
        "crack-reduced capacity must lower the safety factor: with_perimeter={} \
         without_perimeter={}",
        with_perim.safety_factor,
        without_perim.safety_factor
    );
    assert!(
        with_perim.crack_front_fraction.is_some(),
        "the real-perimeter run must record a crack"
    );
    assert_eq!(
        without_perim.crack_front_fraction, None,
        "the placeholder run must record no crack"
    );
}

// ---- UAT-3: bottom layers and placeholder masks are never knocked down ----

#[given(regex = r"^a bottom layer \(below the plate bottom_layer_count\) with any perimeter$")]
fn given_bottom_layer_with_any_perimeter(_world: &mut UatWorld) {
    // Narrative — the concrete bottom-layer/thin-mask pairing is built in
    // the When step below, mirroring the shipped
    // `assess_bottom_layer_crack_does_not_reduce_plate_adhesion` nextest
    // fixture (support_analyzer.rs).
}

// NOTE: `^a run whose masks are fully-solid placeholders \(run_from_areas
// 1×1, or the run_from_layer_inputs W×H all-solid fallback\)$` Given step
// registered in peel_shape_factor_scales_with_aspect_ratio.rs
// (`given_fully_solid_placeholder_run`, spec line 51 there / line 53 here,
// character-identical text); this scenario reuses that step def via
// cucumber's global registry
// (docs/patterns/anti/cucumber-step-regex-ambiguity.md). Re-registering it
// here would fail ONLY at runtime ("Step match is ambiguous"), with no
// compile-time signal — so it is intentionally absent from this file.

#[when(regex = r"^the layers are assessed$")]
fn when_the_layers_are_assessed(world: &mut UatWorld) {
    let resin = ResinBuilder::new().build();
    let printer = PrinterBuilder::new().build();
    let plate = PlateAdhesionProfile::default_textured();
    let supports = default_supports();

    // --- bottom-layer plate adhesion: real perimeter, crack vs no-crack ---
    const SIDE: u32 = 3;
    let thin = LayerMaskBuilder::thin_1xn(SIDE * SIDE, 1.0);
    let area =
        CrossSectionArea::new(thin.solid_area_mm2()).expect("mask area is non-negative");
    let bottom_layer: u32 = 0;
    let baseline = SupportAnalyzer::assess(
        bottom_layer,
        area,
        PeelForce::new(0.0).expect("0.0 N is a valid PeelForce"),
        &resin,
        &supports,
        &plate,
        CrackFront::no_crack(),
    );
    let cracked = SupportAnalyzer::assess(
        bottom_layer,
        area,
        PeelForce::new(0.0).expect("0.0 N is a valid PeelForce"),
        &resin,
        &supports,
        &plate,
        CrackPropagator::crack_from_geometry(thin.solid_area_mm2(), thin.perimeter_mm()),
    );
    world.crack_interlayer_capacity_n = Some(vec![baseline.plate_capacity_n, cracked.plate_capacity_n]);

    // --- placeholder-mask layers: run_from_areas' synthetic 1×1 ... ---
    let areas: Vec<CrossSectionArea> = (0..=FIRST_NORMAL_LAYER)
        .map(|_| CrossSectionArea::new(thin.solid_area_mm2()).expect("valid area"))
        .collect();
    let sim_areas =
        SimulationRunner::run_from_areas(&areas, &resin, &printer, &supports, &plate, ambient(), None)
            .expect("scenario fixture: run_from_areas preconditions satisfied");
    assert!(
        !sim_areas
            .failures()
            .iter()
            .any(|f| f.failure_type == FailureType::Delamination),
        "fixture invariant: run_from_areas placeholder masks must never delaminate"
    );

    // --- ... and run_from_layer_inputs' W×H all-solid fallback (only the
    // first layer carries an explicit mask, so `prepare_layer_inputs`
    // synthesises the SAME 5×5 all-solid fallback for every other layer —
    // a genuinely non-1×1 W×H placeholder, distinct from run_from_areas). --
    let carrying_mask = LayerMaskBuilder::fully_solid(5, 5, 1.0);
    let carrying_area = carrying_mask.solid_area_mm2();
    let fallback_layers: Vec<LayerInput> = (0..=FIRST_NORMAL_LAYER)
        .map(|idx| {
            let li = LayerInput::new(idx, carrying_area, 2.5, 60.0, 50.0, idx as f32 * 0.05)
                .expect("valid layer input");
            if idx == 0 {
                li.with_mask(carrying_mask.clone())
            } else {
                li
            }
        })
        .collect();
    let sim_fallback = run(&fallback_layers, &supports);
    assert!(
        !sim_fallback
            .failures()
            .iter()
            .any(|f| f.failure_type == FailureType::Delamination),
        "fixture invariant: the run_from_layer_inputs W×H all-solid fallback must never \
         delaminate"
    );

    let mut placeholder_layers = sim_areas.layers().to_vec();
    placeholder_layers.extend(sim_fallback.layers().to_vec());
    world.crack_placeholder_layers = Some(placeholder_layers);
}

#[then(regex = r"^the bottom-layer plate adhesion is unchanged \(no crack\)$")]
fn then_bottom_layer_plate_adhesion_unchanged(world: &mut UatWorld) {
    let capacities = world
        .crack_interlayer_capacity_n
        .as_ref()
        .expect("scenario invariant: When step populated crack_interlayer_capacity_n");
    assert!(
        (capacities[0] - capacities[1]).abs() < 1e-3,
        "bottom-layer plate adhesion must be crack-invariant: no-crack={} cracked={}",
        capacities[0],
        capacities[1]
    );
}

#[then(
    regex = r"^every placeholder-mask layer records crack_front_fraction None \(no knockdown\)$"
)]
fn then_every_placeholder_layer_records_no_crack(world: &mut UatWorld) {
    let layers = world
        .crack_placeholder_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_placeholder_layers");
    assert!(
        layers.iter().all(|l| l.crack_front_fraction.is_none()),
        "every placeholder-mask layer must record crack_front_fraction None: {:?}",
        layers
            .iter()
            .map(|l| (l.index, l.crack_front_fraction))
            .collect::<Vec<_>>()
    );
}

#[then(regex = r"^no Delamination is emitted on those layers$")]
fn then_no_delamination_on_placeholder_layers(world: &mut UatWorld) {
    // Structural proof from the exact production gate
    // (`failure_predictor.rs`'s `if crack.value() > 0.0 && reduced_interlayer_n
    // < peel.value()`) and the field it populates in the same function
    // (`crack_front_fraction: (crack.value() > 0.0).then_some(...)`) — both
    // conditioned on the identical boolean, `crack.value() > 0.0`. The Then
    // above already proved that boolean false for every layer here, so the
    // Delamination arm could not have been reached for any of them. This is
    // independently double-checked against the real
    // `PrintSimulation::failures()` list as a fixture invariant inside the
    // When step above, for both placeholder runs.
    let layers = world
        .crack_placeholder_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_placeholder_layers");
    assert!(
        layers.iter().all(|l| l.crack_front_fraction.is_none()),
        "a Some(crack_front_fraction) layer here would be capable of delaminating; \
         none should exist"
    );
}

// ---- UAT-4: Delamination fires iff crack-reduced interlayer < peel load ---

#[given(regex = r"^a NORMAL layer with a crack front present \(crack_front_fraction > 0\)$")]
fn given_normal_layer_with_crack_front(_world: &mut UatWorld) {
    // Narrative — the two When steps below build DIFFERENT geometries (a
    // 0.5×100 mm wall for "fires", the equal-area mildly-thin mask from
    // UAT-1/UAT-2 for "still holds") because a single crack magnitude
    // cannot demonstrate both sides of the Delamination gate at once
    // (findings-a2-adversarial: "the still-holds branch has no existing
    // precedent to copy").
}

#[when(regex = r"^the crack-reduced interlayer capacity is below the shaped peel load$")]
fn when_capacity_below_peel_load(world: &mut UatWorld) {
    let wall = LayerMaskBuilder::thin_1xn(200, 0.5); // 0.5 mm × 100 mm wall —
    // reproduces tests/crack_propagation_runner.rs's Delamination-firing
    // geometry.
    let supports = no_supports();
    let sim = run(&layers_sharing_mask(&wall), &supports);
    let layers = sim.layers().to_vec();
    let failures = sim.failures().to_vec();

    // Boundary-side evidence via the SAME production assess() call the
    // Delamination gate itself reads — not merely the event's presence.
    let resin = ResinBuilder::new().build();
    let plate = PlateAdhesionProfile::default_textured();
    let layer = &layers[FIRST_NORMAL_LAYER as usize];
    let crack = CrackPropagator::crack_from_geometry(wall.solid_area_mm2(), wall.perimeter_mm());
    let assessment = SupportAnalyzer::assess(
        FIRST_NORMAL_LAYER,
        CrossSectionArea::new(wall.solid_area_mm2()).expect("mask area is non-negative"),
        PeelForce::new(layer.total_force_n).expect("LayerResult.total_force_n is non-negative"),
        &resin,
        &supports,
        &plate,
        crack,
    );

    world.crack_interlayer_capacity_n = Some(vec![assessment.plate_capacity_n]);
    world.crack_thin_layers = Some(layers);
    world.crack_failures_below = Some(failures);
}

#[then(
    regex = r"^a Delamination warning is emitted \(co-firing with SupportOverload if capacity is short\)$"
)]
fn then_delamination_emitted(world: &mut UatWorld) {
    let failures = world
        .crack_failures_below
        .as_ref()
        .expect("scenario invariant: When step populated crack_failures_below");
    assert!(
        failures
            .iter()
            .any(|f| f.failure_type == FailureType::Delamination),
        "expected a Delamination warning, got: {failures:?}"
    );
    // The no-supports fires-branch fixture also leaves total capacity short
    // of total load here, so SupportOverload genuinely co-fires — asserted
    // for real, not left as an unchecked "if".
    assert!(
        failures
            .iter()
            .any(|f| f.failure_type == FailureType::SupportOverload),
        "expected co-firing SupportOverload (capacity is short here), got: {failures:?}"
    );

    // Boundary-side evidence (findings-a2-adversarial, binding): the ACTUAL
    // reduced interlayer capacity vs the ACTUAL shaped peel load, from the
    // same production SupportAnalyzer::assess call the Delamination gate
    // itself reads.
    let reduced_interlayer_n = world
        .crack_interlayer_capacity_n
        .as_ref()
        .expect("scenario invariant: When step populated crack_interlayer_capacity_n")[0];
    let peel_n = world
        .crack_thin_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_thin_layers")
        [FIRST_NORMAL_LAYER as usize]
        .peel_force_n;
    assert!(
        reduced_interlayer_n < peel_n,
        "fires branch must be genuinely below the gate: reduced_interlayer_n=\
         {reduced_interlayer_n} peel_n={peel_n}"
    );
}

#[when(regex = r"^instead the crack-reduced interlayer capacity still exceeds the peel load$")]
fn when_capacity_still_exceeds_peel_load(world: &mut UatWorld) {
    const SIDE: u32 = 3;
    let mild = LayerMaskBuilder::thin_1xn(SIDE * SIDE, 1.0); // same mildly-thin
    // mask as UAT-1/UAT-2 (equal-area compact/thin pair) — crack ≈ 0.4.
    let supports = default_supports();
    let sim = run(&layers_sharing_mask(&mild), &supports);
    let layers = sim.layers().to_vec();
    let failures = sim.failures().to_vec();

    let resin = ResinBuilder::new().build();
    let plate = PlateAdhesionProfile::default_textured();
    let layer = &layers[FIRST_NORMAL_LAYER as usize];
    let crack = CrackPropagator::crack_from_geometry(mild.solid_area_mm2(), mild.perimeter_mm());
    let assessment = SupportAnalyzer::assess(
        FIRST_NORMAL_LAYER,
        CrossSectionArea::new(mild.solid_area_mm2()).expect("mask area is non-negative"),
        PeelForce::new(layer.total_force_n).expect("LayerResult.total_force_n is non-negative"),
        &resin,
        &supports,
        &plate,
        crack,
    );

    world.crack_interlayer_capacity_n = Some(vec![assessment.plate_capacity_n]);
    world.crack_compact_layers = Some(layers);
    world.crack_failures_above = Some(failures);
}

#[then(regex = r"^the crack is still recorded but NO Delamination is emitted$")]
fn then_crack_recorded_no_delamination(world: &mut UatWorld) {
    let layer = &world
        .crack_compact_layers
        .as_ref()
        .expect("scenario invariant: When step populated crack_compact_layers")
        [FIRST_NORMAL_LAYER as usize];
    let cf = layer
        .crack_front_fraction
        .expect("the crack must still be recorded (Some(>0))");
    assert!(cf > 0.0, "crack_front_fraction must be > 0, got {cf}");

    let failures = world
        .crack_failures_above
        .as_ref()
        .expect("scenario invariant: When step populated crack_failures_above");
    assert!(
        !failures
            .iter()
            .any(|f| f.failure_type == FailureType::Delamination),
        "expected NO Delamination, got: {failures:?}"
    );

    // Boundary-side evidence, same production call as the fires branch —
    // the actual reduced_interlayer_n must be strictly ABOVE the actual
    // peel load, not merely "no event happened to fire".
    let reduced_interlayer_n = world
        .crack_interlayer_capacity_n
        .as_ref()
        .expect("scenario invariant: When step populated crack_interlayer_capacity_n")[0];
    let peel_n = layer.peel_force_n;
    assert!(
        reduced_interlayer_n > peel_n,
        "still-holds branch must be genuinely above the gate: reduced_interlayer_n=\
         {reduced_interlayer_n} peel_n={peel_n}"
    );
}
