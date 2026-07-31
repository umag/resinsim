//! Property tests for the Athena analytic-log surface: parser (`io/athena.rs`),
//! segmentation (`services::force_series_extractor`), and the comparator /
//! calibrator metrics (`services::force_comparator`,
//! `services::profile_calibrator`). See ADR-0021, ADR-0022 Stage 0, and the
//! `athena-tdd-coverage` issue plan.
//!
//! Four labelled blocks, one per surface, mirroring the layout of
//! `tests/force_properties.rs`:
//!
//! 1. **Parser** — render→parse identity, undocumented-channel forward
//!    compatibility, channel partition, `channel_mean` bounds, `peel_signal`
//!    involution, and the row-numbered parse error UAT-2 depends on.
//! 2. **Extractor** — `ForceSeriesExtractor` / `LayerForce` segmentation:
//!    layer count, index ordering, the sample-count conservation invariant,
//!    peak/mean bounds, the honest-zero case, and `peak_index` tie-break.
//! 3. **Comparator / calibrator** — the metric-range invariants that are the
//!    live successors of the deleted `ForceStats` bounds ("mean in
//!    [min,max]", "std_dev >= 0"): `ComparisonReport` and `ProfileOverrides`
//!    fields, including the documented error paths. Degenerate inputs
//!    (constant series, all-zero actual) are deliberately generated, not
//!    excluded — they are the documented edge cases, not noise.
//! 4. **`filter_layer_range`** — the CLI layer-range filter lifted out of
//!    `resinsim-inspect::cmd_athena` into `services::force_series_extractor`
//!    (plan step 6, precedent: `docs/patterns/single-source-peak-index-argmax.md`).
//!    Written RED-first: this block was added and confirmed to fail to
//!    compile (the function did not exist yet) before the production
//!    function landed. Order-preserving subsequence; retained indices inside
//!    the requested range; monotone under range widening; `from > to` is
//!    empty; `(None, None)` is the identity.
//!
//! Blocks 1-3 cover EXISTING behaviour and were green on first run; a red
//! property there is a real defect, not a test bug. Block 4 is a genuine
//! red-green (see above).
//!
//! Strategy hygiene (`docs/patterns/anti/rust-nan-positive-validation-gap.md`):
//! every `f64` strategy here is bounded and finite, and filters out `-0.0`.
//! NaN and infinity both parse back through `f64::from_str`, so excluding
//! them from the generator (rather than relying on downstream checks) is
//! required, not optional. No CSV quoting/escaping is exercised anywhere in
//! this file because all three analytic-log fields are numeric — that gap is
//! deliberate, not an oversight.
//!
//! Float comparisons: exact equality (`==` / `prop_assert_eq!`) is used only
//! for (a) integer fields, (b) the render→parse identity check (an explicit
//! byte round-trip, not an arithmetic derivation), and (c) values produced by
//! a pure selection or a literal zero-assignment in production code (e.g. a
//! layer's `peak_signal` is one of its input values verbatim; the honest-zero
//! literals are hardcoded `0.0`, not computed). Every float that is *derived*
//! via sum/division/correlation (means, RMSE, Pearson correlation, R²) is
//! compared with a `1e-9` epsilon, per the adversarial review condition on
//! this issue — Pearson correlation in particular can exceed `[-1, 1]` by
//! float noise.

use std::collections::BTreeSet;

use proptest::prelude::*;
use resinsim_core::{
    io::athena::{
        AnalyticLog,
        AnalyticSample,
        CH_AMBIENT_TEMP,
        CH_CURE_TIME,
        CH_DYNAMIC_WAIT,
        CH_LAYER_HEIGHT,
        CH_LAYER_TIME,
        CH_LIFT_HEIGHT,
        CH_PRESSURE,
        CH_RESIN_TEMP,
        CH_SOLID_AREA,
        CH_SPEED,
        parse_analytic,
        peel_signal,
    },
    services::{
        ForceComparator,
        ForceSeriesExtractor,
        LayerForce,
        ProfileCalibrator,
        filter_layer_range,
        peak_index,
    },
};

