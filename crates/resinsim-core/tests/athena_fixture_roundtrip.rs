//! Fixture round-trip integration test for the Athena analytic-log surface,
//! sourced from a COMMITTED file (not an in-code fixture) — the first test
//! to exercise `load_analytic_csv`'s plain-CSV magic-byte branch from disk
//! rather than a temp file written inline by a unit test. Pins exact
//! hand-computed statistics for
//! `crates/resinsim-core/tests/fixtures/synthetic_stepped_forces.csv`, then
//! runs the full chain parse -> extract -> filter -> compare -> calibrate.
//!
//! ## Relationship to `force_comparator_golden.rs`
//!
//! That file pins `ForceComparator` / `ProfileCalibrator` metrics on an
//! **in-code** fixture (`actual()` / `predicted()` built as literal
//! `LayerForce`/`Vec<f32>` values in the test body). THIS file pins the same
//! metric *definitions* on a **file-sourced** fixture — loaded from the
//! committed `synthetic_stepped_forces.csv` via `load_analytic_csv` — with
//! different numeric values (predicted peaks at layer 3 here, vs layer 2 in
//! the golden file). The two are deliberately NOT a duplicate: one guards
//! the in-memory metric math against an arbitrary fixture, the other guards
//! the same math against a real parse of a committed file. If a metric's
//! *definition* changes, both files must be recomputed and updated together
//! — that is why each cross-references the other in its module doc.
//!
//! ## Fixture provenance / regeneration
//!
//! `synthetic_stepped_forces.csv` is KB-115-shaped (force peaks at layer 0,
//! decays monotonically to an honest-zero marker-only layer 5) — see its
//! sibling commit message and `crates/resinsim-core/tests/athena_properties.rs`
//! for the full design rationale. The `.csv.gz` twin was generated
//! reproducibly with:
//!
//! ```text
//! gzip -9 -n -k synthetic_stepped_forces.csv
//! ```
//!
//! using **Apple gzip 430.140.2** (macOS; `-n` suppresses the embedded
//! name/mtime so the archive is byte-reproducible from the same input on the
//! same tool). Regenerate with the exact same command if the plain CSV ever
//! changes — gzip output bytes are not guaranteed identical across gzip
//! implementations (GNU vs Apple), but this test only asserts semantic
//! equality (`parse(plain) == parse(gz)`), which is regeneration-tool
//! independent; only the *tool+version* needs to be recorded, not matched
//! exactly on every machine.
//!
//! ## Float comparison policy
//!
//! Same as `athena_properties.rs`: exact equality only for integer counts
//! and pure selections (e.g. a layer's `peak_signal`, which is one of its
//! input values verbatim, and the honest-zero literal `0.0`). Every
//! sum/division/correlation-derived float (means, RMSE, correlation, R²,
//! fitted gain) is compared with a `1e-9` epsilon — never bit-equality —
//! per the adversarial review condition on this issue.

use std::path::{
    Path,
    PathBuf,
};

use resinsim_core::{
    io::athena::{
        CH_AMBIENT_TEMP,
        CH_LAYER_HEIGHT,
        CH_PRESSURE,
        CH_RESIN_TEMP,
        load_analytic_csv,
    },
    services::{
        ForceComparator,
        ForceSeriesExtractor,
        ProfileCalibrator,
        filter_layer_range,
        peak_index,
    },
};

const UNDOCUMENTED_CHANNEL: u8 = 13;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Simulated per-layer peel force (Newtons), one entry per fixture layer
/// (0..=5). Chosen to peak mid-print at layer 3 — a KB-115-style offset from
/// the real signal's layer-0 peak — and DISTINCT from
/// `force_comparator_golden.rs`'s `predicted()` (which peaks at layer 2), so
/// the two fixtures are not accidentally testing the same numbers twice.
fn predicted() -> Vec<f32> {
    vec![1.5, 3.0, 5.5, 6.0, 4.5, 2.0]
}

#[test]
fn plain_and_gzip_twins_parse_identically() {
    let plain = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses (plain-CSV magic-byte branch)");
    let gz = load_analytic_csv(&fixture("synthetic_stepped_forces.csv.gz"))
        .expect("committed gzip twin parses (gzip magic-byte branch)");
    assert_eq!(
        plain.samples, gz.samples,
        "gzip twin has drifted from the plain CSV — regenerate with `gzip -9 -n -k` (Apple gzip 430.140.2)"
    );
}

#[test]
fn hand_computed_log_statistics_are_exact() {
    let log = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses");

    // Integer counts: exact equality is correct, not a derived float.
    assert_eq!(log.samples.len(), 39);
    assert_eq!(log.channel(CH_LAYER_HEIGHT).len(), 6);
    assert_eq!(log.channel(CH_PRESSURE).len(), 22);
    assert_eq!(log.channel(CH_RESIN_TEMP).len(), 5);
    assert_eq!(log.channel(CH_AMBIENT_TEMP).len(), 5);
    assert_eq!(log.channel(UNDOCUMENTED_CHANNEL).len(), 1);
    let partition_sum = log.channel(CH_LAYER_HEIGHT).len()
        + log.channel(CH_PRESSURE).len()
        + log.channel(CH_RESIN_TEMP).len()
        + log.channel(CH_AMBIENT_TEMP).len()
        + log.channel(UNDOCUMENTED_CHANNEL).len();
    assert_eq!(
        partition_sum,
        log.samples.len(),
        "the 5 known channels must partition all 39 samples"
    );

    // channel_mean(CH_PRESSURE) is over the WHOLE log (raw counts, incl. the
    // 2-sample prelude of -6.0/-6.0) — deliberately chosen so
    // -4752.0 / 22 == -216.0 exactly; still asserted with an epsilon per the
    // float comparison policy above (it is a division).
    let pressure_mean = log
        .channel_mean(CH_PRESSURE)
        .expect("pressure channel present");
    assert!(
        (pressure_mean - (-216.0)).abs() < 1e-9,
        "pressure mean drifted: {pressure_mean}"
    );

    let resin_mean = log
        .channel_mean(CH_RESIN_TEMP)
        .expect("resin temp channel present");
    assert!(
        (resin_mean - 28.0).abs() < 1e-9,
        "resin mean drifted: {resin_mean}"
    );

    let ambient_mean = log
        .channel_mean(CH_AMBIENT_TEMP)
        .expect("ambient temp channel present");
    assert!(
        (ambient_mean - 22.0).abs() < 1e-9,
        "ambient mean drifted: {ambient_mean}"
    );
}

