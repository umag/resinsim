//! CTB file parser for resinsim.
//! Supports CTB V2-V5 (plain) and V5 encrypted formats.
//! Derived from DragonFruit CTB plugin code (GPL-3.0).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::entities::Recipe;
use crate::io::sliced::{LayerInput, SlicedFileInfo};
use crate::values::{DEFAULT_VOXEL_SIZE_MM, LayerMask, MaskError};

/// Build a Recipe from CTB-parsed fields. Fields not present in CTB format
/// (transition_layers, wait_*, lift_cycle_sec, lift_distance_mm) take documented
/// defaults. Per ADR-0005 §4.
// CTB parser fuses 4 parsed + 6 default values; bundling into a struct would
// obscure which values come from bytes vs defaults.
#[allow(clippy::too_many_arguments)]
fn ctb_recipe(
    layer_height_um: f32,
    bottom_layer_count: u32,
    normal_exposure_sec: f32,
    bottom_exposure_sec: f32,
    lift_speed_mm_min: f32,
) -> Result<Recipe, String> {
    Recipe::new(
        layer_height_um,
        bottom_layer_count,
        3, // transition_layers — CTB does not carry; RERF default
        normal_exposure_sec,
        bottom_exposure_sec,
        0.5, // wait_before_cure_sec — CTB does not carry
        1.0, // wait_before_release_sec — CTB does not carry
        0.0, // wait_after_release_sec — CTB does not carry
        lift_speed_mm_min,
        7.5,  // lift_cycle_sec — CTB does not carry
        5.0,  // lift_distance_mm — CTB does not carry
        None, // retract_speed_mm_min — CTB does not carry; falls back to lift_speed
    )
}

// --- Magic bytes ---
const CTB_MAGIC_V2_V3: u32 = 0x12FD_0086;
const CTB_MAGIC_V4_V5: u32 = 0x12FD_0106;
const CTB_MAGIC_V5_ENCRYPTED: u32 = 0x12FD_0107;

// --- Sizes ---
const CTB_HEADER_SIZE: usize = 112;
const CTB_LAYER_DEF_SIZE: usize = 36;
const CTB_PAGE_SIZE: u64 = 4_294_967_296;
const CTB_ENCRYPTED_SETTINGS_OFFSET: u64 = 48;
const CTB_ENCRYPTED_SETTINGS_SIZE: usize = 288;
const CTB_ENCRYPTED_LAYER_DEF_SIZE: usize = 88;

// --- AES key/iv for encrypted CTB (DragonFruit-derived) ---
const CTB_AES_OBFUSCATION: &[u8; 14] = b"DragonFruitFTW";
const CTB_AES_DEFAULT_KEY_XOR: [u8; 32] = [
    0x94, 0x29, 0xEF, 0x54, 0x1E, 0xB0, 0x7B, 0x68, 0x90, 0x26, 0x56, 0x9B, 0x8B, 0x0C, 0xB9, 0xE6,
    0xCA, 0x3A, 0x0B, 0x54, 0xDB, 0x0C, 0xCA, 0xC6, 0x36, 0x45, 0xA7, 0x47, 0x9C, 0x20, 0x4B, 0x8D,
];
const CTB_AES_DEFAULT_IV_XOR: [u8; 16] = [
    0x4B, 0x73, 0x6B, 0x62, 0x6A, 0x65, 0x40, 0x75, 0x7D, 0x6F, 0x7E, 0x4A, 0x58, 0x5A, 0x4D, 0x7D,
];

/// Read a little-endian u32 from `buf` at `off`. Caller contract: `buf.len() >= off + 4`.
/// Used on header/layer_def/settings buffers whose sizes are fixed constants and
/// validated by the preceding `read_exact` call.
fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    let slice: [u8; 4] = buf[off..off + 4]
        .try_into()
        .expect("4-byte slice at constant offset within read_exact-validated buffer");
    u32::from_le_bytes(slice)
}

/// Read a little-endian f32 from `buf` at `off`. Same contract as `read_u32_le`.
fn read_f32_le(buf: &[u8], off: usize) -> f32 {
    let slice: [u8; 4] = buf[off..off + 4]
        .try_into()
        .expect("4-byte slice at constant offset within read_exact-validated buffer");
    f32::from_le_bytes(slice)
}

