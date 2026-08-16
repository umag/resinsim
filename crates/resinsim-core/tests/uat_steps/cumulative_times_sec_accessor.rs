//! Step definitions for `spec/uat/cumulative-times-sec-accessor.md`
//! UAT-1..UAT-2 (uat-unskip-a3-b, plan step 4). The only in-process module in
//! this increment — independent of the nanodlp fixture band, so it lands
//! right after the lowest-risk athena module and before any nanodlp CLI
//! module.
//!
//! SYMBOL VERIFICATION. `PrintSimulation::cumulative_times_sec`
//! (print_simulation.rs:500) has no `#[cfg]` and delegates to
//! `services::LayerTimingCalculator::cumulative_times_sec` against the
//! aggregate's OWNED recipe + printer (encapsulation preserved — this
//! module never reaches inside `PrintSimulation`); `PrintSimulation::new`
//! and `app::SimulationRunner::run_from_layer_inputs` are likewise
//! `#[cfg]`-free on this call path (only the `_with_voxel` sibling and
//! `run_inner_full`'s internal voxel branches are field-sim-gated, and
//! neither is reached here).
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory: no
//! other module registers "the cumulative_times_sec accessor is called",
//! "the returned Vec has the same length as sim.layers()", "every value is
//! finite and non-negative", "the sequence is monotonic non-decreasing", or
//! "the returned Vec is empty". "the cumulative_times_sec accessor is
//! called" is CHARACTER-IDENTICAL between UAT-1 and UAT-2
//! (`base_adhesion_shifts_peel_peak.rs`'s shared-When precedent) and is
//! registered exactly ONCE below, serving both scenarios.
//!
//! HONEST-BENEFIT CAVEAT (record here, not papered over — same shape as A2's
//! interlayer module). `PrintSimulation::cumulative_times_sec` returns
//! `LayerTimingCalculator::cumulative_times_sec(&self.recipe, &self.printer,
//! self.layers.len() as u32)` — the returned Vec's length is `self.layers
//! .len()` BY CONSTRUCTION today, so UAT-1's length-parity Then cannot fail
//! against the current implementation. It is still the CONTRACT this spec
//! pins against a future refactor of the aggregate's internal recipe/printer
//! ownership, and the assertion reads the production Vec (never a
//! recomputed formula) — the value here is traceability + register shrink,
//! not new defect-finding power. Second: the finiteness Then depends on
//! `retract_speed_mm_min` being non-zero; `RecipeBuilder`'s TOML omits the
//! key and `Recipe` falls back to `lift_speed_mm_min` (60 mm/min) at read
//! time, so `t_retract` is finite — verified by this module actually
//! passing. If `every value is finite` ever fails in the future, investigate
//! the recipe/printer fixture rather than relaxing the assertion.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::simulation::PrintSimulation;

use super::fixtures::{default_plate, test_ambient, test_supports};
use super::world::{PrinterBuilder, RecipeBuilder, ResinBuilder, UatWorld};

/// UAT-1's fixture layer count — also the non-vacuity bound the length-
/// parity Then checks against (never just "lengths match", which would pass
/// on an empty aggregate too).
const LAYER_COUNT: u32 = 100;

/// 100 `LayerInput`s at a fixed 100 mm² cube cross-section — the
/// `ctb_layer_height_authority.rs` / `interlayer_crack_knockdown_scales_
/// with_perimeter.rs` idiom.
fn hundred_layer_cube_inputs() -> Vec<LayerInput> {
    (0..LAYER_COUNT)
        .map(|i| {
            LayerInput::new(i, 100.0, 2.5, 60.0, 50.0, (i + 1) as f32 * 0.05)
                .expect("test fixture: literal LayerInput args satisfy preconditions")
        })
        .collect()
}

// ---- UAT-1: cumulative_times_sec is parallel-indexed with layers() --------

