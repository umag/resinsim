use std::fmt;

use serde::{Deserialize, Serialize};

use crate::entities::Recipe;

// `LayerInput` lives in `crate::values::layer_input` per the
// ctb-layer-height-authority round-2 cleanup; re-exported here so
// `crate::io::sliced::LayerInput` paths still resolve for callers
// outside this crate (resinsim-viz, resinsim-inspect, integration
// tests). The canonical import path within resinsim-core is
// `crate::values::LayerInput`.
pub use crate::values::LayerInput;

/// Header metadata extracted from a sliced file.
///
/// Recipe-shaped fields (layer_height, exposure times, lift speed, bottom-layer count)
/// are nested in `recipe: Recipe` per ADR-0005 §4. File-level metadata (format,
/// layer count, resolution, pixel size, bed size) stays flat because it describes the
/// sliced file itself, not the print recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicedFileInfo {
    pub format: String,
    pub total_layers: u32,
    pub resolution_xy: (u32, u32),
    pub pixel_size_um: (f32, f32),
    pub bed_size_mm: (f32, f32),
    /// Recipe extracted from the sliced file header (ADR-0005 Axis 2b). CTB format
    /// carries layer_height, normal/bottom exposure, bottom-layer count, and lift
    /// speed; fields not present in the format (transition_layers, wait_*,
    /// lift_cycle_sec, lift_distance_mm) take documented defaults in the parser.
    pub recipe: Recipe,
}

impl SlicedFileInfo {
    /// Compute physical area per pixel in mm².
    pub fn pixel_area_mm2(&self) -> f64 {
        let px_w = self.bed_size_mm.0 / self.resolution_xy.0 as f32;
        let px_h = self.bed_size_mm.1 / self.resolution_xy.1 as f32;
        px_w as f64 * px_h as f64
    }
}

impl fmt::Display for SlicedFileInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} layers, {}×{} px, {:.0}×{:.0} mm bed, {:.1}µm layers",
            self.format,
            self.total_layers,
            self.resolution_xy.0,
            self.resolution_xy.1,
            self.bed_size_mm.0,
            self.bed_size_mm.1,
            self.recipe.layer_height_um(),
        )
    }
}

/// Detect sliced file format from file extension.
pub fn detect_format(path: &std::path::Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "ctb" => Some("CTB"),
        "sl1" => Some("SL1"),
        "goo" => Some("GOO"),
        "stl" => Some("STL"),
        "3mf" => Some("3MF"),
        "voxl" => Some("VOXL"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // LayerInput unit tests live in `crate::values::layer_input::tests`
    // alongside the struct definition (moved 2026-05-19 per ticket
    // `ctb-layer-height-authority` round-2 code review LOW).

    #[test]
    fn sliced_file_info_pixel_area() {
        // Saturn 2: 219mm × 123mm, 7680×4320 px
        let info = SlicedFileInfo {
            format: "CTB".into(),
            total_layers: 1000,
            resolution_xy: (7680, 4320),
            pixel_size_um: (28.5, 28.5),
            bed_size_mm: (219.0, 123.0),
            recipe: Recipe::generic_standard(),
        };
        // pixel area = (219/7680) × (123/4320) = 0.02852 × 0.02847 = 0.000812 mm²
        let pa = info.pixel_area_mm2();
        assert!((pa - 0.000812).abs() < 0.00001, "pixel area: got {pa:.6}");
    }

    #[test]
    fn detect_ctb() {
        assert_eq!(
            detect_format(std::path::Path::new("model.ctb")),
            Some("CTB")
        );
    }

    #[test]
    fn detect_stl() {
        assert_eq!(
            detect_format(std::path::Path::new("model.stl")),
            Some("STL")
        );
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format(std::path::Path::new("model.xyz")), None);
    }

    // Per-layer-height extraction + uniformity detection live in
    // `crate::values::layer_height_seq::LayerHeightSeq` (round-1 code
    // review HIGH — module dependency direction). See those tests for
    // coverage.
}
