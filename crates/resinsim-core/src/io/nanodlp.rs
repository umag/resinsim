//! NanoDLP (Athena II) sliced-job reader. See ADR-0021.
//!
//! A `.nanodlp` file is a ZIP archive of per-layer slice PNGs (`{n}.png`,
//! 1-indexed) plus JSON metadata (`meta.json`, `profile.json`, `slicer.json`,
//! `plate.json`, `info.json`) and a gzipped analytic log
//! (`analytic-*.csv.gz`; parsed separately by [`crate::io::athena`]).
//!
//! `parse_nanodlp` mirrors [`crate::io::ctb::parse_ctb`]: it returns the same
//! `(SlicedFileInfo, Vec<LayerInput>)` pair so the simulation engine consumes a
//! NanoDLP job exactly like a CTB one. The job's per-layer cross-section is
//! decoded from the slice PNG (native-pixel area) and downsampled to a
//! [`LayerMask`] voxel grid for topology analysis.
//!
//! # Untrusted input
//!
//! A `.nanodlp` is an arbitrary user-supplied ZIP. Every decompression path is
//! bounded to fail closed against zip-slip and decompression/dimension bombs:
//! entry count ([`MAX_ENTRIES`]), per-entry JSON size ([`MAX_JSON_BYTES`]),
//! and decoded PNG pixel count ([`MAX_PNG_PIXELS`], checked from the IHDR
//! header *before* any pixel buffer is allocated).

use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::entities::Recipe;
use crate::io::sliced::SlicedFileInfo;
use crate::values::{LayerInput, LayerMask, DEFAULT_VOXEL_SIZE_MM};

// --- Untrusted-archive bounds (ADR-0021 §Security) ---

/// Maximum number of entries in the ZIP container (zip-bomb guard).
pub const MAX_ENTRIES: usize = 100_000;
/// Maximum decompressed size of any single JSON metadata entry.
pub const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum decoded pixel count of a single layer PNG. Athena II is
/// 11520×5120 ≈ 59 MP; 64 MP leaves headroom while rejecting dimension bombs.
pub const MAX_PNG_PIXELS: u64 = 64_000_000;
/// Grayscale value at or above which a pixel counts as cured resin (part).
const LIT_THRESHOLD: u8 = 128;

// --- JSON shapes (only the fields resinsim reads) ---

#[derive(Debug, Deserialize)]
struct NanoDlpMeta {
    #[serde(default)]
    distro: String,
    #[serde(default)]
    program: String,
}

#[derive(Debug, Deserialize)]
struct NanoDlpProfile {
    #[serde(rename = "CureTime")]
    cure_time: f32,
    #[serde(rename = "SupportCureTime", default)]
    support_cure_time: f32,
    #[serde(rename = "SupportLayerNumber", default)]
    support_layer_number: u32,
    #[serde(rename = "LiftSpeed")]
    lift_speed: f32,
    #[serde(rename = "RetractSpeed", default)]
    retract_speed: f32,
    #[serde(rename = "WaitBeforePrint", default)]
    wait_before_print: f32,
    #[serde(rename = "WaitAfterPrint", default)]
    wait_after_print: f32,
    #[serde(rename = "WaitHeight", default)]
    wait_height: f32,
    #[serde(rename = "TransitionalLayer", default)]
    transitional_layer: u32,
    /// Layer thickness fallback when slicer.json omits it (µm).
    #[serde(rename = "Depth", default)]
    depth_um: f32,
}

#[derive(Debug, Deserialize)]
struct NanoDlpSlicer {
    #[serde(rename = "PWidth")]
    p_width: u32,
    #[serde(rename = "PHeight")]
    p_height: u32,
    #[serde(rename = "Thickness", default)]
    thickness_um: f32,
    #[serde(rename = "XPixelSize")]
    x_pixel_size_mm: f32,
    #[serde(rename = "YPixelSize")]
    y_pixel_size_mm: f32,
}

#[derive(Debug, Deserialize)]
struct NanoDlpPlate {
    #[serde(rename = "LayersCount", default)]
    layers_count: u32,
}