/// Every channel code the parser documents (io/athena.rs `CH_*` constants).
/// Used to generate channel bytes guaranteed to be outside the documented
/// map, for the forward-compatibility property.
const DOCUMENTED_CHANNELS: [u8; 10] = [
    CH_LAYER_HEIGHT,
    CH_SOLID_AREA,
    CH_SPEED,
    CH_CURE_TIME,
    CH_PRESSURE,
    CH_RESIN_TEMP,
    CH_AMBIENT_TEMP,
    CH_LAYER_TIME,
    CH_LIFT_HEIGHT,
    CH_DYNAMIC_WAIT,
];

fn sample(ts_ns: u64, channel: u8, value: f64) -> AnalyticSample {
    AnalyticSample {
        ts_ns,
        channel,
        value,
    }
}

/// Finite `f64` in a moderate range, excluding `-0.0` (strategy hygiene, see
/// module doc). The bound keeps sums of a few dozen samples well clear of
/// overflow without constraining the domain in any way that matters to these
/// properties.
fn finite_value() -> impl Strategy<Value = f64> {
    (-1.0e6f64..1.0e6).prop_filter("exclude -0.0", |v| !(*v == 0.0 && v.is_sign_negative()))
}

fn ts_ns_strategy() -> impl Strategy<Value = u64> {
    any::<u64>()
}

fn channel_strategy() -> impl Strategy<Value = u8> {
    any::<u8>()
}

fn sample_strategy() -> impl Strategy<Value = AnalyticSample> {
    (ts_ns_strategy(), channel_strategy(), finite_value())
        .prop_map(|(ts_ns, channel, value)| sample(ts_ns, channel, value))
}

// ============================================================================
// Block 1 — parser properties (io/athena.rs: AnalyticLog, parse_analytic).
// ============================================================================

