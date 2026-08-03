//! Test fixtures shared across UAT step-def modules.
//!
//! `default_plate` / `test_ambient` / `test_supports` / `cube_areas` are
//! small closures duplicated from simulation_runner.rs's `#[cfg(test)]`
//! block (not re-exported as `pub`) — no builder covers them.
//! `printer_with_ranges` now delegates to `world::PrinterBuilder`.
//! The ADR-0020 field-sim TOML fragments below
//! (`RESIN_FIELD_SIM_THERMAL_LINES`, `PRINTER_FIELD_SIM_SCALARS`,
//! `PRINTER_BUILD_ENVELOPE_INLINE`, `resin_chemistry_root*`,
//! `valid_recipe_table`) are the single home every UAT resin/printer
//! fixture composes from.
//!
//! `NanoDlpJobBuilder` / `write_tall_analytic_csv` (uat-unskip-a3-b step 2)
//! are the single home for SYNTHESISED nanodlp/analytic-CSV fixtures,
//! composed by both nanodlp step-def modules
//! (docs/patterns/anti/fixture-copy-of-shared-builder.md).

use resinsim_core::entities::PrinterProfile;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

pub fn default_plate() -> PlateAdhesionProfile {
    PlateAdhesionProfile::default_textured()
}

pub fn test_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0).expect("22.0 °C is in AmbientTemperature domain")
}

pub fn test_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 10,
    }
}

pub fn cube_areas(n_layers: usize, area_mm2: f64) -> Vec<CrossSectionArea> {
    let a = CrossSectionArea::new(area_mm2).expect("cube area is non-negative and finite");
    vec![a; n_layers]
}

// ---- ADR-0020 / t2f4 field-sim deltas (single home) ------------------------
//
// t2f4 (286b0af, 2026-05-21) made three resin scalars and four printer
// scalars/tables required under the `field-sim` feature. Every hand-rolled
// UAT fixture must compose from these fragments rather than hand-copy the
// values — see docs/patterns/anti/fixture-copy-of-shared-builder.md, written
// after the identical defect rotted a unit-test fixture in
// resin_profile.rs (`s3-peel-shape-toml-fieldsim-thermal`, c0256a1).

/// ADR-0020 resin thermal-material scalars (KB-152 literature midpoints for
/// acrylate photopolymer). Root-level TOML lines — must land BEFORE any
/// `[recipe]` table or they silently nest into it
/// (docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
pub const RESIN_FIELD_SIM_THERMAL_LINES: &str = "thermal_conductivity_w_mk = 0.20\n\
     specific_heat_j_kgk = 1700.0\n\
     convective_top_h_w_m2k = 10.0\n";

/// ADR-0020 printer vat-wall + convective-BC scalars. Values mirror
/// `PrinterProfile::generic_msla_4k()` (still-air natural convection
/// ~8 W/m²·K, ~2.0 mm Al-alloy vat wall, ~200 W/m·K Al alloy conductivity).
/// Root-level TOML lines — same before-any-table ordering constraint as
/// `RESIN_FIELD_SIM_THERMAL_LINES`.
pub const PRINTER_FIELD_SIM_SCALARS: &str = "convective_wall_h_w_m2k = 8.0\n\
     vat_wall_thickness_mm = 2.0\n\
     vat_wall_k_w_mk = 200.0\n";

/// ADR-0020 `build_envelope_mm`, required under `field-sim`. INLINE table —
/// deliberately NOT a `[build_envelope_mm]` header block, so a scalar
/// appended after it (by a future fixture edit) cannot silently nest inside
/// it and be dropped
/// (docs/patterns/anti/toml-inline-keys-nest-into-preceding-table.md).
/// Extents mirror `generic_msla_4k` and clear `2 x THERMAL_VOXEL_MIN_MM` on
/// every axis.
pub const PRINTER_BUILD_ENVELOPE_INLINE: &str =
    "build_envelope_mm = { width_mm = 192.0, depth_mm = 120.0, max_z_mm = 200.0 }\n";

/// Resin chemistry root fields ONLY — pre-ADR-0020 shape, no thermal-material
/// scalars, no `[recipe]` table. Single home for the chemistry field list
/// (docs/patterns/anti/fixture-copy-of-shared-builder.md) so a future
/// required chemistry field is added in exactly one place. Only fixtures that
/// deliberately exercise the pre-t2f4 / parse-failure shape should call this
/// directly — everything else wants `resin_chemistry_root`.
pub fn resin_chemistry_root_pre_t2f4(name: &str) -> String {
    format!(
        r#"name = "{name}"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
"#
    )
}