fn xor_deobfuscate<const N: usize>(input: [u8; N]) -> [u8; N] {
    let mut out = [0u8; N];
    for i in 0..N {
        out[i] = input[i] ^ CTB_AES_OBFUSCATION[i % CTB_AES_OBFUSCATION.len()];
    }
    out
}

fn aes_decrypt(bytes: &mut [u8]) -> Result<(), String> {
    use aes::cipher::block_padding::NoPadding;
    use aes::cipher::{BlockDecryptMut, KeyIvInit};
    use aes::Aes256;

    if bytes.is_empty() || !bytes.len().is_multiple_of(16) {
        return Err(format!(
            "AES block must be multiple of 16 bytes, got {}",
            bytes.len()
        ));
    }
    let key = xor_deobfuscate(CTB_AES_DEFAULT_KEY_XOR);
    let iv = xor_deobfuscate(CTB_AES_DEFAULT_IV_XOR);
    cbc::Decryptor::<Aes256>::new((&key).into(), (&iv).into())
        .decrypt_padded_mut::<NoPadding>(bytes)
        .map_err(|e| format!("AES decrypt failed: {e}"))?;
    Ok(())
}

fn ctb_layer_rle_xor(seed: u32, layer_index: u32, bytes: &mut [u8]) {
    if seed == 0 {
        return;
    }
    let init = seed.wrapping_mul(0x2d83_cdac).wrapping_add(0xd8a8_3423);
    let mut key = layer_index
        .wrapping_mul(0x1e15_30cd)
        .wrapping_add(0xec3d_47cd)
        .wrapping_mul(init);

    let mut chunks = bytes.chunks_exact_mut(4);
    for chunk in &mut chunks {
        let kb = key.to_le_bytes();
        chunk[0] ^= kb[0];
        chunk[1] ^= kb[1];
        chunk[2] ^= kb[2];
        chunk[3] ^= kb[3];
        key = key.wrapping_add(init);
    }
    let remainder = chunks.into_remainder();
    if !remainder.is_empty() {
        let kb = key.to_le_bytes();
        for (i, b) in remainder.iter_mut().enumerate() {
            *b ^= kb[i];
        }
    }
}

/// Decode an RLE run-length field starting at `data[*cursor]`. Advances
/// `*cursor` past the consumed bytes. Returns None on truncated input.
///
/// Lifted from `rle_count_lit_pixels` and `rle_to_mask` — shared between
/// both decoders.
fn decode_run_len(data: &[u8], cursor: &mut usize, has_len: bool) -> Option<u64> {
    if !has_len {
        return Some(1);
    }
    if *cursor >= data.len() {
        return None;
    }
    let b0 = data[*cursor];
    *cursor += 1;
    if b0 & 0x80 == 0 {
        Some(b0 as u64)
    } else if b0 & 0xc0 == 0x80 {
        if *cursor >= data.len() {
            return None;
        }
        let b1 = data[*cursor];
        *cursor += 1;
        Some(((b0 as u64 & 0x7f) << 8) | b1 as u64)
    } else if b0 & 0xe0 == 0xc0 {
        if *cursor + 1 > data.len() {
            return None;
        }
        let b1 = data[*cursor];
        *cursor += 1;
        let b2 = data[*cursor];
        *cursor += 1;
        Some(((b0 as u64 & 0x3f) << 16) | ((b1 as u64) << 8) | b2 as u64)
    } else {
        if *cursor + 2 > data.len() {
            return None;
        }
        let b1 = data[*cursor];
        *cursor += 1;
        let b2 = data[*cursor];
        *cursor += 1;
        let b3 = data[*cursor];
        *cursor += 1;
        Some(((b0 as u64 & 0x1f) << 24) | ((b1 as u64) << 16) | ((b2 as u64) << 8) | b3 as u64)
    }
}

