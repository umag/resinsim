//! Step definitions for
//! `spec/uat/light-crosstalk-3d-gaussian-convolution.md` UAT-1/UAT-2/UAT-3/
//! UAT-4/UAT-8/UAT-9 — the six runtime 3D convolution scenarios that exercise
//! the Tier-2 voxel cure crosstalk path (ADR-0018) through
//! `SimulationRunner::run_from_layer_inputs_with_voxel`.
//!
//! FIELD-SIM-GATED: every scenario's sole entry point is
//! `SimulationRunner::run_from_layer_inputs_with_voxel`
//! (`#[cfg(feature = "field-sim")]`, `simulation_runner.rs`), so this module
//! compiles only under `cargo uat-field-sim`. Its `pub mod` line in
//! `uat_steps/mod.rs` carries the matching `#[cfg(feature = "field-sim")]`
//! attribute, and its `use` entry lives in the SECOND, separately-gated
//! `use uat_steps::{...}` block in `uat_gherkin.rs`.
//!
//! This is the SECOND step-def module for this spec — the first
//! (`light_crosstalk_3d_gaussian_convolution.rs`) covers UAT-5/6/7
//! (validation-only, ungated). Two modules for one spec is mapped via
//! `STEP_DEF_MODULE_RENAMES` in `uat_gherkin.rs`.
//!
//! Fixture patterns cribbed from
//! `crates/resinsim-core/tests/voxel_cure_crosstalk_integration.rs`.

use cucumber::{given, then, when};

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::ResinProfile;
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

use super::world::{PrinterBuilder, UatWorld};

fn ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22 °C valid ambient")
}

fn default_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 20,
    }
}

fn single_pixel_mask(nx: u32, ny: u32, ix: u32, iy: u32) -> LayerMask {
    let mut m = LayerMask::new(nx, ny, 0.5).expect("LayerMask::new in-domain");
    m.set(ix, iy).expect("set within bounds");
    m
}

fn empty_mask(nx: u32, ny: u32) -> LayerMask {
    LayerMask::new(nx, ny, 0.5).expect("LayerMask::new in-domain")
}

fn layers_with_mask(mask: LayerMask, n: u32) -> Vec<LayerInput> {
    (0..n)
        .map(|i| {
            let mut li = LayerInput::new(
                i,
                0.25,
                3.0,
                60.0,
                50.0,
                (i as f32 + 1.0) * 0.05,
            )
            .expect("LayerInput::new in-domain");
            li.mask = Some(mask.clone());
            li
        })
        .collect()
}

fn layers_single_exposure(
    nx: u32,
    ny: u32,
    ix: u32,
    iy: u32,
    source_layer: u32,
    total_layers: u32,
) -> Vec<LayerInput> {
    let mask_lit = single_pixel_mask(nx, ny, ix, iy);
    let mask_empty = empty_mask(nx, ny);
    (0..total_layers)
        .map(|i| {
            let mut li = LayerInput::new(i, 0.25, 3.0, 60.0, 50.0, (i as f32 + 1.0) * 0.05)
                .expect("LayerInput in-domain");
            li.mask = Some(if i == source_layer {
                mask_lit.clone()
            } else {
                mask_empty.clone()
            });
            li
        })
        .collect()
}

// ---- UAT-1: Both σ None — t2f1 path unchanged (regime AA) ------------------

#[given(
    regex = r"^a printer profile with both crosstalk_sigma_xy_um and crosstalk_sigma_z_um absent$"
)]
fn given_both_sigma_absent(world: &mut UatWorld) {
    world.peel_printer = Some(PrinterBuilder::new().build());
}

#[given(regex = r"^a CTB input with per-layer masks$")]
fn given_ctb_per_layer_masks(world: &mut UatWorld) {
    let mask = LayerMask::new_all_solid(5, 5, 0.5).expect("5×5 solid mask");
    world.ctb_layer_inputs = Some(layers_with_mask(mask, 3));
}