/// `resin_chemistry_root_pre_t2f4` plus the ADR-0020 thermal-material
/// scalars — the field-sim-complete resin root every UAT fixture that
/// reaches `validate()` should compose from.
pub fn resin_chemistry_root(name: &str) -> String {
    format!(
        "{}{RESIN_FIELD_SIM_THERMAL_LINES}",
        resin_chemistry_root_pre_t2f4(name)
    )
}

/// The canonical `[recipe]` block — matches `generic_standard`'s recipe
/// (layer_height_um=50, normal_exposure=2.5, bottom_exposure=25,
/// bottom_layer_count=6, transition_layers=3, lift_speed=60, ...).
pub fn valid_recipe_table() -> &'static str {
    r#"[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#
}

/// Build a narrowed-range `PrinterProfile` — thin delegate to
/// `world::PrinterBuilder`. Every other field (name aside) tracks
/// `PrinterBuilder::new()`'s defaults, which were verified field-by-field
/// to equal this function's former hand-rolled literal (led_power 4.0,
/// pixel_pitch 50, lift-speed 10..200, bottom_layer_count_max 15,
/// z_stiffness 460, delta_t_steady 10, thermal_tau 1200, lcd_uniformity
/// 0.22) — a before/after TOML diff showed the only change is the four
/// ADR-0020 lines PrinterBuilder now appends
/// (docs/patterns/anti/fixture-copy-of-shared-builder.md: "split the
/// builder, don't fork it").
pub fn printer_with_ranges(
    layer_min: f32,
    layer_max: f32,
    exposure_min: f32,
    exposure_max: f32,
) -> PrinterProfile {
    super::world::PrinterBuilder::new()
        .with_name("UatNarrowed")
        .with_layer_height_range(layer_min, layer_max)
        .with_exposure_range(exposure_min, exposure_max)
        .build()
}

// ---- Committed-fixture paths (uat-unskip-a3-b step 2) ----------------------

/// Path to a committed fixture under `tests/fixtures/`. Single home for the
/// `CARGO_MANIFEST_DIR`-relative join so callers don't hand-roll it — mirrors
/// `io::nanodlp`'s own `#[cfg(test)] fixture()` helper, one level up (test
/// binary, not unit-test module).
pub fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Path to the committed `mini.nanodlp` fixture (3 layers, 8×8 px,
/// 50 µm pixels, `SupportLayerNumber = 1`). Moved here from
/// `base_adhesion_shifts_peel_peak.rs` (uat-unskip-a3-b step 2) so every
/// nanodlp step-def module shares one path literal
/// (docs/patterns/anti/fixture-copy-of-shared-builder.md); that module keeps
/// a one-line delegating helper + pointer comment at the old call site.
/// Implementation kept byte-identical to the original (workspace-data-dir-
/// relative, not `fixture_path`-based) — a mechanical move, not a rewrite.
pub fn mini_nanodlp_path() -> std::path::PathBuf {
    super::cli_fixtures::workspace_data_dir()
        .parent()
        .expect("workspace_data_dir has a repo-root parent")
        .join("crates/resinsim-core/tests/fixtures/mini.nanodlp")
}

// ---- NanoDLP job synthesiser (uat-unskip-a3-b step 2) ----------------------
//
// REACHABILITY (plan step 2(a) probe): `zip`, `png`, `flate2` are regular
// `[dependencies]` of `resinsim-core` (not `[dev-dependencies]`), so Cargo
// makes them available to every target in the package including this
// integration-test crate — proven by this module compiling and by
// `nanodlp_import_simulates.rs`'s round-trip-through-`parse_sliced` step
// passing under `cargo uat`. `zip` is built
// `default-features = false, features = ["deflate"]`, so every entry below
// uses `CompressionMethod::Stored` — no optional zip feature is assumed.
//
// Entry names and JSON key spellings mirror the committed `mini.nanodlp`
// fixture exactly (`meta.json` with `"distro": "athena", "program":
// "NanoDLP"`, `profile.json`, `slicer.json`, `plate.json`, `{n}.png`
// 1-indexed) so a synthesised archive exercises the SAME `parse_nanodlp`
// branches a real export does — every scenario using this builder proves it
// by round-tripping through `io::sliced::parse_sliced` before any Then reads
// the output.

/// Monotonic counter for unique per-scenario temp-file names — cucumber runs
/// scenarios within a feature concurrently (same rationale as
/// `sim_json_roundtrips_zero_force_layer.rs::unique_sim_json_path`), so a
/// fixed shared filename would race across scenarios.
fn unique_tmp_path(tag: &str, ext: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("uat-{tag}-{n}.{ext}"))
}

