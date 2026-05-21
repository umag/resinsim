//! ADR-0020 / t2f4 — Mars 5 Ultra LED-case BC anchoring test.
//!
//! KB-152 fitted Tier-1 Stage-A coefficients for the Elegoo Mars 5 Ultra:
//!
//!   - `led_delta_t_steady_c = 13.5 °C`  (plateau ≈ 40.5 °C − idle ≈ 27 °C)
//!   - `led_tau_sec = 4000 s` (3 τ ≈ 3–4 h to 95 % plateau)
//!   - `initial_led_c = 27 °C` (idle-standby baseline)
//!   - `led_to_vat_coupling = 0.71` (USER ESTIMATE — no vat sensor)
//!
//! `data/elegoo/roden_uv_led_temp_dec_jan_hourly.csv` carries 722 hours of
//! real LED-case telemetry from the user's home-server, starting from
//! `2026-01-01T22:00:00+00:00` at the plateau. The vat-side ground truth
//! does NOT exist in `data/elegoo/` — only LED-case + ambient.
//!
//! This test is the **only ground-truth-calibrated quantity in v1**
//! (per ADR-0020 §Decision viii). The vat-side `coupling = 0.71` is an
//! unverified estimate; the Tier-2 vat-volume-mean is reported in the
//! run-end summary log but not constrained against external data
//! (vat-thermistor data collection is filed as a follow-on).
//!
//! ## What this test asserts
//!
//! The 722-hour fixture captures the printer's REAL usage profile —
//! mostly idle (~27 °C LED case ≈ KB-152 `initial_led_c`), with
//! intermittent active-print episodes that drive the LED case up
//! toward the steady-state plateau. So the load-bearing invariants
//! are bracket-style, not mean-style:
//!
//! - **Idle baseline matches KB-152.** The CSV's *median* hourly mean
//!   is within ±1.0 °C of KB-152's `initial_led_c = 27 °C` — confirms
//!   the idle-standby model.
//! - **Active-print peak matches KB-152 plateau.** At least one
//!   hourly mean is within ±1.0 °C of `initial_led_c + led_delta_t_steady_c
//!   = 40.5 °C` — confirms the asymptotic rise reaches what the model
//!   predicts.
//! - **`ThermalCalculator::led_temperature_at_time(10 τ)`** matches
//!   KB-152's plateau prediction within 0.01 °C — confirms the
//!   formula itself.
//!
//! ## What this test does NOT assert
//!
//! - The exponential ramp behaviour (the CSV starts AFTER the warm-up).
//! - Any vat-side temperature claim (no ground-truth data exists; the
//!   `coupling = 0.71` is a user estimate per KB-152).
//! - Any Tier-2 spatial-diffusion claim — Tier-2 reproduces Tier-1's
//!   LED case as its bottom Dirichlet BC, so anchoring the BC source
//!   anchors the input to the diffusion solve. The diffusion-driven
//!   output is reported (`tier-2 thermal complete:` log line) but not
//!   constrained against external data in v1.

use resinsim_core::services::ThermalCalculator;
use resinsim_core::values::ThermalTimeConstant;
use std::path::Path;

fn telemetry_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("elegoo")
        .join("roden_uv_led_temp_dec_jan_hourly.csv")
}

/// Parse a column from the LED telemetry CSV (header + 722 rows). The
/// `mean_c` column is at index 2.
fn read_led_mean_c() -> Vec<f32> {
    let path = telemetry_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("LED telemetry must exist at {}: {e}", path.display()));
    let mut out = Vec::with_capacity(722);
    for (idx, line) in text.lines().enumerate() {
        if idx == 0 {
            // header
            assert!(
                line.starts_with("timestamp_start,timestamp_end,mean_c"),
                "header schema must match expected — got {line:?}"
            );
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        assert!(
            cols.len() >= 5,
            "row {idx} has fewer than 5 columns: {line:?}"
        );
        let mean_c: f32 = cols[2]
            .parse()
            .unwrap_or_else(|e| panic!("row {idx} mean_c must parse as f32: {e}"));
        out.push(mean_c);
    }
    assert!(
        !out.is_empty(),
        "telemetry CSV must contain at least one row"
    );
    out
}

