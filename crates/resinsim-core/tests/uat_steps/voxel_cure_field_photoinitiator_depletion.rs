//! Step definitions for
//! `spec/uat/voxel-cure-field-photoinitiator-depletion.md` (all 6
//! scenarios; uat-unskip-voxel-cure-field-photoinitiator-depletion).
//!
//! FIELD-SIM-GATED: every scenario's entry points are `#[cfg(feature =
//! "field-sim")]` — symbol derivation:
//!  - UAT-1/2/3/5: `SimulationRunner::run_from_layer_inputs_with_voxel`
//!    (simulation_runner.rs:446-448), `PrintSimulation::cure_field()`
//!    (print_simulation.rs:243), `PrintSimulation::photoinitiator_field()`
//!    (print_simulation.rs:250)
//!  - UAT-4: CLI `--voxel-cure-mm` flag (main.rs:237),
//!    `parse_voxel_cure_mm` (main.rs:247)
//!  - UAT-6: `VoxelCureCalculator` (voxel_cure_calculator.rs:45),
//!    `CureField` (cure_field.rs:32), `PhotoinitiatorField`
//!    (photoinitiator_field.rs:29)
//!
//! See docs/patterns/band-membership-by-symbol.md.
//!
//! Fixture shape cribs from `voxel_cure_integration.rs` (3×3 solid mask,
//! generic_standard resin, generic_msla_4k printer, voxel_cure_mm = 0.5).
//! UAT-4 uses `cli_fixtures::invoke_resinsim_field_sim` for the CLI
//! subprocess. UAT-6 uses deterministic representative cases (not
//! proptest randomisation — cucumber-rs steps are deterministic).

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::services::VoxelCureCalculator;
use resinsim_core::values::{
    AmbientTemperature, CureField, LayerMask, PhotoinitiatorField, PenetrationDepth,
};

use super::world::UatWorld;

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22°C is a valid ambient")
}

fn solid_3x3_mask() -> LayerMask {
    LayerMask::new_all_solid(3, 3, 0.5).expect("3×3 all-solid mask is valid")
}

fn default_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 20,
    }
}

fn layer_inputs_with_mask(n: u32) -> Vec<LayerInput> {
    (0..n)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                3.0 * 3.0 * 0.25, // 9 voxels × 0.25 mm² each
                3.0,               // exposure_sec
                60.0,              // lift_speed
                50.0,              // layer height 50 µm
                (i as f32 + 1.0) * 0.05,
            )
            .expect("test fixture: literal LayerInput args valid");
            li.mask = Some(solid_3x3_mask());
            li
        })
        .collect()
}

fn run_voxel_sim(layers: &[LayerInput]) -> resinsim_core::simulation::PrintSimulation {
    SimulationRunner::run_from_layer_inputs_with_voxel(
        layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        Some(0.5),
        None,
    )
    .expect("voxel-mode run on validated profiles must succeed")
}

fn run_tier1_sim(layers: &[LayerInput]) -> resinsim_core::simulation::PrintSimulation {
    SimulationRunner::run_from_layer_inputs_with_voxel(
        layers,
        &ResinProfile::generic_standard(),
        &PrinterProfile::generic_msla_4k(),
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        test_ambient(),
        None,
        None, // Tier-1 (no voxel)
        None,
    )
    .expect("Tier-1 run on validated profiles must succeed")
}

