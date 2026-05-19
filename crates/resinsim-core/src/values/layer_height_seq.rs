//! `LayerHeightSeq` — per-layer CTB layer-height value object.
//!
//! ## DDD shape
//!
//! Wraps `Vec<f32>` as a domain value object rather than passing the raw
//! container around the simulation. Validates on construction
//! (non-empty, each entry finite and > 0), provides the small set of
//! queries the runtime actually performs (`uniform`, `min`, `max`,
//! `mean`, `len`, `get`, `as_slice`), and gives the type system the
//! "every entry is a valid layer thickness" invariant for free.
//!
//! The constructor `LayerHeightSeq::from_layer_inputs(&[LayerInput])`
//! is the bridge between the file-axis I/O type (`LayerInput`) and the
//! runtime value. It's the only public path from raw per-layer data to
//! a `LayerHeightSeq` — direct callers go through
//! `LayerHeightSeq::try_from_vec` which validates the same way.
//!
//! ## Tolerance
//!
//! [`Self::is_uniform`] uses
//! `LayerHeightSeq::UNIFORMITY_TOL_UM` = 1 nm. CTB headers encode
//! layer-height as a little-endian f32 and printer-mechanical
//! precision is several µm; 1 nm sits well below any plausible
//! real disagreement and far above f32 rounding noise.

use serde::{Deserialize, Serialize};

use crate::values::LayerInput;

/// Per-layer CTB layer-height series. Every entry is finite and > 0 by
/// construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "Vec<f32>", into = "Vec<f32>")]
pub struct LayerHeightSeq(Vec<f32>);

impl LayerHeightSeq {
    /// Approx tolerance used to decide whether the series is uniform.
    pub const UNIFORMITY_TOL_UM: f32 = 1e-3;

    /// Construct from a raw `Vec<f32>`. Rejects empty input and any
    /// non-finite or non-positive entry per the
    /// `rust-nan-positive-validation-gap` anti-pattern (a bare `> 0.0`
    /// check passes NaN silently because NaN > 0 is false).
    pub fn try_from_vec(heights: Vec<f32>) -> Result<Self, String> {
        if heights.is_empty() {
            return Err("LayerHeightSeq: cannot construct from empty Vec".to_string());
        }
        for (i, h) in heights.iter().enumerate() {
            if !h.is_finite() || *h <= 0.0 {
                return Err(format!(
                    "LayerHeightSeq: entry {i} = {h} is not finite-and-positive"
                ));
            }
        }
        Ok(Self(heights))
    }

    /// Construct from a parsed CTB / sliced-file `LayerInput` stack.
    /// Bridges the file-axis I/O type to the runtime value object.
    /// Per-entry validation matches [`Self::try_from_vec`]; on
    /// invalid input the error names the offending `LayerInput.index`
    /// (rather than the position in the Vec).
    pub fn from_layer_inputs(layers: &[LayerInput]) -> Result<Self, String> {
        if layers.is_empty() {
            return Err("LayerHeightSeq: cannot construct from empty LayerInput slice".to_string());
        }
        let mut heights = Vec::with_capacity(layers.len());
        for li in layers {
            let h = li.layer_height_um;
            if !h.is_finite() || h <= 0.0 {
                return Err(format!(
                    "layer {} has invalid layer_height_um = {h} (must be finite and > 0)",
                    li.index
                ));
            }
            heights.push(h);
        }
        Ok(Self(heights))
    }

    /// Number of layers in the series. Always >= 1 — the type
    /// invariant (rejected at construction) means this never returns 0.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always returns `false` — `LayerHeightSeq` rejects empty input on
    /// construction, so the invariant guarantees non-emptiness. Provided
    /// for clippy's `len_without_is_empty` lint and as a paired API; a
    /// caller that needs the runtime check can use it but the return
    /// value is statically known.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Borrow as a `&[f32]` for callers that want to iterate or pass
    /// to functions taking a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    /// Get the height at a specific layer index. `None` if out of range.
    pub fn get(&self, layer_index: usize) -> Option<f32> {
        self.0.get(layer_index).copied()
    }

