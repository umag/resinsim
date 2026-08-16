//! Step definitions for `spec/uat/profile-vacuum-pressure-scales-suction.md`
//! UAT-1..UAT-3 (uat-unskip-campaign increment 1, plan step 9).
//!
//! All three When texts ("the job is simulated" / "a job with a sealed
//! cavity is simulated" / "the profile is validated (factory or TOML
//! load)") are textually distinct from base_adhesion_shifts_peel_peak's
//! shared "a job is simulated with that resin" and from
//! peel_shape_factor_scales_with_aspect_ratio's "a job is simulated" (no
//! "with that resin") — checked directly against both sibling .md files,
//! not assumed. The closed-cup `LayerInput` fixture below is a direct
//! reconstruction of `SimulationRunner`'s own private test helper
//! `closed_cup_layer_inputs` (`src/app/simulation_runner.rs`, not
//! importable across the crate/integration-test boundary) — same 7×7 grid
//! @ 1 mm voxel, same 5-base/10-wall/1-cap layer counts, so the closure
//! layer (15) and interior area (25 mm²) match the shipped
//! `closed_cup_triggers_suction_warning` / `profile_vacuum_pressure_scales_suction`
//! nextest fixtures exactly — this UAT and that nextest module assert the
//! same production path from two different entry points, not two
//! different fixtures that happen to agree.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile, ATMOSPHERIC_PRESSURE_KPA};
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, LayerMask};

use super::world::{PrinterBuilder, UatWorld};

/// Layer index of the closed cup's closure (5 base + 10 wall = index 15,
/// 0-based) — see the module doc comment for the fixture this mirrors.
const CLOSURE_LAYER: usize = 15;

// ---- UAT-1: a per-printer ΔP scales the sealed-cavity suction linearly ----

#[given(regex = r"^a printer profile whose vacuum_pressure_kpa is 101 kPa$")]
fn given_vacuum_101(world: &mut UatWorld) {
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_vacuum_pressure_kpa(101.0)
            .build(),
    );
}

#[given(regex = r"^a job containing one sealed cavity of 25 mm² sealing area$")]
fn given_sealed_cavity_job(world: &mut UatWorld) {
    world.peel_layer_inputs = Some(closed_cup_layer_inputs());
}

#[when(regex = r"^the job is simulated$")]
fn when_the_job_is_simulated(world: &mut UatWorld) {
    let printer = world
        .peel_printer
        .clone()
        .expect("scenario invariant: Given step populated peel_printer");
    let layers = world
        .peel_layer_inputs
        .take()
        .expect("scenario invariant: Given step populated peel_layer_inputs");
    run_and_capture(world, &layers, &printer);
}

#[then(regex = r"^the cavity closure layer's suction_force_n equals 101 × 25 × 1e-3 = 2\.525 N$")]
fn then_suction_2_525(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    let suction = layers[CLOSURE_LAYER].suction_force_n;
    assert!(
        (suction - 2.525).abs() < 1e-2,
        "closure layer suction_force_n should be 2.525 N (101 kPa × 25 mm² × 1e-3); got {suction}"
    );
}

// ---- UAT-2: an unset ΔP is behaviour-preserving (50 kPa default) ----------

#[given(regex = r"^a printer profile whose vacuum_pressure_kpa is unset$")]
fn given_vacuum_unset(world: &mut UatWorld) {
    world.peel_printer = Some(PrinterBuilder::new().build());
}

#[when(regex = r"^a job with a sealed cavity is simulated$")]
fn when_job_with_sealed_cavity_simulated(world: &mut UatWorld) {
    let printer = world
        .peel_printer
        .clone()
        .expect("scenario invariant: Given step populated peel_printer");
    let layers = closed_cup_layer_inputs();
    run_and_capture(world, &layers, &printer);
}

#[then(regex = r"^effective_vacuum_pressure_kpa\(\) returns 50\.0$")]
fn then_effective_vacuum_50(world: &mut UatWorld) {
    let printer = world
        .peel_printer
        .as_ref()
        .expect("scenario invariant: Given step populated peel_printer");
    assert!(
        (printer.effective_vacuum_pressure_kpa() - 50.0).abs() < 1e-6,
        "unset vacuum_pressure_kpa must default to 50.0; got {}",
        printer.effective_vacuum_pressure_kpa(),
    );
}

#[then(
    regex = r"^every sealed-cavity suction_force_n equals 50 kPa × sealed_area × 1e-3 \(byte-identical to the pre-Stage-2 output\)$"
)]
fn then_suction_1_25_default(world: &mut UatWorld) {
    let layers = world
        .peel_sim_layers
        .as_ref()
        .expect("scenario invariant: When step populated peel_sim_layers");
    let suction = layers[CLOSURE_LAYER].suction_force_n;
    assert!(
        (suction - 1.25).abs() < 1e-2,
        "closure layer suction_force_n should be 1.25 N (50 kPa default × 25 mm² × 1e-3); got {suction}"
    );
}

// ---- UAT-3: ΔP is validated to not exceed atmospheric ---------------------

#[given(
    regex = r"^a printer profile whose vacuum_pressure_kpa exceeds 101\.325 kPa \(atmospheric\)$"
)]
fn given_vacuum_above_atmospheric(world: &mut UatWorld) {
    // Pins the scenario's literal primary case. The When step below also
    // covers zero/negative/NaN per the spec's own inline comment
    // ("0/negative/NaN also rejected") — one Gherkin scenario, four
    // sub-cases exercised in the step body, mirroring
    // cli_temperature_flag_validation.rs's UAT-3 "--ambient=-300 or NaN"
    // loop.
    world.peel_printer = Some(
        PrinterBuilder::new()
            .with_vacuum_pressure_kpa(ATMOSPHERIC_PRESSURE_KPA + 1.0)
            .build_unvalidated(),
    );
}

