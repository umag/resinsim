//! Compare simulated peel force against the real Athena force log. ADR-0021.
//!
//! The simulated series is in Newtons; the real series is in raw load-cell
//! counts (see [`crate::io::athena`]). Because the absolute counts→Newton gain
//! is unknown until calibration, the comparison is done in a **normalized**
//! space: each series is min-max scaled to `[0, 1]` and the shape agreement is
//! scored (RMSE, max error, Pearson correlation). Absolute-unit fitting is the
//! job of [`ProfileCalibrator`](crate::services::ProfileCalibrator).

use crate::services::force_series_extractor::{argmax_by, peak_index, LayerForce};

/// Result of comparing predicted vs actual per-layer peel force.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonReport {
    pub layer_count: usize,
    /// RMSE between the two min-max-normalized series (0 = perfect shape match).
    pub normalized_rmse: f64,
    /// Largest absolute per-layer error in normalized space.
    pub max_abs_error: f64,
    /// Pearson correlation of the raw (un-normalized) aligned series.
    pub correlation: f64,
    /// 0-based position, within the compared `[..layer_count]` window, of the
    /// predicted series' peak; `None` when the window is empty. A large gap
    /// between this and [`actual_peak_layer`](Self::actual_peak_layer) is the
    /// KB-115 base-adhesion symptom (sim peaks late, real peaks at the base).
    pub predicted_peak_layer: Option<usize>,
    /// 0-based position of the real series' peak within the compared window.
    pub actual_peak_layer: Option<usize>,
}

/// Scores predicted (Newtons) against actual (raw counts) per-layer peel force.
pub struct ForceComparator;

impl ForceComparator {
    /// Compare per-layer predicted peel force (N) against real per-layer peak
    /// signal (counts). Series are aligned by position up to the shorter length.
    pub fn compare(predicted: &[f32], actual: &[LayerForce]) -> Result<ComparisonReport, String> {
        let n = predicted.len().min(actual.len());
        if n == 0 {
            return Err("nothing to compare: predicted or actual series is empty".into());
        }
        let pred: Vec<f64> = predicted[..n].iter().map(|&p| p as f64).collect();
        let act: Vec<f64> = actual[..n].iter().map(|l| l.peak_signal).collect();

        let pred_n = normalize(&pred);
        let act_n = normalize(&act);

        let mut sq_sum = 0.0;
        let mut max_abs = 0.0_f64;
        for i in 0..n {
            let e = (pred_n[i] - act_n[i]).abs();
            sq_sum += e * e;
            max_abs = max_abs.max(e);
        }
        Ok(ComparisonReport {
            layer_count: n,
            normalized_rmse: (sq_sum / n as f64).sqrt(),
            max_abs_error: max_abs,
            correlation: pearson(&pred, &act),
            // Peak positions over the same aligned [..n] window, via the shared
            // argmax core so predicted/actual/CLI peak semantics never diverge.
            predicted_peak_layer: argmax_by(&pred, |&v| v),
            actual_peak_layer: peak_index(&actual[..n]),
        })
    }
}

/// Min-max scale to `[0, 1]`. A constant series maps to all-0.5 (no shape info).
fn normalize(xs: &[f64]) -> Vec<f64> {
    let min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if range.abs() < f64::EPSILON {
        return vec![0.5; xs.len()];
    }
    xs.iter().map(|x| (x - min) / range).collect()
}

/// Pearson correlation coefficient; 0 when either series is constant.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut va = 0.0;
    let mut vb = 0.0;
    for i in 0..a.len() {
        let da = a[i] - ma;
        let db = b[i] - mb;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    if va < f64::EPSILON || vb < f64::EPSILON {
        return 0.0;
    }
    cov / (va.sqrt() * vb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lf(index: u32, peak: f64) -> LayerForce {
        LayerForce {
            index,
            peak_signal: peak,
            mean_signal: peak,
            sample_count: 1,
        }
    }

    #[test]
    fn perfectly_correlated_shapes_score_well() {
        // Predicted (N) is exactly 0.1× the actual (counts) — same shape.
        let actual = vec![lf(0, 400.0), lf(1, 250.0), lf(2, 150.0)];
        let predicted = vec![40.0_f32, 25.0, 15.0];
        let r = ForceComparator::compare(&predicted, &actual).expect("compares");
        assert_eq!(r.layer_count, 3);
        assert!(r.normalized_rmse < 1e-6, "rmse {}", r.normalized_rmse);
        assert!((r.correlation - 1.0).abs() < 1e-9, "corr {}", r.correlation);
    }

    #[test]
    fn anti_correlated_shapes_score_poorly() {
        let actual = vec![lf(0, 100.0), lf(1, 200.0), lf(2, 300.0)];
        let predicted = vec![30.0_f32, 20.0, 10.0];
        let r = ForceComparator::compare(&predicted, &actual).expect("compares");
        assert!(
            r.correlation < 0.0,
            "expected negative corr, got {}",
            r.correlation
        );
    }

    #[test]
    fn mismatched_lengths_align_to_shorter() {
        let actual = vec![lf(0, 400.0), lf(1, 250.0)];
        let predicted = vec![40.0_f32, 25.0, 15.0, 5.0];
        let r = ForceComparator::compare(&predicted, &actual).expect("compares");
        assert_eq!(r.layer_count, 2);
    }

    #[test]
    fn empty_is_error() {
        assert!(ForceComparator::compare(&[], &[]).is_err());
    }

    #[test]
    fn reports_predicted_and_actual_peak_layers() {
        // Real (counts) peaks at layer 0 (base adhesion); predicted (N) peaks
        // mid-print at layer 2 — the KB-115 peak-layer offset.
        let actual = vec![lf(0, 400.0), lf(1, 250.0), lf(2, 150.0)];
        let predicted = vec![10.0_f32, 25.0, 60.0];
        let r = ForceComparator::compare(&predicted, &actual).expect("compares");
        assert_eq!(r.predicted_peak_layer, Some(2));
        assert_eq!(r.actual_peak_layer, Some(0));
    }

    #[test]
    fn peak_layer_is_within_the_compared_window() {
        // predicted's global max is the ignored tail (index 3); only 3 layers
        // are compared, so the reported peak must fall within [..3].
        let actual = vec![lf(0, 100.0), lf(1, 300.0), lf(2, 120.0)];
        let predicted = vec![5.0_f32, 40.0, 9.0, 999.0];
        let r = ForceComparator::compare(&predicted, &actual).expect("compares");
        assert_eq!(r.layer_count, 3);
        assert_eq!(r.predicted_peak_layer, Some(1), "within window, not the tail");
        assert_eq!(r.actual_peak_layer, Some(1));
    }
}
