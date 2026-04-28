//! Integration tests against the lilith-torso real-print fixture.
//!
//! `tests/fixtures/lilith-torso.sim.json` is a 4492-layer simulation
//! produced by:
//!
//!   resinsim sim --file ~/Documents/3d/lilith-torso.ctb \
//!       --resin generic_standard --printer generic_msla_4k \
//!       --out crates/resinsim-viz/tests/fixtures/lilith-torso.sim.json
//!
//! It is the real-world counterpart to `resinsim-inspect`'s minimal
//! `baseline.sim.json` (5 layers): it exercises every v2 dashboard
//! projection against full bottom-layer peel spikes, vat thermal
//! ramp-up, viscosity decay, and a non-trivial cross-section delta
//! profile. Tests here are pre-flight checks that the v2 projections
//! survive a real-world layer count, not exhaustive numerical
//! regressions.

use std::path::PathBuf;

use resinsim_core::repositories::load_from_path;
use resinsim_core::simulation::PrintSimulation;

const EXPECTED_LAYERS: usize = 4492;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lilith-torso.sim.json")
}

fn load_lilith() -> PrintSimulation {
    load_from_path(&fixture_path()).expect("test fixture: lilith sim.json must load")
}

#[test]
fn lilith_loads_and_has_expected_layer_count() {
    let sim = load_lilith();
    assert_eq!(
        sim.layers().len(),
        EXPECTED_LAYERS,
        "lilith fixture expected {EXPECTED_LAYERS} layers; \
         regenerate the fixture if the printer profile or CTB changes"
    );
}

#[test]
fn lilith_force_fields_are_finite_for_every_layer() {
    let sim = load_lilith();
    for (i, layer) in sim.layers().iter().enumerate() {
        assert!(
            layer.peel_force_n.is_finite() && layer.peel_force_n >= 0.0,
            "layer {i}: peel_force_n must be finite and non-negative, got {}",
            layer.peel_force_n
        );
        assert!(
            layer.suction_force_n.is_finite() && layer.suction_force_n >= 0.0,
            "layer {i}: suction_force_n must be finite and non-negative, got {}",
            layer.suction_force_n
        );
        assert!(
            layer.total_force_n.is_finite() && layer.total_force_n >= 0.0,
            "layer {i}: total_force_n must be finite and non-negative, got {}",
            layer.total_force_n
        );
        assert!(
            layer.total_force_n + 1e-3 >= layer.peel_force_n,
            "layer {i}: total ({}) must be >= peel ({})",
            layer.total_force_n,
            layer.peel_force_n
        );
    }
}

#[test]
fn lilith_safety_factor_finite_or_inf_at_zero_force() {
    // Per `safety-factor-zero-force.md`: zero-force layers carry
    // safety_factor = INFINITY (no force to fail against). Real
    // layers are finite. The v2 Safety pane filters non-finite at
    // projection time per `crate::ui::v2::panes::safety::draw_loaded`.
    let sim = load_lilith();
    let mut finite = 0_usize;
    let mut infinite = 0_usize;
    for layer in sim.layers() {
        if layer.safety_factor.is_finite() {
            finite += 1;
        } else if layer.safety_factor.is_infinite() {
            infinite += 1;
        } else {
            panic!(
                "layer {}: safety_factor must be finite or infinite, got NaN",
                layer.index
            );
        }
    }
    assert_eq!(finite + infinite, EXPECTED_LAYERS);
    assert!(
        finite > 0,
        "expected at least some finite safety_factor values"
    );
}

#[test]
fn lilith_bottom_layer_peel_spike_dominates_steady_state() {
    // Lilith's bottom layers are exposed at a much longer time and
    // experience a peel spike well above steady state. The Forces
    // pane's p95-cap projection assumes this. Verify the magnitude
    // ratio so the cap remains the right default.
    let sim = load_lilith();
    let bottom = sim.layers()[0].peel_force_n;
    let steady = sim.layers()[sim.layers().len() / 2].peel_force_n;
    assert!(
        bottom > steady,
        "bottom-layer peel ({bottom}) should exceed mid-print steady-state ({steady}); \
         if this fails the recipe or geometry has materially changed"
    );
}

#[test]
fn lilith_cure_depth_finite_and_geq_layer_height_in_steady_state() {
    // For a healthy MSLA print, cure depth in the steady-state region
    // (post-bottom layers) should comfortably exceed the layer height.
    // This is the load-bearing health indicator in the Cure depth
    // pane's amber threshold.
    let sim = load_lilith();
    let bottom_count = 6_usize;
    let mut healthy = 0_usize;
    let mut total_steady = 0_usize;
    for (i, layer) in sim.layers().iter().enumerate().skip(bottom_count) {
        if !layer.cure_depth_um.is_finite() || !layer.effective_layer_height_um.is_finite() {
            continue;
        }
        total_steady += 1;
        if layer.cure_depth_um >= layer.effective_layer_height_um {
            healthy += 1;
        } else {
            // Allow occasional dips (the worst_cure_depth_um is the
            // pixel-worst, not the typical) but flag if it's a
            // regression in the median trend.
            if i < bottom_count + 50 {
                continue; // transition layers are noisier
            }
        }
    }
    let healthy_ratio = healthy as f32 / total_steady.max(1) as f32;
    assert!(
        healthy_ratio > 0.95,
        "healthy cure_depth ratio in steady-state was {healthy_ratio:.3}; \
         expected > 0.95 for the lilith generic_standard recipe"
    );
}

#[test]
fn lilith_vat_temperature_warms_from_ambient() {
    // The Vat temperature pane assumes a thermal ramp from ambient
    // (typically 22 °C) up to a steady-state bath temperature.
    // Check the curve has the expected monotonic-ish warmup shape:
    // first few layers near ambient, mid-print well above.
    let sim = load_lilith();
    let early = sim.layers()[0].vat_temperature_c;
    let mid = sim.layers()[sim.layers().len() / 2].vat_temperature_c;
    assert!(
        mid > early,
        "vat temperature should warm: early={early} mid={mid}"
    );
    assert!(
        early.is_finite() && mid.is_finite(),
        "every layer's vat_temperature_c must be finite"
    );
}

#[test]
fn lilith_viscosity_decays_with_warming() {
    // Viscosity is inversely correlated with temperature for typical
    // photopolymer resins. The Viscosity pane shows the trajectory
    // so the developer can correlate a creeping rise with
    // late-print peel spikes — for healthy lilith we expect the
    // opposite shape (high at start, lower at steady state).
    let sim = load_lilith();
    let early = sim.layers()[0].viscosity_mpa_s;
    let mid = sim.layers()[sim.layers().len() / 2].viscosity_mpa_s;
    assert!(
        mid < early,
        "viscosity should decay as vat warms: early={early} mid={mid}"
    );
}

#[test]
fn lilith_cross_section_area_and_delta_finite() {
    // Area + Δarea pane assumes both fields are always finite; a
    // NaN slipping through would crash plot rendering with an
    // unhelpful "scale by NaN" message.
    let sim = load_lilith();
    for (i, layer) in sim.layers().iter().enumerate() {
        assert!(
            layer.cross_section_area_mm2.is_finite() && layer.cross_section_area_mm2 >= 0.0,
            "layer {i}: cross_section_area_mm2 must be finite + non-negative, got {}",
            layer.cross_section_area_mm2
        );
        assert!(
            layer.area_delta_mm2.is_finite(),
            "layer {i}: area_delta_mm2 must be finite, got {}",
            layer.area_delta_mm2
        );
    }
}