#[when(regex = r"^the profile is validated \(factory or TOML load\)$")]
fn when_profile_validated(world: &mut UatWorld) {
    let above_atmospheric = world
        .peel_printer
        .take()
        .expect("scenario invariant: Given step populated peel_printer");

    let mut messages = Vec::new();
    for (label, printer) in [
        ("above-atmospheric", above_atmospheric),
        (
            "zero",
            PrinterBuilder::new()
                .with_vacuum_pressure_kpa(0.0)
                .build_unvalidated(),
        ),
        (
            "negative",
            PrinterBuilder::new()
                .with_vacuum_pressure_kpa(-1.0)
                .build_unvalidated(),
        ),
        (
            "NaN",
            PrinterBuilder::new()
                .with_vacuum_pressure_kpa(f32::NAN)
                .build_unvalidated(),
        ),
    ] {
        match printer.validate() {
            Ok(()) => panic!(
                "scenario invariant violated: {label} vacuum_pressure_kpa unexpectedly \
                 passed validate()"
            ),
            Err(e) => messages.push(format!("{label}: {e}")),
        }
    }
    world.peel_validate_err = Some(messages.join(" | "));
}

#[then(regex = r"^validate\(\) returns an error naming vacuum_pressure_kpa$")]
fn then_validate_names_vacuum_pressure(world: &mut UatWorld) {
    let combined = world
        .peel_validate_err
        .as_deref()
        .expect("scenario invariant: When step populated peel_validate_err");
    for label in ["above-atmospheric", "zero", "negative", "NaN"] {
        assert!(
            combined.contains(label),
            "expected the {label} sub-case's error in the captured messages: {combined}"
        );
    }
    assert!(
        combined.matches("vacuum_pressure_kpa").count() >= 4,
        "every one of the 4 sub-case errors must name vacuum_pressure_kpa; got: {combined}"
    );
}

// ---- helpers ----------------------------------------------------------------

/// Reconstruction of `SimulationRunner`'s private `closed_cup_layer_inputs`
/// test helper — see the module doc comment. 7×7 grid @ 1 mm voxel: 5
/// solid base layers, 10 ring-wall layers (5×5 = 25 mm² interior void),
/// 1 solid cap layer. Closure event fires at index 15 (`CLOSURE_LAYER`).
fn closed_cup_layer_inputs() -> Vec<LayerInput> {
    const W: u32 = 7;
    const H: u32 = 7;
    const VOXEL_MM: f32 = 1.0;
    const EXPOSURE_SEC: f32 = 2.5;
    const LAYER_HEIGHT_UM: f32 = 50.0;
    const LIFT_SPEED_MM_MIN: f32 = 60.0;

    let solid_mask = LayerMask::new_all_solid(W, H, VOXEL_MM).expect("7×7 all-solid constructs");
    let ring_mask = {
        let mut m = LayerMask::new_all_solid(W, H, VOXEL_MM).expect("7×7 all-solid constructs");
        for x in 1..W - 1 {
            for y in 1..H - 1 {
                m.clear(x, y).expect("interior cell in bounds");
            }
        }
        m
    };
    let solid_area = f64::from(W) * f64::from(H) * f64::from(VOXEL_MM).powi(2); // 49 mm²
    let ring_area = solid_area - 25.0; // 24 mm² (wall ring)

    let mut layers = Vec::new();
    let mut idx: u32 = 0;
    let mut z_mm = 0.0_f32;
    let layer_height_mm = LAYER_HEIGHT_UM / 1000.0;
    let push = |area: f64,
                mask: LayerMask,
                layers: &mut Vec<LayerInput>,
                idx: &mut u32,
                z_mm: &mut f32| {
        layers.push(
            LayerInput::new(
                *idx,
                area,
                EXPOSURE_SEC,
                LIFT_SPEED_MM_MIN,
                LAYER_HEIGHT_UM,
                *z_mm,
            )
            .expect("valid LayerInput")
            .with_mask(mask),
        );
        *idx += 1;
        *z_mm += layer_height_mm;
    };
    for _ in 0..5 {
        push(
            solid_area,
            solid_mask.clone(),
            &mut layers,
            &mut idx,
            &mut z_mm,
        );
    }
    for _ in 0..10 {
        push(
            ring_area,
            ring_mask.clone(),
            &mut layers,
            &mut idx,
            &mut z_mm,
        );
    }
    push(solid_area, solid_mask, &mut layers, &mut idx, &mut z_mm);
    layers
}

fn run_and_capture(world: &mut UatWorld, layers: &[LayerInput], printer: &PrinterProfile) {
    let resin = ResinProfile::generic_standard();
    let supports = SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 10,
    };
    let plate = PlateAdhesionProfile::default_textured();
    let ambient = AmbientTemperature::new(22.0).expect("22 °C is in AmbientTemperature domain");
    let sim = SimulationRunner::run_from_layer_inputs(
        layers, &resin, printer, &supports, &plate, ambient, None, None,
    )
    .expect(
        "scenario fixture: generic_standard + PrinterBuilder output satisfy \
             run_from_layer_inputs preconditions",
    );
    world.peel_sim_layers = Some(sim.layers().to_vec());
}
