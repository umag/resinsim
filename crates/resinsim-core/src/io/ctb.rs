//! CTB file parser for resinsim.
//! Supports CTB V2-V5 (plain) and V5 encrypted formats.
//! Derived from DragonFruit CTB plugin code (GPL-3.0).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::io::sliced::{LayerInput, SlicedFileInfo};

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
    use aes::Aes256;
    use aes::cipher::block_padding::NoPadding;
    use aes::cipher::{BlockDecryptMut, KeyIvInit};

    if bytes.is_empty() || !bytes.len().is_multiple_of(16) {
        return Err(format!("AES block must be multiple of 16 bytes, got {}", bytes.len()));
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
                if i >= data.len() { break; }
                let b1 = data[i]; i += 1;
                ((b0 as u64 & 0x7f) << 8) | b1 as u64
            } else if b0 & 0xe0 == 0xc0 {
                if i + 1 > data.len() { break; }
                let b1 = data[i]; i += 1;
                let b2 = data[i]; i += 1;
                ((b0 as u64 & 0x3f) << 16) | ((b1 as u64) << 8) | b2 as u64
            } else {
                if i + 2 > data.len() { break; }
                let b1 = data[i]; i += 1;
                let b2 = data[i]; i += 1;
                let b3 = data[i]; i += 1;
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
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("failed to open CTB: {e}"))?;

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
    file.seek(SeekFrom::Start(0)).map_err(|e| format!("seek: {e}"))?;
    let mut header = [0u8; CTB_HEADER_SIZE];
    file.read_exact(&mut header).map_err(|e| format!("header read: {e}"))?;

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
        layer_height_um: layer_height_mm * 1000.0,
        resolution_xy: (width_px, height_px),
        pixel_size_um: (bed_x / width_px as f32 * 1000.0, bed_y / height_px as f32 * 1000.0),
        bed_size_mm: (bed_x, bed_y),
        normal_exposure_sec: exposure_sec,
        bottom_exposure_sec,
        bottom_layer_count: bottom_layers,
        lift_speed_mm_min: 60.0, // default, not in basic header
    };

    let mut layers = Vec::with_capacity(layer_count as usize);
    for i in 0..layer_count {
        let def_offset = layers_def_off as u64 + i as u64 * CTB_LAYER_DEF_SIZE as u64;
        file.seek(SeekFrom::Start(def_offset)).map_err(|e| format!("layer def seek: {e}"))?;
        let mut layer_def = [0u8; CTB_LAYER_DEF_SIZE];
        file.read_exact(&mut layer_def).map_err(|e| format!("layer def read: {e}"))?;

        let z_mm = read_f32_le(&layer_def, 0);
        let layer_exposure = read_f32_le(&layer_def, 4);
        let data_page_rel = read_u32_le(&layer_def, 12);
        let encoded_len = read_u32_le(&layer_def, 16);
        let page_number = read_u32_le(&layer_def, 20);

        let abs_data = page_number as u64 * CTB_PAGE_SIZE + data_page_rel as u64;
        file.seek(SeekFrom::Start(abs_data)).map_err(|e| format!("layer data seek: {e}"))?;
        let mut rle_bytes = vec![0u8; encoded_len as usize];
        file.read_exact(&mut rle_bytes).map_err(|e| format!("layer data read: {e}"))?;

        ctb_layer_rle_xor(xor_key, i, &mut rle_bytes);
        let lit_pixels = rle_count_lit_pixels(&rle_bytes);
        let area_mm2 = lit_pixels as f64 * pixel_area_mm2;

        layers.push(LayerInput {
            index: i,
            cross_section_area_mm2: area_mm2,
            exposure_sec: layer_exposure,
            lift_speed_mm_min: 60.0,
            layer_height_um: layer_height_mm * 1000.0,
            z_mm,
        });
    }

    Ok((info, layers))
}

fn parse_encrypted(file: &mut std::fs::File) -> Result<(SlicedFileInfo, Vec<LayerInput>), String> {
    file.seek(SeekFrom::Start(CTB_ENCRYPTED_SETTINGS_OFFSET)).map_err(|e| format!("seek: {e}"))?;
    let mut settings = vec![0u8; CTB_ENCRYPTED_SETTINGS_SIZE];
    file.read_exact(&mut settings).map_err(|e| format!("settings read: {e}"))?;
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
        file.seek(SeekFrom::Start(ptr_offset)).map_err(|e| format!("ptr seek: {e}"))?;
        let mut pointer = [0u8; 16];
        file.read_exact(&mut pointer).map_err(|e| format!("ptr read: {e}"))?;

        let def_page_rel = read_u32_le(&pointer, 0);
        let def_page = read_u32_le(&pointer, 4);
        let abs_def = def_page as u64 * CTB_PAGE_SIZE + def_page_rel as u64;

        file.seek(SeekFrom::Start(abs_def)).map_err(|e| format!("layer def seek: {e}"))?;
        let mut layer_def = [0u8; CTB_ENCRYPTED_LAYER_DEF_SIZE];
        file.read_exact(&mut layer_def).map_err(|e| format!("layer def read: {e}"))?;

        let z_mm = read_f32_le(&layer_def, 4);
        let layer_exposure = read_f32_le(&layer_def, 8);
        let data_page_rel = read_u32_le(&layer_def, 16);
        let data_page = read_u32_le(&layer_def, 20);
        let encoded_len = read_u32_le(&layer_def, 24);

        let abs_data = data_page as u64 * CTB_PAGE_SIZE + data_page_rel as u64;
        file.seek(SeekFrom::Start(abs_data)).map_err(|e| format!("layer data seek: {e}"))?;
        let mut rle_bytes = vec![0u8; encoded_len as usize];
        file.read_exact(&mut rle_bytes).map_err(|e| format!("layer data read: {e}"))?;

        ctb_layer_rle_xor(xor_key, i, &mut rle_bytes);
        let lit_pixels = rle_count_lit_pixels(&rle_bytes);
        let area_mm2 = lit_pixels as f64 * pixel_area_mm2;

        layers.push(LayerInput {
            index: i,
            cross_section_area_mm2: area_mm2,
            exposure_sec: layer_exposure,
            lift_speed_mm_min: 60.0,
            layer_height_um: 0.0, // filled in below
            z_mm,
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
    let bottom_count = layers.iter().take_while(|l| l.exposure_sec > normal_exp * 1.5).count() as u32;

    let info = SlicedFileInfo {
        format: "CTB (encrypted)".into(),
        total_layers: layer_count,
        layer_height_um: lh_um,
        resolution_xy: (width_px, height_px),
        pixel_size_um: (bed_x / width_px as f32 * 1000.0, bed_y / height_px as f32 * 1000.0),
        bed_size_mm: (bed_x, bed_y),
        normal_exposure_sec: normal_exp,
        bottom_exposure_sec: bottom_exp,
        bottom_layer_count: bottom_count,
        lift_speed_mm_min: 60.0,
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
}
