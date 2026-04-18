use std::fmt;

use serde::{Deserialize, Serialize};

/// Per-layer data extracted from a sliced file (CTB, SL1, GOO).
/// Format-independent — the simulation consumes this regardless of source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInput {
    pub index: u32,
    pub cross_section_area_mm2: f64,
    pub exposure_sec: f32,
    pub lift_speed_mm_min: f32,
    pub layer_height_um: f32,
    pub z_mm: f32,
}

/// Header metadata extracted from a sliced file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicedFileInfo {
    pub format: String,
    pub total_layers: u32,
    pub layer_height_um: f32,
    pub resolution_xy: (u32, u32),
    pub pixel_size_um: (f32, f32),
    pub bed_size_mm: (f32, f32),
    pub normal_exposure_sec: f32,
    pub bottom_exposure_sec: f32,
    pub bottom_layer_count: u32,
    pub lift_speed_mm_min: f32,
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
        })
    }
}

impl SlicedFileInfo {
    /// Compute physical area per pixel in mm².
    pub fn pixel_area_mm2(&self) -> f64 {
        let px_w = self.bed_size_mm.0 / self.resolution_xy.0 as f32;
        let px_h = self.bed_size_mm.1 / self.resolution_xy.1 as f32;
        px_w as f64 * px_h as f64
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
            self.layer_height_um,
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
    fn sliced_file_info_pixel_area() {
        // Saturn 2: 219mm × 123mm, 7680×4320 px
        let info = SlicedFileInfo {
            format: "CTB".into(),
            total_layers: 1000,
            layer_height_um: 50.0,
            resolution_xy: (7680, 4320),
            pixel_size_um: (28.5, 28.5),
            bed_size_mm: (219.0, 123.0),
            normal_exposure_sec: 2.5,
            bottom_exposure_sec: 25.0,
            bottom_layer_count: 6,
            lift_speed_mm_min: 60.0,
        };
        // pixel area = (219/7680) × (123/4320) = 0.02852 × 0.02847 = 0.000812 mm²
        let pa = info.pixel_area_mm2();
        assert!((pa - 0.000812).abs() < 0.00001,
            "pixel area: got {pa:.6}");
    }

    #[test]
    fn detect_ctb() {
        assert_eq!(detect_format(std::path::Path::new("model.ctb")), Some("CTB"));
    }

    #[test]
    fn detect_stl() {
        assert_eq!(detect_format(std::path::Path::new("model.stl")), Some("STL"));
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format(std::path::Path::new("model.xyz")), None);
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