proptest! {
    /// (a) Render an arbitrary sample list to tall `ID,T,V` CSV, then
    /// `parse_analytic` recovers it exactly, INCLUDING order. `{:?}`
    /// formatting is Rust's shortest round-trip representation, so this is a
    /// parse-identity check (exact equality is explicitly safe here, per the
    /// module doc's float-comparison policy).
    #[test]
    fn render_then_parse_is_identity_including_order(
        samples in prop::collection::vec(sample_strategy(), 0..40),
    ) {
        let mut csv_text = String::from("ID,T,V\n");
        for s in &samples {
            csv_text.push_str(&format!("{},{},{:?}\n", s.ts_ns, s.channel, s.value));
        }
        let log = parse_analytic(csv_text.as_bytes())
            .expect("every row is well-formed by construction: ts_ns/channel are unsigned ints, value is a finite f64 rendered via {:?} shortest round-trip");
        prop_assert_eq!(log.samples, samples);
    }

    /// (b) A sample on a channel code outside the documented map survives
    /// the parse under its own channel — forward compatibility with a future
    /// NanoDLP build (dddAnalysis: the channel is a bare u8, not a validated
    /// enum) — while `channel(CH_PRESSURE)` / `peel_signal_series()` do NOT
    /// mistake it for a pressure sample.
    #[test]
    fn undocumented_channel_survives_and_is_ignored_by_pressure_accessors(
        channel in (0u8..=255).prop_filter("channel not in the documented map", |c| !DOCUMENTED_CHANNELS.contains(c)),
        value in finite_value(),
        ts_ns in ts_ns_strategy(),
    ) {
        let log = AnalyticLog { samples: vec![sample(ts_ns, channel, value)] };

        // Survives: retrievable under its own (undocumented) channel code.
        // channel_mean of a single sample is x/1.0, exact for any finite x.
        prop_assert_eq!(log.channel(channel).len(), 1);
        prop_assert_eq!(log.channel(channel)[0], (ts_ns, value));
        prop_assert_eq!(log.channel_mean(channel), Some(value));

        // Ignored: not mistaken for a pressure/peel sample.
        prop_assert!(log.channel(CH_PRESSURE).is_empty());
        prop_assert!(log.peel_signal_series().is_empty());
    }

    /// (c) Partition invariant: summing `channel(c).len()` over every
    /// DISTINCT channel actually present in the log recovers `samples.len()`
    /// exactly. Every sample belongs to exactly one channel bucket.
    #[test]
    fn channel_partition_covers_all_samples(
        samples in prop::collection::vec(sample_strategy(), 0..40),
    ) {
        let log = AnalyticLog { samples: samples.clone() };
        let distinct_channels: BTreeSet<u8> = samples.iter().map(|s| s.channel).collect();
        let partition_sum: usize = distinct_channels.iter().map(|&c| log.channel(c).len()).sum();
        prop_assert_eq!(partition_sum, samples.len());
    }

    /// (d) `channel_mean(c)` is `Some` iff channel `c` is present in the log,
    /// and (the audit's relocated "mean in [min,max]") lies within the
    /// [min, max] of that channel's own values. Mean is a genuine
    /// sum/len derivation, so the bound uses a 1e-9 epsilon.
    #[test]
    fn channel_mean_present_iff_channel_present_and_within_bounds(
        samples in prop::collection::vec(sample_strategy(), 0..40),
        probe_channel in channel_strategy(),
    ) {
        let log = AnalyticLog { samples: samples.clone() };
        let vals: Vec<f64> = samples.iter().filter(|s| s.channel == probe_channel).map(|s| s.value).collect();
        match log.channel_mean(probe_channel) {
            Some(mean) => {
                prop_assert!(!vals.is_empty());
                let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                prop_assert!(mean >= min - 1e-9 && mean <= max + 1e-9,
                    "mean {mean} outside [{min}, {max}]");
            }
            None => prop_assert!(vals.is_empty()),
        }
    }

    /// (e) `peel_signal` is an involution, and `peel_signal_series()` equals
    /// `channel(CH_PRESSURE)` negated, same length and order. Double negation
    /// is an exact IEEE-754 bit operation (sign-bit flip, no rounding), so
    /// exact equality is safe here — this is not a sqrt/division/correlation
    /// derivation.
    #[test]
    fn peel_signal_is_involution_and_series_matches_negated_channel(
        samples in prop::collection::vec(sample_strategy(), 0..40),
    ) {
        for s in &samples {
            prop_assert_eq!(peel_signal(peel_signal(s.value)), s.value);
        }

        let log = AnalyticLog { samples: samples.clone() };
        let pressure_channel = log.channel(CH_PRESSURE);
        let peel_series = log.peel_signal_series();
        prop_assert_eq!(peel_series.len(), pressure_channel.len());
        for (&(ts_a, raw), &(ts_b, peeled)) in pressure_channel.iter().zip(peel_series.iter()) {
            prop_assert_eq!(ts_a, ts_b);
            prop_assert_eq!(peeled, peel_signal(raw));
        }
    }

    /// (f) UAT-2 at library level: a non-numeric `V` at an arbitrary row
    /// makes `parse_analytic` return `Err` whose message names that exact
    /// 1-based row.
    #[test]
    fn non_numeric_value_names_the_offending_row(
        rows in prop::collection::vec((ts_ns_strategy(), channel_strategy(), finite_value()), 1..15),
        bad_idx_seed in any::<usize>(),
    ) {
        let bad_idx = bad_idx_seed % rows.len();
        let mut csv_text = String::from("ID,T,V\n");
        for (i, (ts_ns, channel, value)) in rows.iter().enumerate() {
            if i == bad_idx {
                csv_text.push_str(&format!("{ts_ns},{channel},not_a_number\n"));
            } else {
                csv_text.push_str(&format!("{ts_ns},{channel},{value:?}\n"));
            }
        }
        let err = parse_analytic(csv_text.as_bytes())
            .expect_err("row bad_idx contains a non-numeric V field, must fail to parse");
        let needle = format!("row {}", bad_idx + 1);
        prop_assert!(err.contains(needle.as_str()), "error {:?} does not name {needle}", err);
    }
}

// ============================================================================
// Block 2 — extractor properties (services::force_series_extractor).
// ============================================================================

