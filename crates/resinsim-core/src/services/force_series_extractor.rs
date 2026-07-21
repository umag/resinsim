//! Segment a tall Athena analytic log into per-layer real force stats. ADR-0021.
//!
//! The analytic log has no layer column, so layer boundaries are inferred from
//! the layer-height channel ([`CH_LAYER_HEIGHT`]), which NanoDLP emits once at
//! the start of each layer. Every pressure sample ([`CH_PRESSURE`]) seen before
//! the next layer-height marker is attributed to the current layer. This is the
//! documented heuristic; if a print lacks layer-height markers no layer is ever
//! opened and the extractor yields an **empty** series (callers should check
//! `len()`). Pressure samples that arrive *before the first* marker cannot be
//! attributed to any layer and are dropped; [`ForceSeriesExtractor::extract_with_prelude_count`]
//! returns how many, so the drop is a reported diagnostic rather than silent.

use crate::io::athena::{peel_signal, AnalyticLog, CH_LAYER_HEIGHT, CH_PRESSURE};

/// Real peel-force statistics for one printed layer (raw load-cell counts,
/// sign-corrected so peel is positive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerForce {
    pub index: u32,
    pub peak_signal: f64,
    pub mean_signal: f64,
    pub sample_count: usize,
}

/// Segments the analytic log into per-layer force stats using layer-height
/// markers as boundaries.
pub struct ForceSeriesExtractor;

impl ForceSeriesExtractor {
    pub fn extract_layer_forces(log: &AnalyticLog) -> Vec<LayerForce> {
        Self::extract_with_prelude_count(log).0
    }

    /// Like [`extract_layer_forces`](Self::extract_layer_forces), additionally
    /// returning the count of [`CH_PRESSURE`] samples that arrived *before* the
    /// first layer-height marker. Those samples cannot be attributed to any
    /// layer (no layer is open yet) and are dropped from the series; callers
    /// surface the count as a diagnostic rather than let the drop stay silent.
    /// The `Vec<LayerForce>` returned is identical to `extract_layer_forces`.
    pub fn extract_with_prelude_count(log: &AnalyticLog) -> (Vec<LayerForce>, usize) {
        let mut layers: Vec<LayerForce> = Vec::new();
        // Accumulator for the layer currently being filled.
        let mut cur: Option<(u32, Vec<f64>)> = None;
        // Pressure samples seen before any layer-height marker opened a layer.
        let mut prelude = 0usize;

        let finish = |layers: &mut Vec<LayerForce>, idx: u32, vals: Vec<f64>| {
            if vals.is_empty() {
                layers.push(LayerForce {
                    index: idx,
                    peak_signal: 0.0,
                    mean_signal: 0.0,
                    sample_count: 0,
                });
                return;
            }
            let peak = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            layers.push(LayerForce {
                index: idx,
                peak_signal: peak,
                mean_signal: mean,
                sample_count: vals.len(),
            });
        };

        for s in &log.samples {
            match s.channel {
                CH_LAYER_HEIGHT => {
                    // Boundary: flush the previous layer, open the next.
                    if let Some((idx, vals)) = cur.take() {
                        finish(&mut layers, idx, vals);
                    }
                    let next_idx = layers.len() as u32;
                    cur = Some((next_idx, Vec::new()));
                }
                CH_PRESSURE => match cur.as_mut() {
                    Some((_, vals)) => vals.push(peel_signal(s.value)),
                    // No layer open yet — unattributable prelude sample.
                    None => prelude += 1,
                },
                _ => {}
            }
        }
        if let Some((idx, vals)) = cur.take() {
            finish(&mut layers, idx, vals);
        }
        (layers, prelude)
    }
}

/// Core argmax: 0-based position of the item with the greatest `key`, or `None`
/// when `items` is empty. Deterministic and NaN-tolerant via `f64::total_cmp`;
/// ties resolve to the first (earliest) item. The single source of truth for
/// "which position peaks" — [`peak_index`] and [`ForceComparator`] both route
/// through it so predicted/actual/CLI peak semantics never diverge.
///
/// [`ForceComparator`]: crate::services::ForceComparator
pub fn argmax_by<T>(items: &[T], key: impl Fn(&T) -> f64) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .reduce(|acc, cur| {
            // Tie-break to first: only overtake on a *strictly* greater key.
            if key(cur.1).total_cmp(&key(acc.1)) == std::cmp::Ordering::Greater {
                cur
            } else {
                acc
            }
        })
        .map(|(i, _)| i)
}