    /// `Some(uniform_um)` when every entry agrees with the first within
    /// [`Self::UNIFORMITY_TOL_UM`]; `None` when at least one entry
    /// differs (adaptive / variable layer height slicing).
    pub fn uniform(&self) -> Option<f32> {
        let first = *self.0.first()?;
        for h in self.0.iter().skip(1) {
            if (*h - first).abs() > Self::UNIFORMITY_TOL_UM {
                return None;
            }
        }
        Some(first)
    }

    /// Minimum layer thickness across the series, in micrometres.
    pub fn min_um(&self) -> f32 {
        self.0.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// Maximum layer thickness across the series, in micrometres.
    pub fn max_um(&self) -> f32 {
        self.0.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }

    /// Arithmetic mean of the per-layer heights, in micrometres.
    /// Computed in f64 to limit rounding noise on large prints.
    pub fn mean_um(&self) -> f32 {
        let total: f64 = self.0.iter().map(|h| *h as f64).sum();
        (total / self.0.len() as f64) as f32
    }

    /// Total Z-extent (sum of all layer heights) in micrometres.
    /// Computed in f64.
    pub fn total_z_um(&self) -> f64 {
        self.0.iter().map(|h| *h as f64).sum()
    }
}

impl TryFrom<Vec<f32>> for LayerHeightSeq {
    type Error = String;
    fn try_from(v: Vec<f32>) -> Result<Self, Self::Error> {
        Self::try_from_vec(v)
    }
}

impl From<LayerHeightSeq> for Vec<f32> {
    fn from(seq: LayerHeightSeq) -> Self {
        seq.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from_vec_uniform_constructs() {
        let s = LayerHeightSeq::try_from_vec(vec![40.0; 4]).expect("uniform 40 µm valid");
        assert_eq!(s.len(), 4);
        assert_eq!(s.uniform(), Some(40.0));
    }

    #[test]
    fn try_from_vec_variable_constructs() {
        let s = LayerHeightSeq::try_from_vec(vec![50.0, 30.0, 20.0, 30.0, 50.0])
            .expect("variable adaptive slicing is valid");
        assert_eq!(s.len(), 5);
        assert_eq!(s.uniform(), None);
        assert!((s.min_um() - 20.0).abs() < 1e-6);
        assert!((s.max_um() - 50.0).abs() < 1e-6);
        assert!((s.mean_um() - 36.0).abs() < 1e-6);
        assert!((s.total_z_um() - 180.0).abs() < 1e-9);
    }

    #[test]
    fn uniform_within_tolerance() {
        // 0.5 nm wobble — inside the 1 nm tolerance.
        let s = LayerHeightSeq::try_from_vec(vec![40.0, 40.0 + 5e-4, 40.0 - 5e-4])
            .expect("valid heights");
        assert!(
            (s.uniform().expect("within-tol noise is uniform") - 40.0).abs() < 1e-3,
            "uniform value: {:?}",
            s.uniform()
        );
    }

    #[test]
    fn try_from_vec_rejects_empty() {
        let err = LayerHeightSeq::try_from_vec(vec![]).expect_err("empty must reject");
        assert!(err.contains("empty"), "err: {err}");
    }

    #[test]
    fn try_from_vec_rejects_zero() {
        assert!(LayerHeightSeq::try_from_vec(vec![0.0]).is_err());
    }

    #[test]
    fn try_from_vec_rejects_negative() {
        assert!(LayerHeightSeq::try_from_vec(vec![-40.0]).is_err());
    }

    #[test]
    fn try_from_vec_rejects_nan() {
        let err = LayerHeightSeq::try_from_vec(vec![f32::NAN]).expect_err("NaN must reject");
        assert!(err.contains("finite-and-positive"), "err: {err}");
    }

    #[test]
    fn try_from_vec_rejects_infinity() {
        assert!(LayerHeightSeq::try_from_vec(vec![f32::INFINITY]).is_err());
        assert!(LayerHeightSeq::try_from_vec(vec![f32::NEG_INFINITY]).is_err());
    }

    #[test]
    fn try_from_vec_rejects_nan_at_later_position() {
        let err = LayerHeightSeq::try_from_vec(vec![40.0, 40.0, f32::NAN, 40.0])
            .expect_err("NaN at any position must error");
        assert!(err.contains("entry 2"), "err: {err}");
    }

    #[test]
    fn try_from_vec_rejects_negative_zero() {
        // -0.0 is finite but <= 0.0 — must reject.
        assert!(LayerHeightSeq::try_from_vec(vec![-0.0_f32]).is_err());
    }

    #[test]
    fn get_returns_per_layer_value() {
        let s = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0]).expect("valid");
        assert_eq!(s.get(0), Some(30.0));
        assert_eq!(s.get(2), Some(50.0));
        assert_eq!(s.get(3), None);
    }