/// Builds an `AnalyticLog` directly from a prelude pressure-signal list and a
/// list of per-layer pressure-signal lists, each preceded by a `T=0` marker.
/// Values here are already in "signal" space (post `peel_signal`); the raw
/// stored value is the negation, matching the real sign convention
/// documented in `io/athena.rs`. Direct struct construction (not a CSV round
/// trip) — this exercises `ForceSeriesExtractor` in isolation from the
/// parser, which block 1 already covers.
fn build_log(prelude_signal: &[f64], layers_signal: &[Vec<f64>]) -> AnalyticLog {
    let mut ts = 0u64;
    let mut samples = Vec::new();
    for &sig in prelude_signal {
        samples.push(sample(ts, CH_PRESSURE, -sig));
        ts += 1;
    }
    for layer in layers_signal {
        samples.push(sample(ts, CH_LAYER_HEIGHT, 0.0));
        ts += 1;
        for &sig in layer {
            samples.push(sample(ts, CH_PRESSURE, -sig));
            ts += 1;
        }
    }
    AnalyticLog { samples }
}

/// `(prelude, layers)` — at least one T=0 marker (a real print always opens
/// with one; `no_layer_markers_yields_empty` in `force_series_extractor.rs`
/// already covers the zero-marker case as a unit test), each layer's sample
/// list may legitimately be empty (the honest-zero case).
fn layers_strategy() -> impl Strategy<Value = (Vec<f64>, Vec<Vec<f64>>)> {
    (
        prop::collection::vec(finite_value(), 0..4),
        prop::collection::vec(prop::collection::vec(finite_value(), 0..6), 1..8),
    )
}