/// 0-based position of the layer with the greatest `peak_signal`, or `None`
/// when `layers` is empty. Ties resolve to the first (earliest) layer — the
/// earliest-peak convention the peel-force base-adhesion signal cares about.
/// Thin wrapper over [`argmax_by`]; shared by [`ForceComparator`] and
/// `inspect athena` — do not reimplement the argmax.
///
/// [`ForceComparator`]: crate::services::ForceComparator
pub fn peak_index(layers: &[LayerForce]) -> Option<usize> {
    argmax_by(layers, |l| l.peak_signal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::athena::parse_analytic;

    // Three layers; each opens with a T=0 marker then T=6 pressure samples.
    const LOG: &str = "ID,T,V\n\
        1,0,0.05\n1,6,-360\n1,6,-400\n1,6,-360\n\
        2,0,0.10\n2,6,-240\n2,6,-250\n\
        3,0,0.15\n3,6,-150\n3,6,-140\n";

    #[test]
    fn segments_into_three_layers() {
        let log = parse_analytic(LOG.as_bytes()).expect("parse");
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        assert_eq!(forces.len(), 3);
        assert_eq!(forces[0].index, 0);
        assert_eq!(forces[2].index, 2);
    }

    #[test]
    fn peak_and_mean_are_sign_corrected() {
        let log = parse_analytic(LOG.as_bytes()).expect("parse");
        let f = ForceSeriesExtractor::extract_layer_forces(&log);
        // Layer 0 raw −360,−400,−360 → signal 360,400,360; peak 400, mean ~373.3.
        assert!((f[0].peak_signal - 400.0).abs() < 1e-9);
        assert!((f[0].mean_signal - 373.3333).abs() < 1e-3);
        assert_eq!(f[0].sample_count, 3);
        assert!((f[2].peak_signal - 150.0).abs() < 1e-9);
    }

    #[test]
    fn no_layer_markers_yields_empty() {
        let log = parse_analytic("ID,T,V\n1,6,-100\n".as_bytes()).expect("parse");
        let forces = ForceSeriesExtractor::extract_layer_forces(&log);
        assert!(forces.is_empty(), "no T=0 markers → no layers segmented");
    }

    fn lf(index: u32, peak: f64) -> LayerForce {
        LayerForce {
            index,
            peak_signal: peak,
            mean_signal: peak,
            sample_count: 1,
        }
    }

    #[test]
    fn peak_index_returns_position_of_max() {
        let xs = vec![lf(0, 100.0), lf(1, 250.0), lf(2, 400.0), lf(3, 150.0)];
        assert_eq!(peak_index(&xs), Some(2));
    }

    #[test]
    fn peak_index_ties_break_to_first() {
        let xs = vec![lf(0, 400.0), lf(1, 400.0), lf(2, 100.0)];
        assert_eq!(peak_index(&xs), Some(0));
    }

    #[test]
    fn peak_index_none_on_empty() {
        assert_eq!(peak_index(&[]), None);
    }

    #[test]
    fn no_prelude_when_first_sample_is_a_marker() {
        let log = parse_analytic("ID,T,V\n1,0,0.05\n1,6,-100\n1,6,-200\n".as_bytes())
            .expect("parse");
        let (layers, prelude) = ForceSeriesExtractor::extract_with_prelude_count(&log);
        assert_eq!(prelude, 0, "log opens with a marker → no prelude");
        assert_eq!(layers.len(), 1);
    }

    #[test]
    fn prelude_samples_before_first_marker_are_counted() {
        // Two pressure samples arrive before the first T=0 marker → prelude.
        let log = parse_analytic("ID,T,V\n1,6,-100\n1,6,-200\n2,0,0.05\n2,6,-50\n".as_bytes())
            .expect("parse");
        let (layers, prelude) = ForceSeriesExtractor::extract_with_prelude_count(&log);
        assert_eq!(prelude, 2, "two pre-marker pressure samples are prelude");
        assert_eq!(layers.len(), 1, "one marker → one segmented layer");
        assert!((layers[0].peak_signal - 50.0).abs() < 1e-9);
    }
}