/// Decode a CTB RLE layer into a downsampled LayerMask at `voxel_size_mm`
/// physical resolution.
///
/// Algorithm (Step 5 of suction-detector-raft-false-positive):
/// - For each lit run in the RLE stream, determine which native pixel rows
///   and columns it covers.
/// - For each voxel cell intersecting the run, count the number of lit
///   native pixels that fall inside its world-space footprint. Accumulate
///   per-voxel lit-pixel counts in `lit_counts`.
/// - After decoding, a voxel is marked solid iff `lit_counts[v] >=
///   ceil(native_pixels_per_voxel / 2)` — the majority rule.
///
/// Memory trade-off (accepted 2026-04-21): one native-resolution temporary
/// lit-count vector per layer. For a 4K MSLA at 0.5mm voxels on a 153×78mm
/// bed: voxel grid ≈ 306×156 = 48K cells × 4 bytes = 192 KB per layer.
/// The full `LayerMask` output is ~6 KB per layer (bit-packed).
///
/// Returns `Err(MaskError)` on malformed dimensions / voxel size.
fn rle_to_mask(
    rle_bytes: &[u8],
    native_width_px: u32,
    native_height_px: u32,
    bed_x_mm: f32,
    bed_y_mm: f32,
    voxel_size_mm: f32,
) -> Result<LayerMask, MaskError> {
    if native_width_px == 0 || native_height_px == 0 {
        return Err(MaskError::InvalidDimensions {
            width: native_width_px,
            height: native_height_px,
        });
    }
    if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
        return Err(MaskError::InvalidVoxelSize(voxel_size_mm));
    }
    if !bed_x_mm.is_finite() || bed_x_mm <= 0.0 || !bed_y_mm.is_finite() || bed_y_mm <= 0.0 {
        return Err(MaskError::InvalidDimensions {
            width: bed_x_mm as u32,
            height: bed_y_mm as u32,
        });
    }

    let voxel_width = ((bed_x_mm / voxel_size_mm).ceil() as u32).max(1);
    let voxel_height = ((bed_y_mm / voxel_size_mm).ceil() as u32).max(1);
    let mut lit_counts = vec![0u32; (voxel_width as usize) * (voxel_height as usize)];

    let px_w_mm = bed_x_mm / native_width_px as f32;
    let px_h_mm = bed_y_mm / native_height_px as f32;
    let native_per_voxel_x = voxel_size_mm / px_w_mm;
    let native_per_voxel_y = voxel_size_mm / px_h_mm;
    let native_per_voxel = (native_per_voxel_x * native_per_voxel_y).round().max(1.0) as u32;
    // Majority threshold: at least half the native pixels in the voxel must be lit.
    let threshold = native_per_voxel.div_ceil(2);

    let total_pixels = (native_width_px as u64) * (native_height_px as u64);
    let mut pixel_idx: u64 = 0;
    let mut i = 0;
    while i < rle_bytes.len() && pixel_idx < total_pixels {
        let code = rle_bytes[i];
        i += 1;
        let pixel = (code & 0x7f) << 1;
        let has_len = (code & 0x80) != 0;
        let run_len = match decode_run_len(rle_bytes, &mut i, has_len) {
            Some(n) => n,
            None => break,
        };

        if pixel > 0 && run_len > 0 {
            accumulate_lit_run(
                pixel_idx,
                run_len,
                native_width_px,
                native_height_px,
                px_w_mm,
                px_h_mm,
                voxel_width,
                voxel_height,
                voxel_size_mm,
                &mut lit_counts,
            );
        }
        pixel_idx += run_len;
    }

    let mut mask = LayerMask::new(voxel_width, voxel_height, voxel_size_mm)?;
    for vy in 0..voxel_height {
        for vx in 0..voxel_width {
            let c = lit_counts[(vy as usize) * voxel_width as usize + vx as usize];
            if c >= threshold {
                mask.set(vx, vy)?;
            }
        }
    }
    Ok(mask)
}