/// Build a `Recipe` from NanoDLP metadata. Fields NanoDLP does not separate
/// (wait_before_release, lift_cycle) take documented defaults, mirroring
/// `ctb_recipe`. Per ADR-0005 §4 / ADR-0021.
fn nanodlp_recipe(profile: &NanoDlpProfile, layer_height_um: f32) -> Result<Recipe, String> {
    // NanoDLP profiles frequently store 0 for SupportCureTime meaning "same as
    // normal"; fall back so the bottom layers aren't left with zero exposure.
    let bottom_exposure = if profile.support_cure_time > 0.0 {
        profile.support_cure_time
    } else {
        profile.cure_time
    };
    let retract = if profile.retract_speed > 0.0 {
        Some(profile.retract_speed)
    } else {
        None
    };
    Recipe::new(
        layer_height_um,
        profile.support_layer_number,
        profile.transitional_layer,
        profile.cure_time,
        bottom_exposure,
        profile.wait_before_print,
        0.0, // wait_before_release_sec — NanoDLP does not separate
        profile.wait_after_print,
        profile.lift_speed,
        7.5, // lift_cycle_sec — NanoDLP does not carry; ctb-consistent default
        if profile.wait_height > 0.0 {
            profile.wait_height
        } else {
            5.0
        },
        retract,
    )
}

/// Read a bounded entry from the archive into a `String` (JSON metadata).
fn read_json_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> Result<String, String> {
    let entry = archive
        .by_name(name)
        .map_err(|e| format!("NanoDLP archive missing {name}: {e}"))?;
    if entry.size() > MAX_JSON_BYTES {
        return Err(format!(
            "NanoDLP {name} is {} bytes, exceeds the {MAX_JSON_BYTES}-byte limit",
            entry.size()
        ));
    }
    let mut s = String::new();
    entry
        .take(MAX_JSON_BYTES)
        .read_to_string(&mut s)
        .map_err(|e| format!("NanoDLP {name} read failed: {e}"))?;
    Ok(s)
}