#[given(
    regex = r"^a PrintSimulation built from a 100-layer cube via SimulationRunner::run_from_layer_inputs$"
)]
fn given_100_layer_cube_sim(world: &mut UatWorld) {
    let layers = hundred_layer_cube_inputs();
    let sim = SimulationRunner::run_from_layer_inputs(
        &layers,
        &ResinBuilder::new().build(),
        &PrinterBuilder::new().build(),
        &test_supports(),
        &default_plate(),
        test_ambient(),
        None,
        None,
    )
    .expect(
        "scenario fixture: ResinBuilder/PrinterBuilder output satisfies \
         run_from_layer_inputs preconditions",
    );
    world.cumulative_sim = Some(sim);
}

// ---- UAT-2: cumulative_times_sec is empty for an empty aggregate ---------

#[given(
    regex = r"^a PrintSimulation constructed via PrintSimulation::new with no layers added$"
)]
fn given_empty_print_simulation(world: &mut UatWorld) {
    let recipe = RecipeBuilder::new().build_standalone();
    let printer = PrinterBuilder::new().build();
    world.cumulative_sim = Some(PrintSimulation::new(recipe, printer));
}

// ---- shared When ------------------------------------------------------------

#[when(regex = r"^the cumulative_times_sec accessor is called$")]
fn when_cumulative_times_sec_called(world: &mut UatWorld) {
    let sim = world
        .cumulative_sim
        .as_ref()
        .expect("scenario invariant: Given step populated cumulative_sim");
    world.cumulative_times = Some(sim.cumulative_times_sec());
}

// ---- UAT-1 Then steps -------------------------------------------------------

#[then(regex = r"^the returned Vec has the same length as sim\.layers\(\)$")]
fn then_length_matches_layers(world: &mut UatWorld) {
    let times = world
        .cumulative_times
        .as_ref()
        .expect("scenario invariant: When step populated cumulative_times");
    let sim = world
        .cumulative_sim
        .as_ref()
        .expect("scenario invariant: Given step populated cumulative_sim");
    assert_eq!(
        times.len(),
        sim.layers().len(),
        "cumulative_times_sec() length must equal sim.layers().len()"
    );
    // Non-vacuity guard: prove this isn't an empty-vs-empty pass.
    assert_eq!(
        times.len(),
        LAYER_COUNT as usize,
        "UAT-1 fixture must produce exactly {LAYER_COUNT} layers"
    );
}

#[then(regex = r"^every value is finite and non-negative$")]
fn then_every_value_finite_nonnegative(world: &mut UatWorld) {
    let times = world
        .cumulative_times
        .as_ref()
        .expect("scenario invariant: When step populated cumulative_times");
    for (i, &v) in times.iter().enumerate() {
        assert!(
            v.is_finite() && v >= 0.0,
            "cumulative_times_sec()[{i}] must be finite and non-negative, got {v}"
        );
    }
}

#[then(regex = r"^the sequence is monotonic non-decreasing$")]
fn then_sequence_monotonic_non_decreasing(world: &mut UatWorld) {
    let times = world
        .cumulative_times
        .as_ref()
        .expect("scenario invariant: When step populated cumulative_times");
    // No epsilon slack: the accessor accumulates a running total, so exact
    // non-decrease is the real contract.
    for w in times.windows(2) {
        assert!(
            w[1] >= w[0],
            "cumulative_times_sec() must be monotonic non-decreasing: {} then {}",
            w[0],
            w[1]
        );
    }
}

// ---- UAT-2 Then step ---------------------------------------------------------

#[then(regex = r"^the returned Vec is empty$")]
fn then_returned_vec_is_empty(world: &mut UatWorld) {
    let times = world
        .cumulative_times
        .as_ref()
        .expect("scenario invariant: When step populated cumulative_times");
    assert!(
        times.is_empty(),
        "cumulative_times_sec() must be empty for a zero-layer aggregate, got {times:?}"
    );
}