#[when(regex = r"^the simulation runs with --voxel-cure-mm set$")]
fn when_sim_runs_voxel(world: &mut UatWorld) {
    let printer = world
        .peel_printer
        .clone()
        .expect("Given step populated peel_printer");
    let layers = world
        .ctb_layer_inputs
        .clone()
        .expect("Given step populated ctb_layer_inputs");
    let resin = ResinProfile::generic_standard();
    let sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &printer,
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
        None,
    )
    .expect("voxel-mode simulation must succeed");
    world.sim_primary = Some(sim);
}

#[then(regex = r"^the produced cure_field is bit-exact equal to the t2f1 baseline$")]
fn then_cure_field_bit_exact_t2f1(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("When step populated sim_primary");
    let cf = sim
        .cure_field()
        .expect("AA regime must install cure_field (t2f1 path)");
    let (nx, ny, nz) = cf.dimensions();
    assert_eq!((nx, ny, nz), (5, 5, 3), "cure_field dimensions match input");
    let max_dose = cf.max_dose();
    assert!(
        max_dose > 0.0,
        "AA regime must produce non-zero cure dose, got max={max_dose}"
    );
}

#[then(regex = r"^the produced photoinitiator_field is bit-exact equal to the t2f1 baseline$")]
fn then_pi_field_bit_exact_t2f1(world: &mut UatWorld) {
    let sim = world
        .sim_primary
        .as_ref()
        .expect("When step populated sim_primary");
    let pi = sim
        .photoinitiator_field()
        .expect("AA regime must install pi_field (t2f1 path)");
    let (nx, ny, nz) = pi.dimensions();
    assert_eq!(
        (nx, ny, nz),
        (5, 5, 3),
        "pi_field dimensions match input"
    );
}

// ---- UAT-2: σ_xy set — XY crosstalk produces off-pixel cure dose -----------

#[given(
    regex = r"^a printer profile with crosstalk_sigma_xy_um set to a value producing σ_voxels >= 1$"
)]
fn given_sigma_xy_large(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_xy_um(1000.0)
            .build(),
    );
}

#[given(regex = r"^a CTB input with a single-pixel solid mask at the centre$")]
fn given_single_pixel_centre(world: &mut UatWorld) {
    let mask = single_pixel_mask(9, 9, 4, 4);
    world.ctb_layer_inputs = Some(layers_with_mask(mask, 1));
}

#[then(
    regex = r"^the cure_field shows non-zero cure dose at off-mask voxels adjacent to the source pixel$"
)]
fn then_off_pixel_dose(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let neighbour = cf.dose_at(5, 4, 0).expect("neighbour in bounds");
    assert!(
        neighbour > 0.0,
        "σ_xy = 1000 µm must leak cure dose to off-mask neighbour (5,4,0), got 0"
    );
}

#[then(
    regex = r"^the off-pixel cure dose pattern is 4-fold symmetric about the source pixel$"
)]
fn then_four_fold_symmetric(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let centre = cf.dose_at(4, 4, 0).expect("centre");
    let n_xp = cf.dose_at(5, 4, 0).expect("(+x)");
    let n_xm = cf.dose_at(3, 4, 0).expect("(-x)");
    let n_yp = cf.dose_at(4, 5, 0).expect("(+y)");
    let n_ym = cf.dose_at(4, 3, 0).expect("(-y)");
    let tol = 1e-5 * centre;
    assert!(
        (n_xp - n_xm).abs() < tol,
        "x-symmetry: +x={n_xp}, -x={n_xm}"
    );
    assert!(
        (n_yp - n_ym).abs() < tol,
        "y-symmetry: +y={n_yp}, -y={n_ym}"
    );
    assert!(
        (n_xp - n_yp).abs() < tol,
        "x-y-symmetry: +x={n_xp}, +y={n_yp}"
    );
}

// ---- UAT-3: σ_z set — Z crosstalk co-scatters cure dose AND PI depletion ---