/// Decode one slice PNG into (lit-pixel count, voxel mask). Bounds the declared
/// dimensions before allocating. `bed_*_mm` size the voxel grid.
fn decode_layer_png(
    bytes: &[u8],
    bed_x_mm: f32,
    bed_y_mm: f32,
) -> Result<(u64, LayerMask), String> {
    let mut decoder = png::Decoder::new(bytes);
    // Normalize any color type so the first output byte is an 8-bit luma/red
    // channel: EXPAND lifts palette→RGB and sub-8-bit grayscale→8-bit; STRIP_16
    // drops 16-bit samples to 8-bit. Without this the occupancy read below would
    // misinterpret a palette index or a 16-bit high byte on non-grayscale slices.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG header decode failed: {e}"))?;
    let (w, h) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if (w as u64) * (h as u64) > MAX_PNG_PIXELS {
        return Err(format!(
            "PNG layer is {w}×{h} = {} px, exceeds the {MAX_PNG_PIXELS}-px limit (dimension bomb?)",
            w as u64 * h as u64
        ));
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG pixel decode failed: {e}"))?;
    let pixel_count = frame.width as usize * frame.height as usize;
    if pixel_count == 0 {
        return Err("PNG frame has zero pixels".into());
    }
    let bytes_per_pixel = frame.buffer_size() / pixel_count;
    if bytes_per_pixel == 0 {
        return Err("PNG frame has zero-size pixels".into());
    }

    let voxel = DEFAULT_VOXEL_SIZE_MM;
    let cells_x = ((bed_x_mm / voxel).ceil() as u32).max(1);
    let cells_y = ((bed_y_mm / voxel).ceil() as u32).max(1);
    let mut mask = LayerMask::new(cells_x, cells_y, voxel)
        .map_err(|e| format!("layer mask alloc failed: {e}"))?;

    let px_w_mm = bed_x_mm / w as f32;
    let px_h_mm = bed_y_mm / h as f32;
    let data = &buf[..frame.buffer_size()];
    let mut lit: u64 = 0;
    for y in 0..frame.height as usize {
        let row = y * frame.width as usize * bytes_per_pixel;
        for x in 0..frame.width as usize {
            // First channel (luma / red) is the occupancy signal for both
            // grayscale and RGB(A) slices.
            if data[row + x * bytes_per_pixel] >= LIT_THRESHOLD {
                lit += 1;
                let cx = ((x as f32 * px_w_mm) / voxel) as u32;
                let cy = ((y as f32 * px_h_mm) / voxel) as u32;
                let _ = mask.set(cx.min(cells_x - 1), cy.min(cells_y - 1));
            }
        }
    }
    Ok((lit, mask))
}

/// Parse a `.nanodlp` archive into a sliced-file header + per-layer inputs.
/// Mirrors [`crate::io::ctb::parse_ctb`].
pub fn parse_nanodlp(path: &Path) -> Result<(SlicedFileInfo, Vec<LayerInput>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("failed to open NanoDLP: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a valid NanoDLP (zip): {e}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(format!(
            "NanoDLP archive has {} entries, exceeds the {MAX_ENTRIES} limit",
            archive.len()
        ));
    }

    let meta: NanoDlpMeta = serde_json::from_str(&read_json_entry(&mut archive, "meta.json")?)
        .map_err(|e| format!("meta.json parse: {e}"))?;
    if !meta.program.eq_ignore_ascii_case("nanodlp") && !meta.distro.eq_ignore_ascii_case("athena")
    {
        return Err(format!(
            "not a NanoDLP job (meta.program={:?}, distro={:?})",
            meta.program, meta.distro
        ));
    }

    let slicer: NanoDlpSlicer =
        serde_json::from_str(&read_json_entry(&mut archive, "slicer.json")?)
            .map_err(|e| format!("slicer.json parse: {e}"))?;
    let profile: NanoDlpProfile =
        serde_json::from_str(&read_json_entry(&mut archive, "profile.json")?)
            .map_err(|e| format!("profile.json parse: {e}"))?;
    let plate: NanoDlpPlate = serde_json::from_str(&read_json_entry(&mut archive, "plate.json")?)
        .map_err(|e| format!("plate.json parse: {e}"))?;

    let layer_height_um = if slicer.thickness_um > 0.0 {
        slicer.thickness_um
    } else {
        profile.depth_um
    };
    if layer_height_um <= 0.0 {
        return Err(
            "NanoDLP layer thickness missing/zero (slicer.Thickness, profile.Depth)".into(),
        );
    }
    if slicer.p_width == 0 || slicer.p_height == 0 {
        return Err("NanoDLP resolution missing (slicer.PWidth/PHeight)".into());
    }

    let bed_x_mm = slicer.p_width as f32 * slicer.x_pixel_size_mm;
    let bed_y_mm = slicer.p_height as f32 * slicer.y_pixel_size_mm;
    let pixel_area_mm2 = (slicer.x_pixel_size_mm as f64) * (slicer.y_pixel_size_mm as f64);
    let layer_height_mm = layer_height_um / 1000.0;

    let total_layers = if plate.layers_count > 0 {
        plate.layers_count
    } else {
        // Fall back to counting {n}.png entries.
        (1..)
            .take_while(|i| archive.by_name(&format!("{i}.png")).is_ok())
            .count() as u32
    };

    let recipe = nanodlp_recipe(&profile, layer_height_um)
        .map_err(|e| format!("NanoDLP recipe invalid: {e}"))?;
    // Pull the per-layer values out before `recipe` is moved into the header.
    let bottom_layers = recipe.bottom_layer_count;
    let lift_speed = recipe.lift_speed_mm_min;
    let bottom_exposure_sec = recipe.bottom_exposure_sec;
    let normal_exposure_sec = recipe.normal_exposure_sec;

    let info = SlicedFileInfo {
        format: "NANODLP".into(),
        total_layers,
        resolution_xy: (slicer.p_width, slicer.p_height),
        pixel_size_um: (
            slicer.x_pixel_size_mm * 1000.0,
            slicer.y_pixel_size_mm * 1000.0,
        ),
        bed_size_mm: (bed_x_mm, bed_y_mm),
        recipe,
    };

    let mut layers = Vec::with_capacity(total_layers as usize);
    for i in 0..total_layers {
        let png_name = format!("{}.png", i + 1);
        let mut png_bytes = Vec::new();
        {
            let mut entry = archive
                .by_name(&png_name)
                .map_err(|e| format!("NanoDLP missing layer {png_name}: {e}"))?;
            entry
                .read_to_end(&mut png_bytes)
                .map_err(|e| format!("NanoDLP layer {png_name} read: {e}"))?;
        }
        let (lit, mask) = decode_layer_png(&png_bytes, bed_x_mm, bed_y_mm)
            .map_err(|e| format!("NanoDLP layer {png_name}: {e}"))?;
        let area_mm2 = lit as f64 * pixel_area_mm2;
        let exposure_sec = if i < bottom_layers {
            bottom_exposure_sec
        } else {
            normal_exposure_sec
        };
        let layer = LayerInput::new(
            i,
            area_mm2,
            exposure_sec,
            lift_speed,
            layer_height_um,
            (i + 1) as f32 * layer_height_mm,
        )
        .map_err(|e| format!("NanoDLP layer {i} invalid: {e}"))?
        .with_mask(mask);
        layers.push(layer);
    }

    Ok((info, layers))
}