/// Accumulate lit-pixel counts from a single RLE lit run into `lit_counts`.
/// Batches native pixels per voxel (not per-pixel iteration) to keep decoding
/// tractable on dense layers.
#[allow(clippy::too_many_arguments)]
fn accumulate_lit_run(
    pixel_idx: u64,
    run_len: u64,
    native_width_px: u32,
    native_height_px: u32,
    px_w_mm: f32,
    px_h_mm: f32,
    voxel_width: u32,
    voxel_height: u32,
    voxel_size_mm: f32,
    lit_counts: &mut [u32],
) {
    let nw = native_width_px as u64;
    let nh = native_height_px as u64;
    let total = nw * nh;
    let run_end = (pixel_idx + run_len).min(total);
    let mut p = pixel_idx;

    while p < run_end {
        let row = p / nw;
        if row >= nh {
            break;
        }
        let col_start_in_row = p % nw;
        // End of this row's portion of the run (exclusive)
        let row_boundary = (row + 1) * nw;
        let slice_end = run_end.min(row_boundary);
        let col_end_in_row_excl = col_start_in_row + (slice_end - p);

        // Map row → voxel row
        let world_y = row as f32 * px_h_mm;
        let vy_f = world_y / voxel_size_mm;
        let vy = (vy_f as i64).clamp(0, voxel_height as i64 - 1) as u32;

        // Map col range → voxel col range
        let world_x_start = col_start_in_row as f32 * px_w_mm;
        let world_x_end_excl = col_end_in_row_excl as f32 * px_w_mm;
        let vx_start = ((world_x_start / voxel_size_mm) as i64)
            .clamp(0, voxel_width as i64 - 1) as u32;
        let vx_end = (((world_x_end_excl - f32::EPSILON) / voxel_size_mm) as i64)
            .clamp(0, voxel_width as i64 - 1) as u32;

        for vx in vx_start..=vx_end {
            // Voxel's native-pixel column range [px_lo, px_hi)
            let voxel_x_left = vx as f32 * voxel_size_mm;
            let voxel_x_right = (vx + 1) as f32 * voxel_size_mm;
            let px_lo = (voxel_x_left / px_w_mm).ceil() as u64;
            let px_hi = (voxel_x_right / px_w_mm).ceil() as u64;
            // Overlap with the run's portion in this row
            let lo = px_lo.max(col_start_in_row);
            let hi = px_hi.min(col_end_in_row_excl);
            if hi > lo {
                let add = (hi - lo) as u32;
                let idx = (vy as usize) * voxel_width as usize + vx as usize;
                lit_counts[idx] = lit_counts[idx].saturating_add(add);
            }
        }

        p = slice_end;
    }
}

/// Count non-zero pixels in CTB RLE data without allocating full bitmap.
fn rle_count_lit_pixels(data: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut i = 0;
    while i < data.len() {
        let code = data[i];
        i += 1;
        let pixel = (code & 0x7f) << 1;
        let has_len = (code & 0x80) != 0;

        let run_len: u64 = if !has_len {
            1
        } else if i >= data.len() {
            break;
        } else {
            let b0 = data[i];
            i += 1;
            if b0 & 0x80 == 0 {
                b0 as u64
            } else if b0 & 0xc0 == 0x80 {
                if i >= data.len() {
                    break;
                }
                let b1 = data[i];
                i += 1;
                ((b0 as u64 & 0x7f) << 8) | b1 as u64
            } else if b0 & 0xe0 == 0xc0 {
                if i + 1 > data.len() {
                    break;
                }
                let b1 = data[i];
                i += 1;
                let b2 = data[i];
                i += 1;
                ((b0 as u64 & 0x3f) << 16) | ((b1 as u64) << 8) | b2 as u64
            } else {
                if i + 2 > data.len() {
                    break;
                }
                let b1 = data[i];
                i += 1;
                let b2 = data[i];
                i += 1;
                let b3 = data[i];
                i += 1;
                ((b0 as u64 & 0x1f) << 24) | ((b1 as u64) << 16) | ((b2 as u64) << 8) | b3 as u64
            }
        };

        if pixel > 0 {
            count += run_len;
        }
    }
    count
}