    #[test]
    fn as_slice_borrows_inner_vec() {
        let s = LayerHeightSeq::try_from_vec(vec![30.0, 40.0]).expect("valid");
        assert_eq!(s.as_slice(), &[30.0, 40.0]);
    }

    // ---- from_layer_inputs ----

    fn li_with_h(index: u32, h: f32) -> LayerInput {
        let mut li = LayerInput::new(index, 100.0, 2.5, 60.0, h, index as f32 * h / 1000.0)
            .expect("test fixture: finite non-negative inputs");
        li.layer_height_um = h;
        li
    }

    #[test]
    fn from_layer_inputs_uniform() {
        let layers: Vec<LayerInput> = (0..4).map(|i| li_with_h(i, 40.0)).collect();
        let s = LayerHeightSeq::from_layer_inputs(&layers).expect("uniform layers are valid");
        assert_eq!(s.as_slice(), &[40.0; 4]);
    }

    #[test]
    fn from_layer_inputs_variable_preserves_per_layer_values() {
        let layers = vec![
            li_with_h(0, 50.0),
            li_with_h(1, 30.0),
            li_with_h(2, 20.0),
            li_with_h(3, 30.0),
            li_with_h(4, 50.0),
        ];
        let s = LayerHeightSeq::from_layer_inputs(&layers).expect("variable is valid");
        assert_eq!(s.as_slice(), &[50.0, 30.0, 20.0, 30.0, 50.0]);
    }

    #[test]
    fn from_layer_inputs_rejects_empty() {
        let layers: Vec<LayerInput> = vec![];
        let err = LayerHeightSeq::from_layer_inputs(&layers).expect_err("empty LayerInput slice");
        assert!(err.contains("empty"), "err: {err}");
    }

    #[test]
    fn from_layer_inputs_rejects_nan_naming_layer_index() {
        let mut layers: Vec<LayerInput> = (0..5).map(|i| li_with_h(i, 40.0)).collect();
        layers[3] = li_with_h(3, f32::NAN);
        let err = LayerHeightSeq::from_layer_inputs(&layers).expect_err("NaN must reject");
        assert!(err.contains("layer 3"), "err: {err}");
        assert!(err.contains("finite"), "err: {err}");
    }

    // ---- serde round-trip ----

    #[test]
    fn serde_round_trip_via_vec_f32() {
        let s = LayerHeightSeq::try_from_vec(vec![30.0, 40.0, 50.0]).expect("valid");
        let j = serde_json::to_value(&s).expect("Serialize via Into<Vec<f32>> infallible");
        // Serialised as a plain JSON array (Vec<f32> shape) thanks to the
        // `#[serde(into = "Vec<f32>")]` attribute.
        assert!(j.is_array(), "expected JSON array, got {j}");
        let s2: LayerHeightSeq = serde_json::from_value(j).expect("valid round-trip");
        assert_eq!(s2.as_slice(), s.as_slice());
    }

    /// Canonical serde-side NaN/non-positive regression guard
    /// (harvest UAT-C, ctb-layer-height-authority). The
    /// `#[serde(try_from = "Vec<f32>")]` attribute routes Deserialize
    /// through the validating constructor — this test pins that
    /// behaviour so a future drift (e.g. someone switching to a
    /// derive-Deserialize that bypasses validation) panics loudly.
    /// Production callers that arrive at LayerHeightSeq via Deserialize
    /// (e.g. from a sim.json file) get the same invariant as
    /// `try_from_vec` constructors.
    #[test]
    fn serde_deserialize_rejects_invalid_vec() {
        // try_from = "Vec<f32>" runs the validating constructor.
        let j = serde_json::json!([40.0, -1.0, 40.0]);
        let r: Result<LayerHeightSeq, _> = serde_json::from_value(j);
        assert!(r.is_err(), "Deserialize must reject invalid entries");
    }
}
