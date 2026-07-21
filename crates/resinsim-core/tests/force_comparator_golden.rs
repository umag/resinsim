//! Committed regression golden for the calibration harness (ADR-0022 Stage 0).
//!
//! Locks the fit metrics `ForceComparator` + `ProfileCalibrator` produce on a
//! fixed synthetic predicted(N)/actual(counts) pair, so a future peel-model
//! correction can be *graded*: any drift in peak-layer, correlation, RMSE, or
//! fit R² trips a test here. Deterministic — no PNG decode, no large `.nanodlp`
//! archive (the real 37 MB Athena job stays a manual `#[ignore]` check in
//! `nanodlp_real_sample.rs`).
//!
//! Fixture shape mirrors the KB-115 finding: the real force peaks at layer 0
//! (base adhesion) while the area-driven sim peaks mid-print, and a single
//! counts→N gain fits with R²≈0. If a metric's *definition* changes, recompute
//! and update the constants deliberately — do not loosen the epsilons.

use resinsim_core::io::athena::AnalyticLog;
use resinsim_core::services::{
    ForceComparator, LayerForce, ProfileCalibrator,
};

/// Real per-layer peak signal (raw counts), peaking at layer 0.
fn actual() -> Vec<LayerForce> {
    [420.0_f64, 380.0, 300.0, 260.0, 180.0, 120.0]
        .into_iter()
        .enumerate()
        .map(|(i, peak)| LayerForce {
            index: i as u32,
            peak_signal: peak,
            mean_signal: peak * 0.8,
            sample_count: 4,
        })
        .collect()
}

/// Simulated per-layer force (Newtons), peaking mid-print at layer 2.
fn predicted() -> Vec<f32> {
    vec![3.0, 5.0, 6.5, 6.0, 4.0, 2.5]
}

#[test]
fn golden_peak_layers_expose_the_offset() {
    let report = ForceComparator::compare(&predicted(), &actual()).expect("compares");
    assert_eq!(report.layer_count, 6);
    assert_eq!(report.predicted_peak_layer, Some(2), "area-driven sim peaks mid-print");
    assert_eq!(report.actual_peak_layer, Some(0), "real force peaks at the base layer");
}

#[test]
fn golden_shape_metrics_are_stable() {
    let report = ForceComparator::compare(&predicted(), &actual()).expect("compares");
    assert!(
        (report.correlation - 0.237_628_479_1).abs() < 1e-9,
        "correlation drifted: {}",
        report.correlation
    );
    assert!(
        (report.normalized_rmse - 0.443_732_068_0).abs() < 1e-9,
        "normalized_rmse drifted: {}",
        report.normalized_rmse
    );
    assert!(
        (report.max_abs_error - 0.875).abs() < 1e-9,
        "max_abs_error drifted: {}",
        report.max_abs_error
    );
}

#[test]
fn golden_calibrator_fit_is_stable() {
    let o = ProfileCalibrator::calibrate(&predicted(), &actual(), &AnalyticLog::default())
        .expect("calibrates");
    // Single-print gain fit is poor (R² clamps to 0), exactly the KB-115 signal.
    assert!(
        (o.fit_quality - 0.0).abs() < 1e-9,
        "fit R² drifted: {}",
        o.fit_quality
    );
    assert!(
        (o.peel_gain_n_per_count - 0.014_642_041_1).abs() < 1e-9,
        "peel gain drifted: {}",
        o.peel_gain_n_per_count
    );
}