proptest! {
    /// (a) Layer count equals the number of T=0 markers.
    #[test]
    fn layer_count_equals_marker_count((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        prop_assert_eq!(forces.len(), layers.len());
    }

    /// (b) `index` values are exactly `0..n-1` and strictly increasing (the
    /// audit's relocated `filter_layers` monotonicity intent).
    #[test]
    fn layer_indices_are_0_to_n_minus_1_strictly_increasing((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        for (i, f) in forces.iter().enumerate() {
            prop_assert_eq!(f.index, i as u32);
        }
        for w in forces.windows(2) {
            prop_assert!(w[1].index > w[0].index);
        }
    }

    /// (c) Conservation (docs/patterns/decomposition-invariant-for-result-structs.md):
    /// `sum(sample_count) + prelude` equals the number of `T=6` samples in
    /// the log. The property most likely to expose a real segmentation
    /// defect — if this goes red, report it, do not weaken it.
    #[test]
    fn sample_count_conservation_with_prelude((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let (forces, prelude_count) = ForceSeriesExtractor::extract_with_prelude_count(&log);
        prop_assert_eq!(prelude_count, prelude.len());
        let sum_sample_counts: usize = forces.iter().map(|f| f.sample_count).sum();
        prop_assert_eq!(sum_sample_counts + prelude_count, log.channel(CH_PRESSURE).len());
    }

    /// (d) For every layer with `sample_count > 0`: `peak_signal` equals the
    /// max of that layer's sign-corrected samples (a selection, exact),
    /// `mean_signal` lies within [min, max] (a derivation, epsilon), and
    /// `peak_signal >= mean_signal`.
    #[test]
    fn peak_and_mean_bounds_with_peak_ge_mean((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        prop_assert_eq!(forces.len(), layers.len());
        for (f, raw_layer) in forces.iter().zip(layers.iter()) {
            if f.sample_count > 0 {
                let min = raw_layer.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = raw_layer.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                prop_assert!((f.peak_signal - max).abs() < 1e-9,
                    "peak {} != max {}", f.peak_signal, max);
                prop_assert!(f.mean_signal >= min - 1e-9 && f.mean_signal <= max + 1e-9,
                    "mean {} outside [{}, {}]", f.mean_signal, min, max);
                prop_assert!(f.peak_signal >= f.mean_signal - 1e-9,
                    "peak {} < mean {}", f.peak_signal, f.mean_signal);
            }
        }
    }

    /// (e) Honest zero (docs/patterns/honest-zero-with-model-gap-caveat.md):
    /// `sample_count == 0` implies `peak_signal == 0.0 && mean_signal ==
    /// 0.0`. One-way implication only — the generator is not constrained to
    /// avoid all-zero-valued real samples (that would be a guard that cannot
    /// observe its own failure mode); the converse is pinned separately by
    /// the fixture round-trip test's marker-only layer.
    #[test]
    fn zero_samples_implies_honest_zero((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        for f in &forces {
            if f.sample_count == 0 {
                // Hardcoded 0.0 literals in the `finish` closure, not a
                // computed value — exact equality is safe.
                prop_assert_eq!(f.peak_signal, 0.0);
                prop_assert_eq!(f.mean_signal, 0.0);
            }
        }
    }

    /// (f) `peak_index` on non-empty input returns the EARLIEST index
    /// attaining the maximum `peak_signal` (single-source argmax contract,
    /// docs/patterns/single-source-peak-index-argmax.md).
    #[test]
    fn peak_index_returns_earliest_maximum((prelude, layers) in layers_strategy()) {
        let log = build_log(&prelude, &layers);
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        match peak_index(&forces) {
            Some(idx) => {
                let max_val = forces.iter().map(|f| f.peak_signal).fold(f64::NEG_INFINITY, f64::max);
                prop_assert!((forces[idx].peak_signal - max_val).abs() < 1e-9);
                prop_assert!(forces[..idx].iter().all(|f| f.peak_signal < max_val - 1e-9),
                    "a layer before idx {idx} also attains the max — not earliest");
            }
            None => prop_assert!(forces.is_empty()),
        }
    }
}

// ============================================================================
// Block 3 — comparator / calibrator metric-range properties
// (services::force_comparator, services::profile_calibrator). Live
// successors of the deleted `ForceStats` invariants ("mean in [min,max]",
// "std_dev >= 0"). Degenerate inputs (constant series, all-zero actual) are
// generated deliberately — excluding them would make these properties pass
// vacuously on exactly the cases the audit cared about.
// ============================================================================

/// Minimal `LayerForce` builder for property fixtures — `mean_signal`
/// mirrors `peak_signal`, `sample_count` fixed at 1 (not exercised by these
/// comparator/calibrator properties, which only read `peak_signal`). This is
/// a FOURTH inlined copy of the pattern already duplicated in
/// `force_series_extractor.rs`, `force_comparator.rs`, and
/// `profile_calibrator.rs` test modules — flagged per
/// docs/patterns/anti/fixture-copy-of-shared-builder.md as a harvest
/// candidate: extract a shared `#[cfg(test)]` helper now that a fourth call
/// site exists.
fn lf(index: u32, peak: f64) -> LayerForce {
    LayerForce {
        index,
        peak_signal: peak,
        mean_signal: peak,
        sample_count: 1,
    }
}

fn peak_values_strategy(len: usize) -> impl Strategy<Value = Vec<f64>> {
    prop_oneof![
        3 => prop::collection::vec(-1.0e3f64..1.0e3, len),
        1 => (-1.0e3f64..1.0e3).prop_map(move |v| vec![v; len]),
        1 => Just(vec![0.0f64; len]),
    ]
}

fn predicted_strategy(len: usize) -> impl Strategy<Value = Vec<f32>> {
    prop_oneof![
        3 => prop::collection::vec(-1.0e3f32..1.0e3, len),
        1 => (-1.0e3f32..1.0e3).prop_map(move |v| vec![v; len]),
    ]
}

/// `(predicted, actual)` with independently-varying lengths (0..8 each) so
/// `min(predicted.len(), actual.len())` alignment is exercised, including
/// both-empty. `actual`'s peak values are drawn from a mix of generic random,
/// constant, and all-zero series — the degenerate cases the review demanded
/// be included, not excluded.
fn compare_inputs_strategy() -> impl Strategy<Value = (Vec<f32>, Vec<LayerForce>)> {
    (0usize..8, 0usize..8).prop_flat_map(|(plen, alen)| {
        (predicted_strategy(plen), peak_values_strategy(alen)).prop_map(|(pred, peaks)| {
            let actual = peaks
                .into_iter()
                .enumerate()
                .map(|(i, p)| lf(i as u32, p))
                .collect();
            (pred, actual)
        })
    })
}

proptest! {
    /// `layer_count == min(predicted.len(), actual.len())`, non-zero
    /// whenever `Ok`; `normalized_rmse` / `max_abs_error` both in `[0, 1]`
    /// with `normalized_rmse <= max_abs_error`; `correlation` in `[-1, 1]`
    /// within a 1e-9 epsilon (Pearson can exceed 1.0 by float noise); both
    /// peak-layer indices are `Some` and strictly less than `layer_count`.
    #[test]
    fn compare_layer_count_bounds_and_peak_positions((predicted, actual) in compare_inputs_strategy()) {
        let n = predicted.len().min(actual.len());
        match ForceComparator::compare(&predicted, &actual) {
            Ok(r) => {
                prop_assert_eq!(r.layer_count, n);
                prop_assert!(n > 0);

                prop_assert!(r.normalized_rmse >= -1e-9 && r.normalized_rmse <= 1.0 + 1e-9,
                    "normalized_rmse {} outside [0, 1]", r.normalized_rmse);
                prop_assert!(r.max_abs_error >= -1e-9 && r.max_abs_error <= 1.0 + 1e-9,
                    "max_abs_error {} outside [0, 1]", r.max_abs_error);
                prop_assert!(r.normalized_rmse <= r.max_abs_error + 1e-9,
                    "rmse {} > max_abs_error {}", r.normalized_rmse, r.max_abs_error);

                prop_assert!(r.correlation >= -1.0 - 1e-9 && r.correlation <= 1.0 + 1e-9,
                    "correlation {} outside [-1, 1]", r.correlation);

                let ppl = r.predicted_peak_layer.expect("n > 0: argmax_by over a non-empty slice returns Some");
                let apl = r.actual_peak_layer.expect("n > 0: peak_index over a non-empty slice returns Some");
                prop_assert!(ppl < r.layer_count);
                prop_assert!(apl < r.layer_count);
            }
            Err(_) => prop_assert_eq!(n, 0, "compare only documents the empty-aligned-window error path"),
        }
    }

    /// `ProfileOverrides.fit_quality` lies in `[0, 1]`; `calibrate` returns
    /// `Err` EXACTLY when the aligned length is 0 or the sum of squared
    /// actual signals underflows `f64::EPSILON` (the plan's explicit error
    /// characterisation, re-derived here as the thing under test — not a
    /// silent mirror of an unrelated formula).
    #[test]
    fn calibrate_fit_quality_bounds_and_err_condition((predicted, actual) in compare_inputs_strategy()) {
        let n = predicted.len().min(actual.len());
        let denom: f64 = actual.iter().take(n).map(|l| l.peak_signal * l.peak_signal).sum();
        let log = AnalyticLog::default();
        match ProfileCalibrator::calibrate(&predicted, &actual, &log) {
            Ok(o) => {
                prop_assert!(n > 0);
                prop_assert!(denom >= f64::EPSILON);
                prop_assert!(o.fit_quality >= -1e-9 && o.fit_quality <= 1.0 + 1e-9,
                    "fit_quality {} outside [0, 1]", o.fit_quality);
            }
            Err(_) => prop_assert!(n == 0 || denom < f64::EPSILON,
                "calibrate errored but n={n} > 0 and denom={denom} >= EPSILON"),
        }
    }

    /// `delta_t_steady_c` is `Some` iff BOTH `T=7` (resin) and `T=8`
    /// (ambient) channels are present in the log.
    #[test]
    fn calibrate_delta_t_iff_both_temp_channels_present(
        resin in prop::option::of(-50.0f64..150.0),
        ambient in prop::option::of(-50.0f64..150.0),
    ) {
        let mut samples = Vec::new();
        let mut ts = 0u64;
        if let Some(r) = resin {
            samples.push(sample(ts, CH_RESIN_TEMP, r));
            ts += 1;
        }
        if let Some(a) = ambient {
            samples.push(sample(ts, CH_AMBIENT_TEMP, a));
        }
        let log = AnalyticLog { samples };

        // Fixed nonzero predicted/actual so calibrate always succeeds
        // regardless of which temp-channel combination is under test —
        // denom = 10.0*10.0 = 100.0, comfortably >= f64::EPSILON.
        let predicted = vec![10.0_f32];
        let actual = vec![lf(0, 10.0)];
        let o = ProfileCalibrator::calibrate(&predicted, &actual, &log)
            .expect("n=1, denom=100.0 >> f64::EPSILON, always Ok regardless of temp channels");

        match (resin, ambient) {
            (Some(_), Some(_)) => prop_assert!(o.delta_t_steady_c.is_some()),
            _ => prop_assert!(o.delta_t_steady_c.is_none()),
        }
    }
}

// ============================================================================
// Block 4 — filter_layer_range (services::force_series_extractor), plan step
// 6. The one production change in this issue: the CLI's inline `--from/--to`
// layer-range predicate lifted beside `peak_index`, behaviour-preservingly.
// Written RED-first (see module doc). Generic over arbitrary LayerForce
// slices, not just extractor output — the predicate itself doesn't require
// sorted or unique indices, so testing it generically is the honest scope.
// ============================================================================

fn layer_force_strategy() -> impl Strategy<Value = LayerForce> {
    (any::<u32>(), finite_value(), finite_value(), 0usize..20).prop_map(
        |(index, peak_signal, mean_signal, sample_count)| LayerForce {
            index,
            peak_signal,
            mean_signal,
            sample_count,
        },
    )
}

fn layers_vec_strategy() -> impl Strategy<Value = Vec<LayerForce>> {
    prop::collection::vec(layer_force_strategy(), 0..20)
}

fn optional_bound_strategy() -> impl Strategy<Value = Option<u32>> {
    prop::option::of(any::<u32>())
}

proptest! {
    /// The result is an order-preserving subsequence of the input: every
    /// retained item, walked in order, matches an item of `layers` walked in
    /// the same order. Values pass through unchanged (no arithmetic), so
    /// exact equality is safe.
    #[test]
    fn filter_layer_range_is_order_preserving_subsequence(
        layers in layers_vec_strategy(),
        from in optional_bound_strategy(),
        to in optional_bound_strategy(),
    ) {
        let filtered = filter_layer_range(&layers, from, to);
        let mut remaining = layers.iter();
        for f in &filtered {
            let matched = remaining.by_ref().any(|l| l == f);
            prop_assert!(matched, "filtered item {f:?} is not an in-order subsequence of the input");
        }
    }

    /// Every retained layer's index is inside the requested range.
    #[test]
    fn filter_layer_range_retains_only_in_range_indices(
        layers in layers_vec_strategy(),
        from in optional_bound_strategy(),
        to in optional_bound_strategy(),
    ) {
        let filtered = filter_layer_range(&layers, from, to);
        let lo = from.unwrap_or(0);
        let hi = to.unwrap_or(u32::MAX);
        for f in &filtered {
            prop_assert!(f.index >= lo && f.index <= hi);
        }
    }

    /// Widening the range (lower bound down, upper bound up) never drops a
    /// layer that was already retained — monotone under range inclusion.
    #[test]
    fn filter_layer_range_monotone_under_range_widening(
        layers in layers_vec_strategy(),
        from1 in optional_bound_strategy(),
        to1 in optional_bound_strategy(),
        widen_from_by in 0u32..50,
        widen_to_by in 0u32..50,
    ) {
        let from2 = from1.map(|f| f.saturating_sub(widen_from_by));
        let to2 = to1.map(|t| t.saturating_add(widen_to_by));
        let narrow = filter_layer_range(&layers, from1, to1);
        let wide = filter_layer_range(&layers, from2, to2);
        for f in &narrow {
            prop_assert!(wide.contains(f), "widening the range dropped {f:?}");
        }
    }

    /// `from > to` yields an empty result.
    #[test]
    fn filter_layer_range_from_greater_than_to_yields_empty(
        layers in layers_vec_strategy(),
        to in 0u32..1000,
        gap in 1u32..1000,
    ) {
        let from = to + gap;
        let filtered = filter_layer_range(&layers, Some(from), Some(to));
        prop_assert!(filtered.is_empty());
    }

    /// `(None, None)` is the identity.
    #[test]
    fn filter_layer_range_none_none_is_identity(layers in layers_vec_strategy()) {
        let filtered = filter_layer_range(&layers, None, None);
        prop_assert_eq!(filtered, layers);
    }
}