/// Write a tall `ID,T,V` analytic CSV under `CARGO_TARGET_TMPDIR`, optionally
/// gzip-compressed, with a unique per-call name. Used by
/// `athena_analytic_log_ingest.rs`'s malformed-row Given (the exact shape
/// `io/athena.rs`'s own `malformed_row_rejected` unit test uses).
pub fn write_tall_analytic_csv(tag: &str, body: &str, gzip: bool) -> std::path::PathBuf {
    use std::io::Write as _;
    if gzip {
        let path = unique_tmp_path(tag, "csv.gz");
        let file = std::fs::File::create(&path)
            .unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder
            .write_all(body.as_bytes())
            .unwrap_or_else(|e| panic!("gzip write {}: {e}", path.display()));
        encoder
            .finish()
            .unwrap_or_else(|e| panic!("gzip finish {}: {e}", path.display()));
        path
    } else {
        let path = unique_tmp_path(tag, "csv");
        std::fs::write(&path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        path
    }
}

/// Encode a single-channel (grayscale, 8-bit) PNG whose first `lit_pixels`
/// pixels (raster order) are lit (255, ≥ `nanodlp::LIT_THRESHOLD` = 128) and
/// the rest are unlit (0) — the same "first channel ≥ threshold ⇒ occupied"
/// read `decode_layer_png` performs, so a synthesised layer's cross-section
/// area is exactly `lit_pixels * XPixelSize * YPixelSize` mm².
fn encode_grayscale_png(width: u32, height: u32, lit_pixels: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .expect("PNG header encode is infallible for a valid width/height");
        let total = (width as u64) * (height as u64);
        let lit = lit_pixels.min(total) as usize;
        let mut data = vec![0u8; total as usize];
        data[..lit].fill(255);
        writer
            .write_image_data(&data)
            .expect("PNG pixel encode is infallible for a correctly-sized buffer");
    }
    buf
}

fn gzip_bytes(body: &str) -> Vec<u8> {
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(body.as_bytes())
        .expect("gzip in-memory encode: write");
    encoder.finish().expect("gzip in-memory encode: finish")
}

/// Write one ZIP entry with `CompressionMethod::Stored`.
fn zip_write_entry<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    options: zip::write::SimpleFileOptions,
    name: &str,
    bytes: &[u8],
) {
    use std::io::Write as _;
    zip.start_file(name, options)
        .unwrap_or_else(|e| panic!("zip start_file {name}: {e}"));
    zip.write_all(bytes)
        .unwrap_or_else(|e| panic!("zip write entry {name}: {e}"));
}

/// The output of [`NanoDlpJobBuilder::build`]: the archive path plus the
/// exact recipe inputs the builder recorded — Thens compare per-layer
/// `exposure_sec` against THESE fields, never a re-derived `if i < K`
/// branch expression (docs/patterns/anti/test-mirrors-production-formula.md).
pub struct BuiltNanoDlpJob {
    pub path: std::path::PathBuf,
    /// `profile.json`'s `SupportLayerNumber` — the K in "the first K layers".
    pub support_layer_number: u32,
    /// `profile.json`'s `SupportCureTime` — bottom-layer exposure, since this
    /// builder always sets a non-zero `SupportCureTime` (the
    /// `nanodlp_recipe` fallback-to-`CureTime` branch is never exercised by
    /// synthesised jobs).
    pub support_exposure_sec: f32,
    /// `profile.json`'s `CureTime` — normal-layer exposure.
    pub normal_exposure_sec: f32,
}