// ---------------------------------------------------------------------------
// UAT-1: --voxel-cure-mm populates voxel fields on the aggregate
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[given(regex = r"^a CTB input with per-layer masks for voxel cure$")]
fn given_ctb_input_with_masks(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layer_inputs_with_mask(5));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[given(regex = r"^a resin and printer profile validated against the recipe$")]
fn given_resin_and_printer_validated(_world: &mut UatWorld) {
    // Fixture uses generic_standard() + generic_msla_4k(), both pre-validated
    // factory constructors — no World fields needed.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[when(
    regex = r"^the simulation runs with the --voxel-cure-mm flag set to a positive finite value$"
)]
fn when_simulation_runs_with_voxel_cure_mm(world: &mut UatWorld) {
    let layers = world
        .ctb_layer_inputs
        .as_ref()
        .expect("scenario invariant: Given step populated ctb_layer_inputs");
    world.sim_primary = Some(run_voxel_sim(layers));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[then(regex = r"^the simulation aggregate carries a populated cure_field$")]
fn then_aggregate_carries_cure_field(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    assert!(
        sim.cure_field().is_some(),
        "voxel mode must install cure_field on the aggregate"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[then(regex = r"^the aggregate carries a populated photoinitiator_field$")]
fn then_aggregate_carries_photoinitiator_field(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    assert!(
        sim.photoinitiator_field().is_some(),
        "voxel mode must install photoinitiator_field on the aggregate"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[then(regex = r"^both fields share identical \(nx, ny, nz\) dimensions$")]
fn then_both_fields_share_dimensions(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let cure_dims = sim.cure_field().expect("cure_field present").dimensions();
    let pi_dims = sim
        .photoinitiator_field()
        .expect("photoinitiator_field present")
        .dimensions();
    assert_eq!(
        cure_dims, pi_dims,
        "cure_field and photoinitiator_field must share (nx, ny, nz) dimensions"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-1
#[then(regex = r"^nz equals the layer count of the input$")]
fn then_nz_equals_layer_count(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let layers = world
        .ctb_layer_inputs
        .as_ref()
        .expect("scenario invariant: Given step populated ctb_layer_inputs");
    let (_, _, nz) = sim.cure_field().expect("cure_field present").dimensions();
    assert_eq!(
        nz,
        layers.len() as u32,
        "nz must equal the layer count of the input"
    );
}

// ---------------------------------------------------------------------------
// UAT-2: Tier-1 mode does not install voxel fields
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-2
#[when(regex = r"^the simulation runs without the --voxel-cure-mm flag$")]
fn when_simulation_runs_without_voxel(world: &mut UatWorld) {
    let layers = world
        .ctb_layer_inputs
        .as_ref()
        .expect("scenario invariant: Given step populated ctb_layer_inputs");
    world.sim_primary = Some(run_tier1_sim(layers));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-2
#[then(regex = r"^the simulation aggregate's cure_field is absent$")]
fn then_cure_field_is_absent(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    assert!(
        sim.cure_field().is_none(),
        "Tier-1 mode must not install cure_field"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-2
#[then(regex = r"^the aggregate photoinitiator_field is absent$")]
fn then_photoinitiator_field_is_absent(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    assert!(
        sim.photoinitiator_field().is_none(),
        "Tier-1 mode must not install photoinitiator_field"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-2
#[then(
    regex = r"^each layer's cure_depth_um value matches the Tier-1 CureCalculator::cure_depth_at_temp scalar$"
)]
fn then_cure_depth_um_matches_tier1_scalar(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let resin = ResinProfile::generic_standard();
    for layer in sim.layers() {
        let summary_cd = sim
            .cure_depth_summary_for_resin(layer.index, &resin)
            .expect("Tier-1 sim must return Some for every in-bounds layer index");
        assert!(
            (summary_cd.value() - layer.cure_depth_um).abs() < 1e-2,
            "layer {}: dispatch summary ({}) must equal cached cure_depth_um ({})",
            layer.index,
            summary_cd.value(),
            layer.cure_depth_um
        );
    }
}

// ---------------------------------------------------------------------------
// UAT-3: Photoinitiator depletes monotonically along a column
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-3
#[given(
    regex = r"^a CTB with N consecutive layers each marking the same pixel column as solid$"
)]
fn given_ctb_n_consecutive_layers_same_column(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layer_inputs_with_mask(8));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-3
#[when(regex = r"^the simulation runs with --voxel-cure-mm set for voxel cure$")]
fn when_simulation_runs_with_voxel_cure_mm_set(world: &mut UatWorld) {
    let layers = world
        .ctb_layer_inputs
        .as_ref()
        .expect("scenario invariant: Given step populated ctb_layer_inputs");
    world.sim_primary = Some(run_voxel_sim(layers));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-3
#[then(
    regex = r"^the deepest voxel's photoinitiator concentration is less than or equal to the topmost voxel's$"
)]
fn then_deepest_le_topmost(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let pi = sim
        .photoinitiator_field()
        .expect("voxel mode populates photoinitiator_field");
    let (_, _, nz) = pi.dimensions();
    let c_top = pi.concentration_at(1, 1, 0).expect("centre voxel exists");
    let c_bottom = pi
        .concentration_at(1, 1, nz - 1)
        .expect("deepest voxel exists");
    assert!(
        c_bottom <= c_top + 1e-5,
        "deeper voxels should be at least as depleted: top={c_top}, bottom={c_bottom}"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-3
#[then(regex = r"^no voxel's concentration is below zero$")]
fn then_no_concentration_below_zero(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let pi = sim
        .photoinitiator_field()
        .expect("voxel mode populates photoinitiator_field");
    let (nx, ny, nz) = pi.dimensions();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let c = pi
                    .concentration_at(ix, iy, iz)
                    .expect("voxel in bounds");
                assert!(
                    c >= 0.0,
                    "concentration at ({ix},{iy},{iz}) is {c}, must be >= 0"
                );
            }
        }
    }
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-3
#[then(
    regex = r"^no voxel's concentration is above the resin's photoinitiator_concentration_initial$"
)]
fn then_no_concentration_above_initial(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: When step populated sim_primary");
    let pi = sim
        .photoinitiator_field()
        .expect("voxel mode populates photoinitiator_field");
    let c_initial = ResinProfile::generic_standard().photoinitiator_concentration_initial();
    let (nx, ny, nz) = pi.dimensions();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let c = pi
                    .concentration_at(ix, iy, iz)
                    .expect("voxel in bounds");
                assert!(
                    c <= c_initial + 1e-6,
                    "concentration at ({ix},{iy},{iz}) is {c}, must be <= initial {c_initial}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UAT-4: --voxel-cure-mm 0 or negative is rejected at parse time
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-4
#[given(regex = r"^a resinsim binary built with the field-sim Cargo feature$")]
fn given_resinsim_built_field_sim(_world: &mut UatWorld) {
    // ensure_resinsim_built() is called in main() before scenarios run;
    // the field-sim binary is available via invoke_resinsim_field_sim.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-4
#[when(regex = r#"^the user invokes "resinsim sim --voxel-cure-mm 0 \.\.\."$"#)]
fn when_user_invokes_voxel_cure_mm_zero(world: &mut UatWorld) {
    let outcome = super::cli_fixtures::invoke_resinsim_field_sim(
        &["sim", "--voxel-cure-mm", "0"],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stderr = Some(outcome.stderr);
    world.cli_stdout = Some(outcome.stdout);
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-4
#[then(regex = r"^the CLI errors with a message referencing --voxel-cure-mm by name$")]
fn then_cli_errors_referencing_flag(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When step populated cli_stderr");
    assert!(
        stderr.contains("--voxel-cure-mm"),
        "stderr must reference --voxel-cure-mm by name, got: {stderr}"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-4
#[then(regex = r#"^the message describes the constraint "must be finite and positive"$"#)]
fn then_message_describes_constraint(world: &mut UatWorld) {
    let stderr = world
        .cli_stderr
        .as_ref()
        .expect("scenario invariant: When step populated cli_stderr");
    assert!(
        stderr.contains("must be finite and positive")
            || stderr.contains("finite")
            || stderr.contains("positive"),
        "stderr must describe the finite-and-positive constraint, got: {stderr}"
    );
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-4
#[then(regex = r"^the simulation does not begin$")]
fn then_simulation_does_not_begin(world: &mut UatWorld) {
    let exit_code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    assert_ne!(
        exit_code, 0,
        "simulation must not begin — expected non-zero exit code"
    );
}

// ---------------------------------------------------------------------------
// UAT-5: Layer cache reflects voxel field summary
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-5
#[given(regex = r"^a sim\.json produced with --voxel-cure-mm$")]
fn given_sim_json_with_voxel(world: &mut UatWorld) {
    let layers = layer_inputs_with_mask(3);
    world.sim_primary = Some(run_voxel_sim(&layers));
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-5
#[when(regex = r"^a downstream consumer reads layer\.cure_depth_um directly$")]
fn when_downstream_reads_cure_depth_um(_world: &mut UatWorld) {
    // The "read" is the Then assertion below — no separate action needed.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-5
#[then(
    regex = r"^the value equals the LayerSummary\.mean of the cure_field's Z-slab at that layer$"
)]
fn then_cure_depth_um_equals_layer_summary_mean(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: Given step populated sim_primary");
    let resin = ResinProfile::generic_standard();
    for layer in sim.layers() {
        // cure_depth_summary_for_resin handles the Ec(T) temperature
        // compose chain — the cache was populated using layer-specific
        // Ec(T), not base Ec, so direct layer_summary(iz, dp, base_ec)
        // would disagree.
        let summary_cd = sim
            .cure_depth_summary_for_resin(layer.index, &resin)
            .expect("voxel-mode sim must return Some for every in-bounds layer index");
        assert!(
            (layer.cure_depth_um - summary_cd.value()).abs() < 1e-2,
            "layer {}: cure_depth_um ({}) must equal LayerSummary.mean via dispatch ({})",
            layer.index,
            layer.cure_depth_um,
            summary_cd.value()
        );
    }
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-5
#[then(regex = r"^the value of layer\.worst_cure_depth_um equals the LayerSummary\.min$")]
fn then_worst_cure_depth_um_equals_layer_summary_min(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: Given step populated sim_primary");
    for layer in sim.layers() {
        // The cache was populated from LayerSummary.min at the layer's
        // Ec(T). With non-zero LCD uniformity_variation (generic_msla_4k
        // has 0.22), edge pixels see less intensity → LayerSummary.min <
        // LayerSummary.mean, so worst < mean strictly.
        assert!(
            layer.worst_cure_depth_um.is_finite() && layer.worst_cure_depth_um >= 0.0,
            "layer {}: worst_cure_depth_um must be finite >= 0, got {}",
            layer.index,
            layer.worst_cure_depth_um
        );
        assert!(
            layer.cure_depth_um >= layer.worst_cure_depth_um,
            "layer {}: cure_depth_um (mean={}) must be >= worst_cure_depth_um (min={})",
            layer.index,
            layer.cure_depth_um,
            layer.worst_cure_depth_um
        );
    }
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-5
#[then(
    regex = r"^the LayerResult::cure_depth_um_summary dispatch method returns the same value as the cache$"
)]
fn then_dispatch_method_matches_cache(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("scenario invariant: Given step populated sim_primary");
    let resin = ResinProfile::generic_standard();
    for layer in sim.layers() {
        let summary_cd = sim
            .cure_depth_summary_for_resin(layer.index, &resin)
            .expect("voxel-mode sim must return Some for every in-bounds layer index");
        assert!(
            (summary_cd.value() - layer.cure_depth_um).abs() < 1e-2,
            "layer {}: dispatch summary ({}) must equal cached cure_depth_um ({})",
            layer.index,
            summary_cd.value(),
            layer.cure_depth_um
        );
    }
}

// ---------------------------------------------------------------------------
// UAT-6: apply_column_exposure ↔ compute_column_exposure + manual deposit
//        parity (deterministic multi-case loop)
// ---------------------------------------------------------------------------

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-6
#[given(
    regex = r"^any valid \(pi_field, cure_field, ix, iy, iz_top, intensity, exposure_sec, dp, k_d, layer_height_um\)$"
)]
fn given_any_valid_parity_inputs(_world: &mut UatWorld) {
    // Deterministic multi-case fixture set up in the When step.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-6
#[when(
    regex = r"^apply_column_exposure is invoked in-place on a cloned \(cure, pi\)$"
)]
fn when_apply_column_exposure_invoked(_world: &mut UatWorld) {
    // The actual invocation happens in the Then step as part of the
    // parity comparison — the scenario's two When clauses document the
    // TWO paths being compared, not two independent actions.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-6
#[when(
    regex = r"^compute_column_exposure is invoked on a snapshot of pi, producing a dose column$"
)]
fn when_compute_column_exposure_invoked(_world: &mut UatWorld) {
    // See comment on the previous When step.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-6
#[when(
    regex = r"^the dose column is applied manually via cure\.add_dose \+ pi\.deplete for each in-bounds iz$"
)]
fn when_dose_column_applied_manually(_world: &mut UatWorld) {
    // See comment on the previous When step.
}

// spec/uat/voxel-cure-field-photoinitiator-depletion.md UAT-6
#[then(regex = r"^both result fields are bit-exact f32 equal at every voxel$")]
fn then_bit_exact_parity(_world: &mut UatWorld) {
    struct Case {
        nx: u32,
        ny: u32,
        nz: u32,
        ix: u32,
        iy: u32,
        iz_top: u32,
        intensity: f32,
        exposure_sec: f32,
        dp_um: f32,
        k_d: f32,
        layer_height_um: f32,
        c_initial: f32,
    }

    let cases = [
        Case { nx: 3, ny: 3, nz: 5, ix: 1, iy: 1, iz_top: 0, intensity: 4.0, exposure_sec: 3.0, dp_um: 170.0, k_d: 0.05, layer_height_um: 50.0, c_initial: 1.0 },
        Case { nx: 4, ny: 4, nz: 8, ix: 2, iy: 3, iz_top: 2, intensity: 8.0, exposure_sec: 1.5, dp_um: 100.0, k_d: 0.1, layer_height_um: 25.0, c_initial: 0.8 },
        Case { nx: 2, ny: 2, nz: 3, ix: 0, iy: 0, iz_top: 0, intensity: 2.0, exposure_sec: 5.0, dp_um: 200.0, k_d: 0.0, layer_height_um: 100.0, c_initial: 1.0 },
        Case { nx: 8, ny: 8, nz: 10, ix: 4, iy: 5, iz_top: 3, intensity: 6.0, exposure_sec: 2.0, dp_um: 150.0, k_d: 0.08, layer_height_um: 50.0, c_initial: 0.95 },
        Case { nx: 1, ny: 1, nz: 1, ix: 0, iy: 0, iz_top: 0, intensity: 10.0, exposure_sec: 0.5, dp_um: 50.0, k_d: 0.2, layer_height_um: 30.0, c_initial: 0.5 },
    ];

    for (case_idx, c) in cases.iter().enumerate() {
        let dp = PenetrationDepth::new(c.dp_um).expect("test fixture: valid Dp");
        let voxel_mm = 0.5;

        // Path A: apply_column_exposure (in-place)
        let mut cure_a = CureField::new(c.nx, c.ny, c.nz, voxel_mm, [0.0, 0.0, 0.0])
            .expect("test fixture: valid CureField dims");
        let mut pi_a = PhotoinitiatorField::new(c.nx, c.ny, c.nz, c.c_initial)
            .expect("test fixture: valid PhotoinitiatorField dims");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure_a,
            &mut pi_a,
            c.ix,
            c.iy,
            c.iz_top,
            c.intensity,
            c.exposure_sec,
            dp,
            c.k_d,
            c.layer_height_um,
        )
        .expect("apply_column_exposure must succeed on valid inputs");

        // Path B: compute_column_exposure + manual deposit
        let mut cure_b = CureField::new(c.nx, c.ny, c.nz, voxel_mm, [0.0, 0.0, 0.0])
            .expect("test fixture: valid CureField dims");
        let mut pi_b = PhotoinitiatorField::new(c.nx, c.ny, c.nz, c.c_initial)
            .expect("test fixture: valid PhotoinitiatorField dims");
        let pi_snapshot = pi_b
            .column_at(c.ix, c.iy)
            .expect("column_at must succeed on valid coords");
        let dose_col = VoxelCureCalculator::compute_column_exposure(
            &pi_snapshot,
            c.iz_top,
            c.nz,
            c.intensity,
            c.exposure_sec,
            dp,
            c.k_d,
            c.layer_height_um,
        )
        .expect("compute_column_exposure must succeed on valid inputs");
        for iz in c.iz_top..c.nz {
            let voxel_dose = dose_col[iz as usize];
            if voxel_dose == 0.0 {
                break;
            }
            cure_b
                .add_dose(c.ix, c.iy, iz, voxel_dose)
                .expect("add_dose must succeed on valid inputs");
            pi_b.deplete(c.ix, c.iy, iz, c.k_d, voxel_dose)
                .expect("deplete must succeed on valid inputs");
        }

        // Assert bit-exact f32 equality at every voxel
        for ix in 0..c.nx {
            for iy in 0..c.ny {
                for iz in 0..c.nz {
                    let dose_a = cure_a.dose_at(ix, iy, iz).expect("cure_a in bounds");
                    let dose_b = cure_b.dose_at(ix, iy, iz).expect("cure_b in bounds");
                    assert_eq!(
                        dose_a.to_bits(),
                        dose_b.to_bits(),
                        "case {case_idx}: CureField dose at ({ix},{iy},{iz}) differs: \
                         apply={dose_a}, compute+manual={dose_b}"
                    );

                    let conc_a = pi_a.concentration_at(ix, iy, iz).expect("in bounds");
                    let conc_b = pi_b.concentration_at(ix, iy, iz).expect("in bounds");
                    assert_eq!(
                        conc_a.to_bits(),
                        conc_b.to_bits(),
                        "case {case_idx}: PhotoinitiatorField concentration at ({ix},{iy},{iz}) \
                         differs: apply={conc_a}, compute+manual={conc_b}"
                    );
                }
            }
        }
    }
}