#[test]
fn extract_with_prelude_count_matches_hand_computed_layer_table() {
    let log = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses");
    let (layers, prelude) = ForceSeriesExtractor::extract_with_prelude_count(&log);

    assert_eq!(prelude, 2, "two T=6 samples precede the first T=0 marker");
    assert_eq!(layers.len(), 6);

    // (index, peak_signal, mean_signal, sample_count) — hand-computed from
    // the fixture's raw values, see the fixture's sibling commit message.
    // Layer 5 is marker-only: the honest-zero case.
    let expected: [(u32, f64, f64, usize); 6] = [
        (0, 400.0, 385.0, 4),
        (1, 320.0, 305.0, 4),
        (2, 240.0, 225.0, 4),
        (3, 180.0, 165.0, 4),
        (4, 120.0, 105.0, 4),
        (5, 0.0, 0.0, 0),
    ];
    for (l, &(idx, peak, mean, count)) in layers.iter().zip(expected.iter()) {
        assert_eq!(l.index, idx);
        assert_eq!(l.sample_count, count, "layer {idx} sample_count drifted");
        // peak_signal is a selection (max of its own inputs) — exact.
        assert!(
            (l.peak_signal - peak).abs() < 1e-9,
            "layer {idx} peak drifted: {}",
            l.peak_signal
        );
        // mean_signal is a sum/len derivation — epsilon.
        assert!(
            (l.mean_signal - mean).abs() < 1e-9,
            "layer {idx} mean drifted: {}",
            l.mean_signal
        );
    }
}

#[test]
fn peak_index_is_layer_zero_the_kb115_shape() {
    let log = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses");
    let layers = ForceSeriesExtractor::extract_layer_forces(&log);
    assert_eq!(
        peak_index(&layers),
        Some(0),
        "KB-115 shape: real force peaks at the base layer"
    );
}

#[test]
fn filter_layer_range_middle_window_matches_hand_computed_set() {
    let log = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses");
    let layers = ForceSeriesExtractor::extract_layer_forces(&log);
    let windowed = filter_layer_range(&layers, Some(1), Some(3));

    let indices: Vec<u32> = windowed.iter().map(|l| l.index).collect();
    assert_eq!(indices, vec![1, 2, 3]);
    assert!((windowed[0].peak_signal - 320.0).abs() < 1e-9);
    assert!((windowed[1].peak_signal - 240.0).abs() < 1e-9);
    assert!((windowed[2].peak_signal - 180.0).abs() < 1e-9);
}

#[test]
fn compare_and_calibrate_against_fixed_predicted_series() {
    let log = load_analytic_csv(&fixture("synthetic_stepped_forces.csv"))
        .expect("committed plain fixture parses");
    let actual = ForceSeriesExtractor::extract_layer_forces(&log);
    let predicted = predicted();

    let report =
        ForceComparator::compare(&predicted, &actual).expect("6 predicted, 6 actual — compares");
    assert_eq!(report.layer_count, 6);
    assert_eq!(
        report.actual_peak_layer,
        Some(0),
        "real force peaks at the base layer"
    );
    assert_eq!(
        report.predicted_peak_layer,
        Some(3),
        "predicted() peaks mid-print at layer 3"
    );

    // Golden-captured (docs/patterns/golden-file-byte-identity-guard.md): ran
    // this test once with the bounds-only assertions below and an eprintln!
    // of the actual computed values, then pinned exactly what was printed.
    // All three are sum/division/correlation derivations — 1e-9 epsilon,
    // never bit-equality, per the adversarial review condition on this
    // issue. If a metric's *definition* changes, recompute deliberately and
    // update both this file and force_comparator_golden.rs together.
    assert!(
        (report.correlation - (-0.168_847_511_212_176_3)).abs() < 1e-9,
        "correlation drifted: {}",
        report.correlation
    );
    assert!(
        (report.normalized_rmse - 0.540_142_680_433_917_9).abs() < 1e-9,
        "normalized_rmse drifted: {}",
        report.normalized_rmse
    );
    assert!(
        (report.max_abs_error - 1.0).abs() < 1e-9,
        "max_abs_error drifted: {}",
        report.max_abs_error
    );

    let overrides = ProfileCalibrator::calibrate(&predicted, &actual, &log)
        .expect("6 predicted, 6 actual, nonzero signal — calibrates");
    let delta_t = overrides
        .delta_t_steady_c
        .expect("both T=7 and T=8 present in the fixture");
    assert!((delta_t - 6.0).abs() < 1e-9, "delta_t drifted: {delta_t}");
    assert!(
        (overrides.fit_quality - 0.0).abs() < 1e-9,
        "fit_quality drifted: {}",
        overrides.fit_quality
    );
    assert!(
        (overrides.peel_gain_n_per_count - 0.012_268_266_085_060_0).abs() < 1e-9,
        "peel_gain_n_per_count drifted: {}",
        overrides.peel_gain_n_per_count
    );
}