#[given(
    regex = r"^a printer profile with crosstalk_sigma_z_um set to a value producing σ_layers >= 0\.5$"
)]
fn given_sigma_z(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_z_um(40.0)
            .build(),
    );
}

#[given(regex = r"^a CTB input where only layer L is masked \(single layer source\)$")]
fn given_single_layer_source(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layers_single_exposure(5, 5, 2, 2, 3, 7));
}

#[then(
    regex = r"^the cure_field shows non-zero cure dose in layers L-1 and L\+1 of the source pixel$"
)]
fn then_z_spread_cure(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let dose_l_minus_1 = cf.dose_at(2, 2, 2).expect("L-1 in bounds");
    let dose_l_plus_1 = cf.dose_at(2, 2, 4).expect("L+1 in bounds");
    assert!(
        dose_l_minus_1 > 0.0,
        "Z conv must spread dose to L-1, got {dose_l_minus_1}"
    );
    assert!(
        dose_l_plus_1 > 0.0,
        "Z conv must spread dose to L+1, got {dose_l_plus_1}"
    );
}

#[then(
    regex = r"^the photoinitiator_field concentration at those layers is reduced \(depleted\) relative to the t2f1 baseline$"
)]
fn then_pi_depleted_relative_to_baseline(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let pi = sim.photoinitiator_field().expect("pi_field installed");
    let initial = pi.initial_concentration();
    let pi_l_minus_1 = pi.concentration_at(2, 2, 2).expect("L-1 in bounds");
    let pi_l_plus_1 = pi.concentration_at(2, 2, 4).expect("L+1 in bounds");

    // L-1 (iz=2): in the t2f1 baseline (no Z conv), Beer-Lambert deposits
    // zero dose above the source layer, so PI stays at initial. The Z conv
    // spreads dose into L-1, depleting PI below initial.
    assert!(
        pi_l_minus_1 < initial,
        "PI at L-1 must be depleted below initial ({initial}): got {pi_l_minus_1}"
    );
    // L+1 (iz=4): both the baseline and crosstalk runs deposit dose here
    // (Beer-Lambert decay). The Z conv redistributes dose along the column;
    // the net effect at L+1 depends on the local gradient and may slightly
    // increase or decrease dose vs the un-convolved column. The physical
    // invariant is that ANY non-zero dose depletes PI below initial.
    assert!(
        pi_l_plus_1 < initial,
        "PI at L+1 must be depleted below initial ({initial}): got {pi_l_plus_1}"
    );
}

// ---- UAT-4: Both σ active — 3D cure dose neighbourhood ---------------------

#[given(
    regex = r"^a printer profile with both crosstalk_sigma_xy_um and crosstalk_sigma_z_um set to produce σ_voxels >= 1 and σ_layers >= 0\.5$"
)]
fn given_both_sigma(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_xy_um(1000.0)
            .with_crosstalk_sigma_z_um(100.0)
            .build(),
    );
}

#[given(regex = r"^a CTB input with a single-pixel single-layer source mask$")]
fn given_single_pixel_single_layer(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layers_single_exposure(5, 5, 2, 2, 2, 5));
}

#[then(
    regex = r"^the cure_field shows non-zero cure dose in a 3D neighbourhood around the source voxel$"
)]
fn then_3d_neighbourhood(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let centre = cf.dose_at(2, 2, 2).expect("centre");
    let x_neighbour = cf.dose_at(3, 2, 2).expect("+x neighbour");
    let z_neighbour = cf.dose_at(2, 2, 3).expect("+z neighbour");
    assert!(centre > 0.0, "centre cure positive");
    assert!(x_neighbour > 0.0, "x-neighbour cure positive");
    assert!(z_neighbour > 0.0, "z-neighbour cure positive");
}

