//! `LayerInput` — per-layer printable data value object.
//!
//! Per-layer values extracted from a sliced file (CTB, SL1, GOO) or
//! synthesised by test fixtures. Format-independent — the simulation
//! consumes this regardless of source.
//!
//! Originally lived under `io/sliced.rs`; moved to `values/` in the
//! `ctb-layer-height-authority` lifecycle so the dependency direction
//! is consistent (io/ → values/, never the reverse). `io::sliced`
//! re-exports `LayerInput` from here for callers that hold the old
//! import path.
//!
//! Retains flat recipe-shaped fields by design (ADR-0005 §4): per-layer
//! values can legitimately differ from the Recipe default (e.g.
//! transition layers with their own exposure schedule). Collapsing
//! them under Recipe would misrepresent per-layer override semantics.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::values::LayerMask;

/// Per-layer data extracted from a sliced file (CTB, SL1, GOO).
///
/// `mask: Option<LayerMask>` is populated by mask-producing parsers
/// (e.g. the extended CTB parser). Area-only parsers or test fixtures
/// may leave it None; downstream consumers
/// (`SimulationRunner::run_from_areas` adapter) synthesise a trivial
/// fully-solid mask when None is observed.
///
/// The mask is `#[serde(skip)]` — it is large binary data, not
/// meaningful for TOML / JSON persistence. Callers that serialise
/// `LayerInput` (e.g. CLI `--json` output) see the area + recipe fields
/// only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInput {
    pub index: u32,
    pub cross_section_area_mm2: f64,
    pub exposure_sec: f32,
    pub lift_speed_mm_min: f32,
    pub layer_height_um: f32,
    pub z_mm: f32,
    #[serde(skip)]
    pub mask: Option<LayerMask>,
}

impl LayerInput {
    pub fn new(
        index: u32,
        cross_section_area_mm2: f64,
        exposure_sec: f32,
        lift_speed_mm_min: f32,
        layer_height_um: f32,
        z_mm: f32,
    ) -> Result<Self, &'static str> {
        if exposure_sec <= 0.0 {
            return Err("exposure must be positive");
        }
        if cross_section_area_mm2 < 0.0 {
            return Err("area cannot be negative");
        }
        Ok(Self {
            index,
            cross_section_area_mm2,
            exposure_sec,
            lift_speed_mm_min,
            layer_height_um,
            z_mm,
            mask: None,
        })
    }

    /// Attach a `LayerMask` to this input. Chainable builder for
    /// constructors that also want to populate the 3D topology field.
    pub fn with_mask(mut self, mask: LayerMask) -> Self {
        self.mask = Some(mask);
        self
    }
}

impl fmt::Display for LayerInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Layer {}: {:.1} mm², {:.2}s exposure, z={:.3}mm",
            self.index, self.cross_section_area_mm2, self.exposure_sec, self.z_mm
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_input_rejects_zero_exposure() {
        let result = LayerInput::new(0, 100.0, 0.0, 60.0, 50.0, 0.025);
        assert!(result.is_err());
    }

    #[test]
    fn layer_input_rejects_negative_exposure() {
        let result = LayerInput::new(0, 100.0, -1.0, 60.0, 50.0, 0.025);
        assert!(result.is_err());
    }

    #[test]
    fn layer_input_rejects_negative_area() {
        let result = LayerInput::new(0, -10.0, 2.5, 60.0, 50.0, 0.025);
        assert!(result.is_err());
    }

    #[test]
    fn layer_input_accepts_valid() {
        let li = LayerInput::new(0, 100.0, 2.5, 60.0, 50.0, 0.025)
            .expect("test fixture: finite non-negative inputs are in LayerInput::new domain");
        assert_eq!(li.index, 0);
        assert!((li.cross_section_area_mm2 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn layer_input_accepts_zero_area() {
        let li = LayerInput::new(5, 0.0, 2.5, 60.0, 50.0, 0.275);
        assert!(li.is_ok());
    }

    #[test]
    fn layer_input_display() {
        let li = LayerInput::new(42, 256.7, 2.5, 60.0, 50.0, 2.125)
            .expect("test fixture: finite non-negative inputs are in LayerInput::new domain");
        let s = format!("{li}");
        assert!(s.contains("Layer 42"));
        assert!(s.contains("256.7 mm²"));
    }
}