/// Builder for a synthesised `.nanodlp` archive. ONE home for every
/// synthesised nanodlp fixture in the UAT suite — see the module-level
/// reachability/mirroring note above.
#[derive(Debug, Clone)]
pub struct NanoDlpJobBuilder {
    /// Per-layer lit-pixel counts, one entry per layer; drives both the
    /// synthesised PNG content and (via `XPixelSize * YPixelSize`) the
    /// resulting `cross_section_area_mm2` the production PNG decoder
    /// computes. `layers_count` defaults to this Vec's length.
    lit_pixel_counts: Vec<u64>,
    p_width: u32,
    p_height: u32,
    thickness_um: f32,
    x_pixel_size_mm: f32,
    y_pixel_size_mm: f32,
    layers_count: Option<u32>,
    /// `profile.json` `CureTime` (normal-layer exposure, seconds).
    cure_time: f32,
    /// `profile.json` `SupportCureTime` (bottom-layer exposure, seconds).
    support_cure_time: f32,
    /// `profile.json` `SupportLayerNumber` (K, the bottom-layer count).
    support_layer_number: u32,
    lift_speed: f32,
    /// `profile.json` `Depth` — layer-thickness fallback; irrelevant here
    /// since `thickness_um` always populates `slicer.json Thickness` (> 0),
    /// which `parse_nanodlp` prefers.
    depth_um: f32,
    /// Optional tall `ID,T,V` analytic body, gzipped to
    /// `analytic-fixture.csv.gz` when present — mirrors `mini.nanodlp`'s
    /// embedded log entry name exactly.
    analytic_body: Option<String>,
}

impl NanoDlpJobBuilder {
    /// Defaults mirror `mini.nanodlp`'s own shape (8×8 px, 50 µm pixels,
    /// `SupportLayerNumber = 1`, `CureTime = 2.0`, `SupportCureTime = 8.0`,
    /// `LiftSpeed = 150.0`) so a caller that only overrides
    /// `lit_pixel_counts` gets a job that exercises identical branches to
    /// the committed fixture.
    pub fn new() -> Self {
        Self {
            lit_pixel_counts: vec![32, 16, 8],
            p_width: 8,
            p_height: 8,
            thickness_um: 50.0,
            x_pixel_size_mm: 0.05,
            y_pixel_size_mm: 0.05,
            layers_count: None,
            cure_time: 2.0,
            support_cure_time: 8.0,
            support_layer_number: 1,
            lift_speed: 150.0,
            depth_um: 50.0,
            analytic_body: None,
        }
    }

    /// Sets per-layer lit-pixel counts AND (unless overridden separately by
    /// [`Self::with_layers_count`]) the layer count, since the two must
    /// agree for every layer to get a PNG.
    pub fn with_lit_pixel_counts(mut self, counts: impl Into<Vec<u64>>) -> Self {
        self.lit_pixel_counts = counts.into();
        self
    }

    pub fn with_resolution(mut self, p_width: u32, p_height: u32) -> Self {
        self.p_width = p_width;
        self.p_height = p_height;
        self
    }