/// Parse a CTB file (any version) and extract per-layer data.
pub fn parse_ctb(path: &Path) -> Result<(SlicedFileInfo, Vec<LayerInput>), String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("failed to open CTB: {e}"))?;

    let mut magic_bytes = [0u8; 4];
    file.read_exact(&mut magic_bytes)
        .map_err(|e| format!("CTB magic read failed: {e}"))?;
    let magic = u32::from_le_bytes(magic_bytes);

    match magic {
        CTB_MAGIC_V5_ENCRYPTED => parse_encrypted(&mut file),
        CTB_MAGIC_V4_V5 | CTB_MAGIC_V2_V3 => parse_plain(&mut file),
        _ => Err(format!("unknown CTB magic: 0x{magic:08X}")),
    }
}

fn parse_plain(file: &mut std::fs::File) -> Result<(SlicedFileInfo, Vec<LayerInput>), String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    let mut header = [0u8; CTB_HEADER_SIZE];
    file.read_exact(&mut header)
        .map_err(|e| format!("header read: {e}"))?;

    let bed_x = read_f32_le(&header, 8);
    let bed_y = read_f32_le(&header, 12);
    let layer_height_mm = read_f32_le(&header, 24);
    let exposure_sec = read_f32_le(&header, 28);
    let bottom_exposure_sec = read_f32_le(&header, 32);
    let bottom_layers = read_u32_le(&header, 40);
    let width_px = read_u32_le(&header, 52);
    let height_px = read_u32_le(&header, 56);
    let layers_def_off = read_u32_le(&header, 64);
    let layer_count = read_u32_le(&header, 68);
    let xor_key = read_u32_le(&header, 100);

    let pixel_area_mm2 = (bed_x / width_px as f32) as f64 * (bed_y / height_px as f32) as f64;

    let info = SlicedFileInfo {
        format: "CTB".into(),
        total_layers: layer_count,
        resolution_xy: (width_px, height_px),
        pixel_size_um: (
            bed_x / width_px as f32 * 1000.0,
            bed_y / height_px as f32 * 1000.0,
        ),
        bed_size_mm: (bed_x, bed_y),
        recipe: ctb_recipe(
            layer_height_mm * 1000.0,
            bottom_layers,
            exposure_sec,
            bottom_exposure_sec,
            60.0, // lift_speed_mm_min — not in basic header; documented default
        )
        .map_err(|e| format!("CTB recipe invalid: {e}"))?,
    };

    let mut layers = Vec::with_capacity(layer_count as usize);
    for i in 0..layer_count {
        let def_offset = layers_def_off as u64 + i as u64 * CTB_LAYER_DEF_SIZE as u64;
        file.seek(SeekFrom::Start(def_offset))
            .map_err(|e| format!("layer def seek: {e}"))?;
        let mut layer_def = [0u8; CTB_LAYER_DEF_SIZE];
        file.read_exact(&mut layer_def)
            .map_err(|e| format!("layer def read: {e}"))?;

        let z_mm = read_f32_le(&layer_def, 0);
        let layer_exposure = read_f32_le(&layer_def, 4);
        let data_page_rel = read_u32_le(&layer_def, 12);
        let encoded_len = read_u32_le(&layer_def, 16);
        let page_number = read_u32_le(&layer_def, 20);

        let abs_data = page_number as u64 * CTB_PAGE_SIZE + data_page_rel as u64;
        file.seek(SeekFrom::Start(abs_data))
            .map_err(|e| format!("layer data seek: {e}"))?;
        let mut rle_bytes = vec![0u8; encoded_len as usize];
        file.read_exact(&mut rle_bytes)
            .map_err(|e| format!("layer data read: {e}"))?;

        ctb_layer_rle_xor(xor_key, i, &mut rle_bytes);
        let lit_pixels = rle_count_lit_pixels(&rle_bytes);
        let area_mm2 = lit_pixels as f64 * pixel_area_mm2;
        let mask = rle_to_mask(
            &rle_bytes,
            width_px,
            height_px,
            bed_x,
            bed_y,
            DEFAULT_VOXEL_SIZE_MM,
        )
        .ok();

        layers.push(LayerInput {
            index: i,
            cross_section_area_mm2: area_mm2,
            exposure_sec: layer_exposure,
            lift_speed_mm_min: 60.0,
            layer_height_um: layer_height_mm * 1000.0,
            z_mm,
            mask,
        });
    }

    Ok((info, layers))
}