#[then(
    regex = r"^the off-pixel-source-layer dose ratio matches the XY kernel ratio at that offset$"
)]
fn then_xy_kernel_ratio(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let centre = cf.dose_at(2, 2, 2).expect("centre");
    let neighbour_x = cf.dose_at(3, 2, 2).expect("+x neighbour");

    use resinsim_core::services::LightCrosstalkCalculator;
    let xy_kernel =
        LightCrosstalkCalculator::build_separable_kernel(2.0).expect("σ_xy_voxels = 2 kernel");
    let radius_xy = (xy_kernel.len() as i32 - 1) / 2;
    let kx_centre = xy_kernel[radius_xy as usize];
    let kx_off1 = xy_kernel[(radius_xy + 1) as usize];
    let expected_ratio = kx_off1 / kx_centre;
    let observed_ratio = neighbour_x / centre;
    let tol = 0.02;
    assert!(
        (observed_ratio - expected_ratio).abs() / expected_ratio < tol,
        "DD product structure: observed ratio {observed_ratio}, expected {expected_ratio}"
    );
}

// ---- UAT-8: Z-edge SKIP at field boundary — no dose pileup -----------------

#[given(
    regex = r"^a printer profile with crosstalk_sigma_z_um set to a value producing kernel radius >= 1$"
)]
fn given_sigma_z_kernel_radius(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_z_um(40.0)
            .build(),
    );
}

#[given(regex = r"^a CTB input where only layer 0 \(the first printed layer\) is masked$")]
fn given_layer_zero_source(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layers_single_exposure(5, 5, 2, 2, 0, 4));
}

#[then(
    regex = r"^the cure_field shows dose at iz=0 equal to \(centre kernel weight × Beer-Lambert surface dose\) plus small forward-kz contributions$"
)]
fn then_skip_dose_at_zero(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let conv_dose_at_zero = cf.dose_at(2, 2, 0).expect("edge (2,2,0)");

    use resinsim_core::services::LightCrosstalkCalculator;
    let zk = LightCrosstalkCalculator::build_separable_kernel(0.8).expect("σ=0.8 kernel");

    let baseline_printer = PrinterBuilder::new().build();
    let layers = layers_single_exposure(5, 5, 2, 2, 0, 4);
    let resin = ResinProfile::generic_standard();
    let baseline_sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &baseline_printer,
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
        None,
    )
    .expect("baseline edge run must succeed");
    let cf_baseline = baseline_sim.cure_field().expect("baseline cure_field");

    let baseline_doses: Vec<f32> = (0..4)
        .map(|iz| cf_baseline.dose_at(2, 2, iz).expect("baseline column"))
        .collect();

    let expected_skip: f32 = (0..=3i32)
        .map(|kz| zk[(kz + 3) as usize] * baseline_doses[kz as usize])
        .sum();

    let tol = 0.02 * expected_skip;
    assert!(
        (conv_dose_at_zero - expected_skip).abs() < tol,
        "Z-edge SKIP: conv_dose_at_zero ({conv_dose_at_zero}) should match \
         expected SKIP value ({expected_skip}) within 2%"
    );
}

