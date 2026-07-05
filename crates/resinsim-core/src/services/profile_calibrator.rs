//! Derive *suggested* Athena II profile overrides from a real print. ADR-0021.
//!
//! Given the simulated per-layer peel force (Newtons) and the real per-layer
//! peak signal (raw counts), fit the counts→Newton gain by least squares, and
//! read the steady-state temperature delta from the resin/ambient channels.
//!
//! The result is a set of **suggestions with a fit-quality score**, never a
//! silent rewrite of `data/printers/athena_ii.toml`. A single print is a weak
//! calibration sample; `fit_quality` (R², 0..1) tells the caller how much to
//! trust it. Callers surface the values for a human to apply.

use crate::io::athena::{AnalyticLog, CH_AMBIENT_TEMP, CH_RESIN_TEMP};
use crate::services::force_series_extractor::LayerForce;

/// Suggested calibration deltas for a printer profile. Not applied automatically.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileOverrides {
    /// Fitted load-cell counts → Newton gain (multiply raw peel signal by this).
    pub peel_gain_n_per_count: f64,
    /// Steady-state resin−ambient temperature delta (°C), if both channels present.
    pub delta_t_steady_c: Option<f64>,
    /// Goodness of the gain fit, R² in `[0, 1]`.
    pub fit_quality: f64,
}

/// Fits suggested profile overrides from a real print.
pub struct ProfileCalibrator;

impl ProfileCalibrator {
    pub fn calibrate(
        predicted_n: &[f32],
        actual: &[LayerForce],
        log: &AnalyticLog,
    ) -> Result<ProfileOverrides, String> {
        let n = predicted_n.len().min(actual.len());
        if n == 0 {
            return Err("cannot calibrate: predicted or actual series is empty".into());
        }
        let pred: Vec<f64> = predicted_n[..n].iter().map(|&p| p as f64).collect();
        let act: Vec<f64> = actual[..n].iter().map(|l| l.peak_signal).collect();

        // Least-squares gain g minimizing Σ(pred − g·act)²  ⇒ g = Σ(pred·act)/Σ(act²).
        let denom: f64 = act.iter().map(|a| a * a).sum();
        if denom < f64::EPSILON {
            return Err("cannot calibrate: real force signal is all zero".into());
        }
        let gain = pred.iter().zip(&act).map(|(p, a)| p * a).sum::<f64>() / denom;

        // R² of the fit pred ≈ gain·act.
        let mean_pred = pred.iter().sum::<f64>() / n as f64;
        let ss_tot: f64 = pred.iter().map(|p| (p - mean_pred).powi(2)).sum();
        let ss_res: f64 = pred
            .iter()
            .zip(&act)
            .map(|(p, a)| (p - gain * a).powi(2))
            .sum();
        let fit_quality = if ss_tot < f64::EPSILON {
            0.0
        } else {
            (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
        };

        let delta_t = match (
            log.channel_mean(CH_RESIN_TEMP),
            log.channel_mean(CH_AMBIENT_TEMP),
        ) {
            (Some(resin), Some(ambient)) => Some(resin - ambient),
            _ => None,
        };

        Ok(ProfileOverrides {
            peel_gain_n_per_count: gain,
            delta_t_steady_c: delta_t,
            fit_quality,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::athena::parse_analytic;

    fn lf(index: u32, peak: f64) -> LayerForce {
        LayerForce {
            index,
            peak_signal: peak,
            mean_signal: peak,
            sample_count: 1,
        }
    }

    #[test]
    fn fits_gain_that_maps_counts_to_newtons() {
        // predicted = 0.1 × actual exactly → gain 0.1, R² = 1.
        let actual = vec![lf(0, 400.0), lf(1, 250.0), lf(2, 150.0)];
        let predicted = vec![40.0_f32, 25.0, 15.0];
        let log = AnalyticLog::default();
        let o = ProfileCalibrator::calibrate(&predicted, &actual, &log).expect("calibrates");
        assert!(
            (o.peel_gain_n_per_count - 0.1).abs() < 1e-9,
            "gain {}",
            o.peel_gain_n_per_count
        );
        assert!((o.fit_quality - 1.0).abs() < 1e-9, "R² {}", o.fit_quality);
        assert_eq!(o.delta_t_steady_c, None);
    }

    #[test]
    fn reads_delta_t_from_temp_channels() {
        let log = parse_analytic("ID,T,V\n1,7,28.0\n1,8,22.0\n".as_bytes()).expect("parse");
        let actual = vec![lf(0, 400.0)];
        let predicted = vec![40.0_f32];
        let o = ProfileCalibrator::calibrate(&predicted, &actual, &log).expect("calibrates");
        assert_eq!(o.delta_t_steady_c, Some(6.0));
    }

    #[test]
    fn zero_signal_is_error() {
        let actual = vec![lf(0, 0.0), lf(1, 0.0)];
        let predicted = vec![40.0_f32, 25.0];
        let log = AnalyticLog::default();
        assert!(ProfileCalibrator::calibrate(&predicted, &actual, &log).is_err());
    }
}