fn parse_encrypted(file: &mut std::fs::File) -> Result<(SlicedFileInfo, Vec<LayerInput>), String> {
    file.seek(SeekFrom::Start(CTB_ENCRYPTED_SETTINGS_OFFSET))
        .map_err(|e| format!("seek: {e}"))?;
    let mut settings = vec![0u8; CTB_ENCRYPTED_SETTINGS_SIZE];
    file.read_exact(&mut settings)
        .map_err(|e| format!("settings read: {e}"))?;
    aes_decrypt(&mut settings)?;

    let layer_pointers_off = read_u32_le(&settings, 8);
    let width_px = read_u32_le(&settings, 56);
    let height_px = read_u32_le(&settings, 60);
    let layer_count = read_u32_le(&settings, 64);
    let xor_key = read_u32_le(&settings, 128);

    // Encrypted settings layout (from DragonFruit ctb_v5enc.rs write order):
    //   [0..8]    checksum (u64)
    //   [8..12]   layer_pointers_offset (u32)
    //   [12..16]  build_width_mm (f32)
    //   [16..20]  build_depth_mm (f32)
    //   [20..24]  bed_size_z_mm (f32)
    //   [32..36]  total_height_mm (f32)
    //   [36..40]  layer_height_mm (f32)
    //   [40..44]  normal_exposure_sec (f32)
    //   [44..48]  bottom_exposure_sec (f32)
    //   [52..56]  bottom_layer_count (u32)
    //   [56..60]  width_px (u32)
    //   [60..64]  height_px (u32)
    //   [64..68]  layer_count (u32)
    //   [128..132] layer_xor_key (u32)
    let bed_x = read_f32_le(&settings, 12);
    let bed_y = read_f32_le(&settings, 16);

    if width_px == 0 || height_px == 0 {
        return Err(format!("invalid dimensions {width_px}x{height_px}"));
    }

    let pixel_area_mm2 = (bed_x / width_px as f32) as f64 * (bed_y / height_px as f32) as f64;

    let mut layers = Vec::with_capacity(layer_count as usize);
    for i in 0..layer_count {
        let ptr_offset = layer_pointers_off as u64 + i as u64 * 16;
        file.seek(SeekFrom::Start(ptr_offset))
            .map_err(|e| format!("ptr seek: {e}"))?;
        let mut pointer = [0u8; 16];
        file.read_exact(&mut pointer)
            .map_err(|e| format!("ptr read: {e}"))?;

        let def_page_rel = read_u32_le(&pointer, 0);
        let def_page = read_u32_le(&pointer, 4);
        let abs_def = def_page as u64 * CTB_PAGE_SIZE + def_page_rel as u64;

        file.seek(SeekFrom::Start(abs_def))
            .map_err(|e| format!("layer def seek: {e}"))?;
        let mut layer_def = [0u8; CTB_ENCRYPTED_LAYER_DEF_SIZE];
        file.read_exact(&mut layer_def)
            .map_err(|e| format!("layer def read: {e}"))?;

        let z_mm = read_f32_le(&layer_def, 4);
        let layer_exposure = read_f32_le(&layer_def, 8);
        let data_page_rel = read_u32_le(&layer_def, 16);
        let data_page = read_u32_le(&layer_def, 20);
        let encoded_len = read_u32_le(&layer_def, 24);

        let abs_data = data_page as u64 * CTB_PAGE_SIZE + data_page_rel as u64;
        file.seek(SeekFrom::Start(abs_data))
            .map_err(|e| format!("layer data seek: {e}"))?;
        let mut rle_bytes = vec![0u8; encoded_len as usize];
        file.read_exact(&mut rle_bytes)
            .map_err(|e| format!("layer data read: {e}"))?;

        ctb_layer_rle_xor(xor_key, i, &mut rle_bytes);
        let lit_pixels = rle_count_lit_pixels(&rle_bytes);
        let area_mm2 = lit_pixels as f64 * pixel_area_mm2;
        let mask = rle_to_mask(
            &rle_bytes,
            width_px,
            height_px,
            bed_x,
            bed_y,
            DEFAULT_VOXEL_SIZE_MM,
        )
        .ok();

        layers.push(LayerInput {
            index: i,
            cross_section_area_mm2: area_mm2,
            exposure_sec: layer_exposure,
            lift_speed_mm_min: 60.0,
            layer_height_um: 0.0, // filled in below
            z_mm,
            mask,
        });
    }

    // Derive layer height from Z positions
    let actual_lh_mm = if layers.len() >= 2 {
        layers[1].z_mm - layers[0].z_mm
    } else if !layers.is_empty() {
        layers[0].z_mm
    } else {
        0.05
    };
    let lh_um = actual_lh_mm * 1000.0;
    for li in &mut layers {
        li.layer_height_um = lh_um;
    }

    // Derive exposure from layer data
    let normal_exp = layers.last().map(|l| l.exposure_sec).unwrap_or(2.0);
    let bottom_exp = layers.first().map(|l| l.exposure_sec).unwrap_or(25.0);
    let bottom_count = layers
        .iter()
        .take_while(|l| l.exposure_sec > normal_exp * 1.5)
        .count() as u32;

    let info = SlicedFileInfo {
        format: "CTB (encrypted)".into(),
        total_layers: layer_count,
        resolution_xy: (width_px, height_px),
        pixel_size_um: (
            bed_x / width_px as f32 * 1000.0,
            bed_y / height_px as f32 * 1000.0,
        ),
        bed_size_mm: (bed_x, bed_y),
        recipe: ctb_recipe(
            lh_um,
            bottom_count,
            normal_exp,
            bottom_exp,
            60.0, // lift_speed_mm_min — encrypted CTB does not expose; documented default
        )
        .map_err(|e| format!("CTB (encrypted) recipe invalid: {e}"))?,
    };

    Ok((info, layers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_count_single_lit_pixel() {
        // Code byte: pixel=64 (value 128>>1=64), no length → 1 pixel
        let data = [0x40]; // pixel=(0x40 & 0x7f)<<1 = 128, has_len=false → 1 pixel
        assert_eq!(rle_count_lit_pixels(&data), 1);
    }

    #[test]
    fn rle_count_run_of_lit() {
        // Code byte: pixel=64, has_len=true. Next byte: length=10
        let data = [0xC0, 0x0A]; // pixel=(0xC0 & 0x7f)<<1=128, has_len=true, len=10
        assert_eq!(rle_count_lit_pixels(&data), 10);
    }

    #[test]
    fn rle_count_dark_pixels_not_counted() {
        // pixel=0, run of 100
        let data = [0x80, 0x64]; // pixel=0, has_len=true, len=100
        assert_eq!(rle_count_lit_pixels(&data), 0);
    }

    #[test]
    fn rle_count_mixed() {
        // 50 dark + 30 lit
        let data = [
            0x80, 0x32, // pixel=0, len=50
            0xC0, 0x1E, // pixel=128, len=30
        ];
        assert_eq!(rle_count_lit_pixels(&data), 30);
    }

    #[test]
    fn xor_roundtrip() {
        let mut data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let original = data.clone();
        ctb_layer_rle_xor(12345, 0, &mut data);
        assert_ne!(data, original);
        ctb_layer_rle_xor(12345, 0, &mut data);
        assert_eq!(data, original);
    }

    #[test]
    fn xor_zero_seed_noop() {
        let mut data = vec![1, 2, 3, 4];
        let original = data.clone();
        ctb_layer_rle_xor(0, 0, &mut data);
        assert_eq!(data, original);
    }

    // --- rle_to_mask tests (Step 5 of suction-detector-raft-false-positive) ---

    #[test]
    fn rle_to_mask_empty_rle_all_void() {
        // 4×4 native, 2mm bed at 0.5mm voxel → 4×4 voxels.
        let mask = rle_to_mask(&[], 4, 4, 2.0, 2.0, 0.5)
            .expect("valid inputs yield a mask");
        assert_eq!(mask.solid_cell_count(), 0);
    }

    #[test]
    fn rle_to_mask_fully_solid_layer() {
        // 4×4 native, pixel=128, has_len=true, len=16 (all 16 pixels lit).
        let data = [0xC0, 0x10];
        // 4×4 native at 1mm/pixel = 4mm×4mm bed; voxel_size=1mm → 4×4 voxels
        // native_per_voxel = 1×1 = 1. threshold = 1. Every voxel has 1 lit
        // pixel → all solid.
        let mask = rle_to_mask(&data, 4, 4, 4.0, 4.0, 1.0)
            .expect("valid 4×4 native, 4mm bed, 1mm voxel yields mask");
        assert_eq!(mask.solid_cell_count(), 16);
    }

    #[test]
    fn rle_to_mask_downsamples_majority_rule() {
        // 4×4 native, left half lit (8 pixels), right half dark (8 pixels).
        // At 2×2 voxels (voxel_size=2mm, bed=4mm, native=4×4), each voxel
        // covers a 2×2 block of native pixels. Left voxels have 4 lit each,
        // right voxels have 0 lit. Threshold = ceil(4/2) = 2.
        // Expected: left column solid, right column void.
        //
        // Row-major layout: row0=[1,1,0,0], row1=[1,1,0,0], row2=[1,1,0,0], row3=[1,1,0,0]
        // Encoded as 4 rows of [2 lit, 2 dark]:
        let data = [
            0xC0, 0x02, 0x80, 0x02, // row 0: 2 lit, 2 dark
            0xC0, 0x02, 0x80, 0x02, // row 1
            0xC0, 0x02, 0x80, 0x02, // row 2
            0xC0, 0x02, 0x80, 0x02, // row 3
        ];
        let mask = rle_to_mask(&data, 4, 4, 4.0, 4.0, 2.0)
            .expect("valid inputs yield a mask");
        assert_eq!(mask.width_cells(), 2);
        assert_eq!(mask.height_cells(), 2);
        assert!(mask.is_solid(0, 0), "left column bottom");
        assert!(mask.is_solid(0, 1), "left column top");
        assert!(!mask.is_solid(1, 0), "right column bottom void");
        assert!(!mask.is_solid(1, 1), "right column top void");
    }

    #[test]
    fn rle_to_mask_sparse_below_threshold_stays_void() {
        // 4×4 native, 1 lit pixel in a 2×2 voxel needing threshold=2 → void.
        // native_per_voxel = 4; threshold = ceil(4/2) = 2. A single lit pixel
        // doesn't reach majority.
        //
        // Row 0: 1 lit + 3 dark; rows 1-3 fully dark (16-4 = 12 dark)
        let data = [
            0x40, // 1 lit pixel (no length prefix)
            0x80, 0x0F, // 15 dark
        ];
        let mask = rle_to_mask(&data, 4, 4, 4.0, 4.0, 2.0)
            .expect("valid inputs yield a mask");
        assert_eq!(mask.solid_cell_count(), 0);
    }

    #[test]
    fn rle_to_mask_rejects_zero_native_width() {
        let err = rle_to_mask(&[], 0, 4, 4.0, 4.0, 1.0);
        assert!(matches!(err, Err(MaskError::InvalidDimensions { .. })));
    }

    #[test]
    fn rle_to_mask_rejects_non_finite_voxel_size() {
        assert!(matches!(
            rle_to_mask(&[], 4, 4, 4.0, 4.0, 0.0),
            Err(MaskError::InvalidVoxelSize(_))
        ));
        assert!(matches!(
            rle_to_mask(&[], 4, 4, 4.0, 4.0, f32::NAN),
            Err(MaskError::InvalidVoxelSize(_))
        ));
    }

    #[test]
    fn decode_run_len_single_byte() {
        let data = [0x0A];
        let mut cursor = 0;
        assert_eq!(decode_run_len(&data, &mut cursor, true), Some(10));
        assert_eq!(cursor, 1);
    }

    #[test]
    fn decode_run_len_no_len_returns_one() {
        let data = [];
        let mut cursor = 0;
        assert_eq!(decode_run_len(&data, &mut cursor, false), Some(1));
        assert_eq!(cursor, 0);
    }

    #[test]
    fn decode_run_len_truncated_returns_none() {
        let data = [];
        let mut cursor = 0;
        assert_eq!(decode_run_len(&data, &mut cursor, true), None);
    }
}