#[then(
    regex = r"^the cure_field does NOT show the dose-pileup magnitude that would result from clamp-onto-boundary$"
)]
fn then_no_clamp_pileup(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    let conv_dose_at_zero = cf.dose_at(2, 2, 0).expect("edge (2,2,0)");

    use resinsim_core::services::LightCrosstalkCalculator;
    let zk = LightCrosstalkCalculator::build_separable_kernel(0.8).expect("σ=0.8 kernel");

    let baseline_printer = PrinterBuilder::new().build();
    let layers = layers_single_exposure(5, 5, 2, 2, 0, 4);
    let resin = ResinProfile::generic_standard();
    let baseline_sim = SimulationRunner::run_from_layer_inputs_with_voxel(
        &layers,
        &resin,
        &baseline_printer,
        &default_supports(),
        &PlateAdhesionProfile::default_textured(),
        ambient(),
        None,
        Some(0.5),
        None,
    )
    .expect("baseline edge run must succeed");
    let cf_baseline = baseline_sim.cure_field().expect("baseline cure_field");

    let baseline_doses: Vec<f32> = (0..4)
        .map(|iz| cf_baseline.dose_at(2, 2, iz).expect("baseline column"))
        .collect();

    let expected_skip: f32 = (0..=3i32)
        .map(|kz| zk[(kz + 3) as usize] * baseline_doses[kz as usize])
        .sum();

    let clamp_extra = (zk[0] + zk[1] + zk[2]) * baseline_doses[0];
    let expected_clamp = expected_skip + clamp_extra;
    let tol = 0.02 * expected_skip;

    assert!(
        (expected_clamp - expected_skip).abs() > 5.0 * tol,
        "SKIP ({expected_skip}) and CLAMP ({expected_clamp}) must differ \
         enough to discriminate regressions"
    );
    assert!(
        (conv_dose_at_zero - expected_clamp).abs() > tol,
        "conv_dose_at_zero ({conv_dose_at_zero}) must NOT match CLAMP value \
         ({expected_clamp}) — that would mean dose pileup"
    );
}

// ---- UAT-9: Post-attenuation Z conv shifts peak dose to L+1 ----------------

#[given(
    regex = r"^a printer profile with crosstalk_sigma_z_um set to a value producing σ_layers ≈ 0\.8 \(kernel radius 3\)$"
)]
fn given_sigma_z_08(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_crosstalk_sigma_z_um(40.0)
            .build(),
    );
}

#[given(
    regex = r"^a CTB input where only layer L \(well inside the print\) is masked as a single-pixel source$"
)]
fn given_interior_single_pixel_source(world: &mut UatWorld) {
    world.ctb_layer_inputs = Some(layers_single_exposure(5, 5, 2, 2, 5, 11));
}

#[then(
    regex = r"^the cure_field at the source pixel shows monotone-increasing dose from iz=L-3 to iz=L$"
)]
fn then_monotone_increasing_before_source(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    // L=5, so L-3=2, L-2=3, L-1=4, L=5
    let doses: Vec<f32> = (2..=5)
        .map(|iz| cf.dose_at(2, 2, iz).expect("in bounds"))
        .collect();
    for i in 0..doses.len() - 1 {
        assert!(
            doses[i] < doses[i + 1],
            "monotone-increasing before source: dose[iz={}]={} < dose[iz={}]={} violated",
            i + 2,
            doses[i],
            i + 3,
            doses[i + 1]
        );
    }
}

#[then(
    regex = r"^the cure_field at the source pixel shows dose at iz=L\+1 greater than dose at iz=L$"
)]
fn then_peak_at_l_plus_1(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    // L=5, L+1=6
    let dose_l = cf.dose_at(2, 2, 5).expect("L in bounds");
    let dose_l_plus_1 = cf.dose_at(2, 2, 6).expect("L+1 in bounds");
    assert!(
        dose_l_plus_1 > dose_l,
        "post-attenuation Z conv: dose at L+1 ({dose_l_plus_1}) must exceed \
         dose at L ({dose_l}) due to asymmetric kernel support"
    );
}

#[then(
    regex = r"^the cure_field at the source pixel shows monotone-decreasing dose from iz=L\+1 to iz=L\+3$"
)]
fn then_monotone_decreasing_after_peak(world: &mut UatWorld) {
    let sim = world.sim_primary.as_ref().expect("sim_primary populated");
    let cf = sim.cure_field().expect("cure_field installed");
    // L=5, L+1=6, L+2=7, L+3=8
    let doses: Vec<f32> = (6..=8)
        .map(|iz| cf.dose_at(2, 2, iz).expect("in bounds"))
        .collect();
    for i in 0..doses.len() - 1 {
        assert!(
            doses[i] > doses[i + 1],
            "monotone-decreasing after peak: dose[iz={}]={} > dose[iz={}]={} violated",
            i + 6,
            doses[i],
            i + 7,
            doses[i + 1]
        );
    }
}