/// Load the embedded real force log from a `.nanodlp` archive. When several
/// `analytic-*.csv.gz` entries exist (multiple print runs), the
/// lexicographically-last — the most recent by timestamped filename — is used.
/// Decompression is bounded ([`crate::io::athena::MAX_ANALYTIC_DECOMPRESSED`]).
pub fn load_analytic_from_nanodlp(path: &Path) -> Result<crate::io::athena::AnalyticLog, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("failed to open NanoDLP: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a valid NanoDLP (zip): {e}"))?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name();
            if name.starts_with("analytic-") && name.ends_with(".csv.gz") {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    let name = names
        .last()
        .ok_or("no analytic-*.csv.gz entry in NanoDLP archive")?
        .clone();

    let entry = archive
        .by_name(&name)
        .map_err(|e| format!("NanoDLP analytic {name} read: {e}"))?;
    let decoder = flate2::read::MultiGzDecoder::new(entry);
    crate::io::athena::parse_analytic(decoder.take(crate::io::athena::MAX_ANALYTIC_DECOMPRESSED))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn sample_profile() -> NanoDlpProfile {
        NanoDlpProfile {
            cure_time: 2.0,
            support_cure_time: 8.0,
            support_layer_number: 1,
            lift_speed: 150.0,
            retract_speed: 300.0,
            wait_before_print: 4.0,
            wait_after_print: 0.5,
            wait_height: 5.0,
            transitional_layer: 0,
            depth_um: 50.0,
        }
    }

    #[test]
    fn recipe_maps_nanodlp_profile_fields() {
        let r = nanodlp_recipe(&sample_profile(), 50.0).expect("valid recipe");
        assert_eq!(r.layer_height_um, 50.0);
        assert_eq!(r.bottom_layer_count, 1);
        assert_eq!(r.normal_exposure_sec, 2.0);
        assert_eq!(r.bottom_exposure_sec, 8.0);
        assert_eq!(r.lift_speed_mm_min, 150.0);
        assert_eq!(r.retract_speed_mm_min, Some(300.0));
    }

    #[test]
    fn recipe_falls_back_bottom_exposure_when_zero() {
        let mut p = sample_profile();
        p.support_cure_time = 0.0;
        let r = nanodlp_recipe(&p, 50.0).expect("valid recipe");
        assert_eq!(r.bottom_exposure_sec, r.normal_exposure_sec);
    }

    #[test]
    fn parse_fixture_header() {
        let (info, _layers) = parse_nanodlp(&fixture("mini.nanodlp")).expect("fixture parses");
        assert_eq!(info.format, "NANODLP");
        assert_eq!(info.total_layers, 3);
        assert_eq!(info.resolution_xy, (8, 8));
        assert!((info.pixel_size_um.0 - 50.0).abs() < 1e-3);
    }

    #[test]
    fn parse_fixture_layer_areas_and_z() {
        let (_info, layers) = parse_nanodlp(&fixture("mini.nanodlp")).expect("fixture parses");
        assert_eq!(layers.len(), 3);
        // pixel area = 0.05*0.05 = 0.0025 mm²; L1=32px, L2=16px, L3=8px.
        assert!((layers[0].cross_section_area_mm2 - 0.08).abs() < 1e-6);
        assert!((layers[1].cross_section_area_mm2 - 0.04).abs() < 1e-6);
        assert!((layers[2].cross_section_area_mm2 - 0.02).abs() < 1e-6);
        // z is cumulative (1-based) × 50 µm.
        assert!((layers[0].z_mm - 0.05).abs() < 1e-6);
        assert!((layers[2].z_mm - 0.15).abs() < 1e-6);
    }

    #[test]
    fn parse_fixture_bottom_layer_gets_support_exposure() {
        let (_info, layers) = parse_nanodlp(&fixture("mini.nanodlp")).expect("fixture parses");
        // SupportLayerNumber = 1 → layer 0 is a bottom layer.
        assert_eq!(layers[0].exposure_sec, 8.0);
        assert_eq!(layers[1].exposure_sec, 2.0);
    }

    #[test]
    fn parse_fixture_populates_mask() {
        let (_info, layers) = parse_nanodlp(&fixture("mini.nanodlp")).expect("fixture parses");
        let mask = layers[0].mask.as_ref().expect("layer 0 has a mask");
        assert!(mask.solid_cell_count() > 0, "lit pixels should set cells");
    }

    #[test]
    fn loads_embedded_analytic_log() {
        use crate::io::athena::CH_PRESSURE;
        let log = load_analytic_from_nanodlp(&fixture("mini.nanodlp"))
            .expect("embedded analytic-*.csv.gz parses");
        assert!(!log.samples.is_empty());
        assert!(!log.channel(CH_PRESSURE).is_empty(), "has pressure channel");
    }

    #[test]
    fn dimension_bomb_is_rejected() {
        let err = parse_nanodlp(&fixture("bomb-dimensions.nanodlp"))
            .expect_err("100000×100000 PNG must be rejected");
        assert!(
            err.contains("px limit") || err.to_lowercase().contains("bomb"),
            "got: {err}"
        );
    }
}
