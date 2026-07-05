//! Segment a tall Athena analytic log into per-layer real force stats. ADR-0021.
//!
//! The analytic log has no layer column, so layer boundaries are inferred from
//! the layer-height channel ([`CH_LAYER_HEIGHT`]), which NanoDLP emits once at
//! the start of each layer. Every pressure sample ([`CH_PRESSURE`]) seen before
//! the next layer-height marker is attributed to the current layer. This is the
//! documented heuristic; if a print lacks layer-height markers the extractor
//! yields a single aggregate layer (callers should check `len()`).

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
        let mut layers: Vec<LayerForce> = Vec::new();
        // Accumulator for the layer currently being filled.
        let mut cur: Option<(u32, Vec<f64>)> = None;

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
                CH_PRESSURE => {
                    if let Some((_, vals)) = cur.as_mut() {
                        vals.push(peel_signal(s.value));
                    }
                }
                _ => {}
            }
        }
        if let Some((idx, vals)) = cur.take() {
            finish(&mut layers, idx, vals);
        }
        layers
    }
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
}
