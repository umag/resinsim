//! Mars 5 Ultra full-simulation integration test (ADR-0007 + KB-152 + KB-153).
//!
//! Runs a 2000-layer solid cube simulation on the Mars 5 Ultra (Tilt release) at
//! 23 °C ambient with initial_led = 27 °C. Asserts property bounds for the
//! two-stage LED → vat thermal model approaching the fitted plateau.
//!
//! Bounds are intentionally wide so the test survives coupling / Ea_cure
//! re-calibration (KB-152 notes the coupling estimate + Ea_cure ±50% band).

use resinsim_core::app::SimulationRunner;
use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::services::LayerTimingCalculator;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea, InitialLedTemperature};

fn cube_areas(n_layers: usize, area_mm2: f64) -> Vec<CrossSectionArea> {
    vec![
        CrossSectionArea::new(area_mm2)
            .expect("test fixture: positive finite mm² is in CrossSectionArea domain");
        n_layers
    ]
}

#[test]
fn mars5_ultra_2000_layer_thermal_plateau() {
    let recipe = resinsim_core::entities::Recipe::generic_standard();
    let printer = PrinterProfile::elegoo_mars5_ultra();
    let resin = ResinProfile::generic_standard();

    // Verify the Mars 5 Ultra (Tilt) per-layer time is in the expected range so
    // the 4h / 8h layer indices below make physical sense.
    let t_normal = LayerTimingCalculator::layer_time_sec(&recipe, &printer, 1000);
    assert!(
        (t_normal - 10.5).abs() < 1.0,
        "Mars 5 Ultra (Tilt) normal layer time should be ~10.5 sec, got {t_normal}"
    );

    // Cumulative times for cube: find the layer index whose cumulative time
    // crosses 4h (14400 sec) and 8h (28800 sec).
    let times = LayerTimingCalculator::cumulative_times_sec(&recipe, &printer, 3500);
    let layer_4h = times
        .iter()
        .position(|&t| t >= 14_400.0)
        .expect("3500 layers at 10.5 s/layer spans >4h");
    let layer_8h = times
        .iter()
        .position(|&t| t >= 28_800.0)
        .expect("3500 layers at 10.5 s/layer spans >8h");

    let n_layers = (layer_8h + 10).max(2000);
    let areas = cube_areas(n_layers, 100.0);

    let sim = SimulationRunner::run_from_areas(
        &areas,
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 20,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(23.0).expect("23.0 °C is a valid AmbientTemperature"),
        Some(InitialLedTemperature::new(27.0).expect("27.0 °C is a valid InitialLedTemperature")),
    )
    .expect("validated Mars 5 Ultra profile satisfies run_from_areas preconditions");

    let layers = sim.layers();
    let ambient = 23.0_f32;
    let coupling = printer.led_to_vat_coupling();
    let led_delta = printer.led_delta_t_steady_c();
    let vat_plateau = ambient + coupling * ((27.0 + led_delta) - ambient);
    let vat_initial = ambient + coupling * (27.0 - ambient);
    let half_rise = vat_initial + 0.5 * (vat_plateau - vat_initial);

    // Approaching plateau at 4h: vat should have passed the half-rise mark (τ ≈
    // 4000 s so t = 4h ≈ 3.6τ ⇒ ~97% of rise). Use 0.5 × rise as the lower
    // bound so the test survives ±20% coupling / τ re-calibration.
    let vat_4h = layers[layer_4h].vat_temperature_c;
    assert!(
        vat_4h > half_rise,
        "vat at 4h ({vat_4h:.2} °C) should exceed half-rise ({half_rise:.2} °C)"
    );
    assert!(
        vat_4h < vat_plateau + 1.0,
        "vat at 4h ({vat_4h:.2} °C) should not exceed plateau ({vat_plateau:.2} °C) by >1°"
    );

    // Stable near plateau at 8h — within ±1 °C of the 4h sample.
    let vat_8h = layers[layer_8h].vat_temperature_c;
    assert!(
        (vat_8h - vat_4h).abs() < 1.0,
        "vat at 8h ({vat_8h:.2} °C) should be within ±1 °C of 4h ({vat_4h:.2} °C)"
    );

    // Cure depth at the thermal plateau is greater than at an early normal-phase
    // layer because elevated vat temperature lowers Ec (KB-153 Arrhenius
    // correction). Layer 0 uses bottom_exposure_sec (25 s) and would dominate
    // via exposure rather than thermal physics; compare two normal-phase layers
    // instead (same exposure) to isolate the Ec(T) effect.
    let normal_phase_start = (recipe.bottom_layer_count() + recipe.transition_layers()) as usize;
    let cd_early_normal = layers[normal_phase_start].cure_depth_um;
    let cd_plateau_normal = layers[layer_8h].cure_depth_um;
    assert!(
        cd_plateau_normal > cd_early_normal,
        "cure depth at plateau normal ({cd_plateau_normal:.2} µm) should exceed early-normal \
         ({cd_early_normal:.2} µm) via KB-153 Ec(T) correction"
    );
}