    /// Explicit `plate.json LayersCount`, decoupled from
    /// `lit_pixel_counts.len()`. Not called by any scenario landed so far
    /// (every builder call site wants LayersCount == the lit-pixel Vec
    /// length) — scoped `expect(dead_code)` rather than a blanket `allow`,
    /// matching `world.rs`'s `PrinterBuilder::with_z_stiffness` precedent.
    #[expect(
        dead_code,
        reason = "reserved for a future scenario needing LayersCount != lit_pixel_counts.len()"
    )]
    pub fn with_layers_count(mut self, n: u32) -> Self {
        self.layers_count = Some(n);
        self
    }

    pub fn with_support_layer_number(mut self, k: u32) -> Self {
        self.support_layer_number = k;
        self
    }

    pub fn with_cure_times(mut self, normal_sec: f32, support_sec: f32) -> Self {
        self.cure_time = normal_sec;
        self.support_cure_time = support_sec;
        self
    }

    /// Tall `ID,T,V` analytic body embedded as `analytic-fixture.csv.gz` —
    /// the exact entry name `load_analytic_from_nanodlp` selects when it is
    /// the lexicographically-last (here, only) `analytic-*.csv.gz` entry.
    pub fn with_analytic_body(mut self, body: impl Into<String>) -> Self {
        self.analytic_body = Some(body.into());
        self
    }

    /// Build the archive under `CARGO_TARGET_TMPDIR` with a unique per-call
    /// suffix; `tag` names the caller for readability under `target/tmp`.
    pub fn build(self, tag: &str) -> BuiltNanoDlpJob {
        let out = unique_tmp_path(tag, "nanodlp");
        let layers_count = self
            .layers_count
            .unwrap_or(self.lit_pixel_counts.len() as u32);
        let bed_x_mm = self.p_width as f32 * self.x_pixel_size_mm;
        let bed_y_mm = self.p_height as f32 * self.y_pixel_size_mm;
        let total_z_mm = layers_count as f32 * self.thickness_um / 1000.0;

        let meta_json = serde_json::json!({
            "format_version": 2,
            "distro": "athena",
            "program": "NanoDLP",
            "version": "11403",
            "os": "linux",
            "arch": "arm",
            "profile": false
        })
        .to_string();
        let profile_json = serde_json::json!({
            "ProfileID": 9001,
            "Title": "Synthesised UAT fixture",
            "Depth": self.depth_um,
            "CureTime": self.cure_time,
            "SupportCureTime": self.support_cure_time,
            "SupportLayerNumber": self.support_layer_number,
            "LiftSpeed": self.lift_speed,
            "RetractSpeed": 300,
            "ZStepWait": 600,
            "WaitBeforePrint": 4,
            "WaitAfterPrint": 0.5,
            "WaitHeight": 5,
            "SupportWaitBeforePrint": 7,
            "SupportWaitAfterPrint": 1.5,
            "SupportWaitHeight": 6,
            "TransitionalLayer": 0
        })
        .to_string();
        let slicer_json = serde_json::json!({
            "PWidth": self.p_width,
            "PHeight": self.p_height,
            "Thickness": self.thickness_um,
            "SupportDepth": self.thickness_um,
            "XPixelSize": self.x_pixel_size_mm,
            "YPixelSize": self.y_pixel_size_mm,
            "SupportLayerNumber": self.support_layer_number,
            "Boundary": {
                "XMin": 0.0, "XMax": bed_x_mm,
                "YMin": 0.0, "YMax": bed_y_mm,
                "ZMin": 0, "ZMax": total_z_mm
            }
        })
        .to_string();
        let plate_json = serde_json::json!({
            "PlateID": 42,
            "ProfileID": 9001,
            "Path": "synthesised_uat_fixture",
            "LayersCount": layers_count,
            "ZMax": total_z_mm,
            "TotalSolidArea": 0.0,
            "XMin": 0.0, "XMax": bed_x_mm,
            "YMin": 0.0, "YMax": bed_y_mm
        })
        .to_string();

        let pngs: Vec<Vec<u8>> = (0..layers_count as usize)
            .map(|i| {
                let lit = self
                    .lit_pixel_counts
                    .get(i)
                    .copied()
                    .unwrap_or_else(|| self.lit_pixel_counts.last().copied().unwrap_or(0));
                encode_grayscale_png(self.p_width, self.p_height, lit)
            })
            .collect();
        let analytic_gz = self.analytic_body.as_deref().map(gzip_bytes);

        let file = std::fs::File::create(&out)
            .unwrap_or_else(|e| panic!("create {}: {e}", out.display()));
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip_write_entry(&mut zip, options, "meta.json", meta_json.as_bytes());
        zip_write_entry(&mut zip, options, "profile.json", profile_json.as_bytes());
        zip_write_entry(&mut zip, options, "slicer.json", slicer_json.as_bytes());
        zip_write_entry(&mut zip, options, "plate.json", plate_json.as_bytes());
        for (i, png_bytes) in pngs.iter().enumerate() {
            zip_write_entry(&mut zip, options, &format!("{}.png", i + 1), png_bytes);
        }
        if let Some(gz) = &analytic_gz {
            zip_write_entry(&mut zip, options, "analytic-fixture.csv.gz", gz);
        }
        zip.finish()
            .unwrap_or_else(|e| panic!("finish zip {}: {e}", out.display()));

        BuiltNanoDlpJob {
            path: out,
            support_layer_number: self.support_layer_number,
            support_exposure_sec: self.support_cure_time,
            normal_exposure_sec: self.cure_time,
        }
    }
}

impl Default for NanoDlpJobBuilder {
    fn default() -> Self {
        Self::new()
    }
}