#[test]
fn mars5_ultra_led_csv_idle_median_matches_kb152_initial_led() {
    // KB-152 fitted coefficient — idle-standby LED case is ~4 °C above
    // ambient due to controller-electronics dissipation. The CSV
    // median is dominated by idle hours (the printer prints in
    // intermittent episodes).
    const INITIAL_LED_C: f32 = 27.0;
    const TOLERANCE_C: f64 = 1.0;

    let mut samples = read_led_mean_c();
    samples.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let median = samples[samples.len() / 2] as f64;
    eprintln!(
        "mars5 LED idle anchor: n={}, median={median:.3} °C, KB-152 initial_led={INITIAL_LED_C} °C",
        samples.len()
    );
    assert!(
        (median - INITIAL_LED_C as f64).abs() < TOLERANCE_C,
        "LED CSV median {median:.3} °C must match KB-152 initial_led_c {INITIAL_LED_C} °C \
         within ±{TOLERANCE_C} °C — the idle-standby baseline. If drifted, \
         recalibrate KB-152 + data/printers/elegoo_mars5_ultra.toml."
    );
}

#[test]
fn mars5_ultra_led_csv_active_peak_matches_kb152_plateau() {
    // KB-152 fitted coefficients for Mars 5 Ultra.
    const INITIAL_LED_C: f32 = 27.0;
    const DELTA_T_STEADY_C: f32 = 13.5;
    const EXPECTED_PLATEAU_C: f32 = INITIAL_LED_C + DELTA_T_STEADY_C; // 40.5
    const TOLERANCE_C: f64 = 1.0;

    let samples = read_led_mean_c();
    // The active-print episodes drive the LED case toward the plateau.
    // Look at the warmest rows — at least ONE hourly mean must be
    // within ±TOLERANCE_C of the KB-152 plateau prediction.
    let peak: f64 = samples
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max) as f64;
    eprintln!(
        "mars5 LED active peak: n={}, observed_peak={peak:.3} °C, \
         KB-152 plateau={EXPECTED_PLATEAU_C} °C",
        samples.len()
    );
    assert!(
        (peak - EXPECTED_PLATEAU_C as f64).abs() < TOLERANCE_C,
        "LED CSV peak {peak:.3} °C must match KB-152 plateau {EXPECTED_PLATEAU_C} °C \
         within ±{TOLERANCE_C} °C. If the fit has drifted, recalibrate \
         KB-152 + data/printers/elegoo_mars5_ultra.toml in lockstep."
    );
}

#[test]
fn thermal_calculator_far_future_matches_kb152_plateau() {
    // KB-152 fitted coefficients.
    let initial_led_c = 27.0_f32;
    let delta_t_steady_c = 13.5_f32;
    let tau = ThermalTimeConstant::new(4000.0)
        .expect("4000 s is a valid ThermalTimeConstant");
    // Far-future evaluation — 10 τ ≈ 11.1 hours after print start. At
    // 10 τ the exp(-t/τ) factor is 4.5e-5, well below the 0.5 °C
    // measurement tolerance.
    let t_far = 40_000.0_f32; // 10 τ
    let predicted = ThermalCalculator::led_temperature_at_time(
        initial_led_c,
        delta_t_steady_c,
        tau,
        t_far,
    );
    let expected_plateau = initial_led_c + delta_t_steady_c;
    eprintln!(
        "ThermalCalculator far-future: t={t_far}s, predicted={:.4} °C, \
         expected plateau={expected_plateau} °C",
        predicted.value()
    );
    assert!(
        (predicted.value() - expected_plateau).abs() < 0.01,
        "ThermalCalculator::led_temperature_at_time(10 τ) must match \
         KB-152 plateau {expected_plateau} °C within 0.01 °C, got {}",
        predicted.value()
    );
}
