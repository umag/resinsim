//! Step definitions for
//! `spec/uat/honest-zero-yield-fraction-on-calibrated-solid.md` (both
//! scenarios; uat-unskip-band-d step 6). FIELD-SIM-GATED: both scenarios'
//! sole entry point is `SimulationRunner::run_from_layer_inputs_with_voxel`
//! (`#[cfg(feature = "field-sim")]`, `simulation_runner.rs`), so this
//! module compiles only under `cargo uat-field-sim` — its `pub mod` line
//! in `uat_steps/mod.rs` carries the matching
//! `#[cfg(feature = "field-sim")]` attribute, and its `use` entry lives in
//! the SECOND, separately-gated `use uat_steps::{...}` block in this
//! file's sibling `uat_gherkin.rs` (rustc rejects `#[cfg]` on an
//! identifier inside a braced `use` group — proven by this issue's step-1
//! scratch probe). See docs/patterns/band-membership-by-symbol.md.
//!
//! Fixture shape cribbed from the nextest coverage the spec's own
//! Rationale cites: `voxel_strain_stress_integration.rs::
//! honest_zero_yield_fraction_on_generic_standard_solid` +
//! `::nonzero_strain_magnitude_on_generic_standard_solid` — same 4-layer
//! 3×3 solid_mask geometry, `ResinProfile::generic_standard()`,
//! `voxel_cure_mm = 0.5`. `LayerResult::voxel_yield_fraction` /
//! `::strain_magnitude_max` are themselves NOT feature-gated (plain
//! `Option<f32>` fields on an ungated struct — only their POPULATION path
//! is gated), so `UatWorld::sim_primary` (an ungated `PrintSimulation`
//! field already on the shared World) is reused to hold the result rather
//! than adding a new gated field.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

use super::world::UatWorld;

fn solid_mask_3x3() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3x3 solid mask at 0.5mm voxel is valid")
}

fn four_layer_3x3_layer_inputs() -> Vec<LayerInput> {
    (0..4u32)
        .map(|i| {
            let area = 3.0 * 3.0 * 0.25;
            let mut li = LayerInput::new(i, area, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("LayerInput precondition satisfied by literal fixture values");
            li.mask = Some(solid_mask_3x3());
            li
        })
        .collect()
}

#[given(regex = r"^a ResinProfile generic_standard \(E = 2000, ν = 0\.35, z_ratio = 1\.5\)$")]
fn given_resin_generic_standard(world: &mut UatWorld) {
    world.resin = Some(ResinProfile::generic_standard());
}

#[given(regex = r"^a 4-layer 3×3 solid_mask geometry$")]
fn given_4_layer_3x3_solid_mask(_world: &mut UatWorld) {
    // The fixture is a pure function of the Given text
    // (`four_layer_3x3_layer_inputs`, consumed directly by the shared When
    // step below) — no World field needed, matching the cribbed nextest
    // fixture exactly.
}

#[given(regex = r"^voxel-mode \(--voxel-cure-mm = 0\.5\)$")]
fn given_voxel_mode(_world: &mut UatWorld) {
    // Same rationale as the layer-geometry Given above — the 0.5 mm voxel
    // size is a literal consumed directly by the shared When step.
}

#[when(regex = r"^the SimulationRunner runs to completion$")]
fn when_simulation_runner_runs_to_completion(world: &mut UatWorld) {
    let resin = world
        .resin
        .clone()
        .expect("scenario invariant: Given step populated resin");
    let printer = PrinterProfile::generic_msla_4k();
    let layers = four_layer_3x3_layer_inputs();
    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22°C is in AmbientTemperature domain"),
        None,
        Some(0.5),
    )
    .expect("voxel-mode run on validated profiles must succeed");
    world.sim_primary = Some(sim);
}

#[then(regex = r"^every layer's voxel_yield_fraction is Some\(0\.0\)$")]
fn then_every_layer_yield_fraction_zero(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    assert!(
        !sim.layers().is_empty(),
        "voxel-mode sim must produce layer results"
    );
    for layer in sim.layers() {
        // Strict Some(0.0), not a tolerance — mirrors the nextest guard's
        // rationale (yield_fraction computes exact zeros via an early
        // return, no rounding enters).
        assert_eq!(
            layer.voxel_yield_fraction,
            Some(0.0),
            "layer {idx}: expected Some(0.0) voxel_yield_fraction on the calibrated generic profile",
            idx = layer.index
        );
    }
}

#[then(regex = r"^at least one layer has strain_magnitude_max > 0\.0$")]
fn then_at_least_one_layer_strain_nonzero(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let any_nonzero = sim
        .layers()
        .iter()
        .any(|l| matches!(l.strain_magnitude_max, Some(m) if m > 0.0));
    assert!(
        any_nonzero,
        "expected at least one layer with strain_magnitude_max > 0.0 — \
         magnitude collapse (e.g. a unit error) would zero this out"
    );
}
