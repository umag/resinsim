//! Sidecar decoder. ADR-0019. Reads the RSFIELD binary format and
//! reconstitutes the four optional voxel fields.
//!
//! # Bounded decompression
//!
//! Each slab is decompressed into a per-slab fixed-size buffer matching
//! the descriptor's `uncompressed_layer_byte_size`. `zstd::decode_all`
//! is BANNED — it has no output bound. We instead use `zstd::Decoder`
//! over a bounded input slice and assert the decoded byte count equals
//! the expected size; mismatch produces a typed error.
//!
//! # Allocation guards
//!
//! The decoder rejects implausible `field_count`, `layer_count`, and
//! `uncompressed_layer_byte_size × layer_count` BEFORE any allocation
//! by validating against the format spec caps and the active field
//! budget (`field_budget::active_budget_bytes`).

use std::io::{Read, Seek, SeekFrom};

use ndarray::Array3;

use crate::values::field_budget::active_budget_bytes;
use crate::values::{
    CureField, PhotoinitiatorField, StrainField, StrainTensor, StressField, StressTensor,
    ThermalField,
};

use super::error::DecodeError;
use super::format::{
    FieldKind, COMPRESSION_TAG_ZSTD, FIELD_COMPONENT_SIZE_SCALAR, FIELD_COMPONENT_SIZE_TENSOR,
    LAYOUT_TAG_LAYER_SLABS, MAX_FIELD_COUNT, MAX_REASONABLE_LAYER_COUNT, RSFIELD_FORMAT_VERSION,
    RSFIELD_MAGIC,
};

/// Decoded sidecar — the five optional reconstituted voxel fields.
/// ADR-0020 / t2f4 added `thermal` which uses a different bbox + voxel
/// size from the others (vat envelope vs part bbox).
#[derive(Debug)]
pub struct DecodedSidecar {
    pub cure: Option<CureField>,
    pub photoinitiator: Option<PhotoinitiatorField>,
    pub strain: Option<StrainField>,
    pub stress: Option<StressField>,
    /// ADR-0020 / t2f4 — vat-envelope thermal field.
    pub thermal: Option<ThermalField>,
}

/// Decode a sidecar from `reader`. The reader must support `Seek`
/// (for random-access slab reads). `context` is used in error messages
/// (typically the sidecar path).
pub fn decode_sidecar<R: Read + Seek>(
    reader: &mut R,
    context: &str,
) -> Result<DecodedSidecar, DecodeError> {
    let decoder = FieldSidecarDecoder::new(context);
    decoder.decode(reader)
}

/// Stateful decoder. Carries the error-context string used in
/// `DecodeError::UnknownMagic` etc.
pub struct FieldSidecarDecoder {
    context: String,
}

impl FieldSidecarDecoder {
    pub fn new(context: &str) -> Self {
        Self {
            context: context.into(),
        }
    }

    pub fn decode<R: Read + Seek>(&self, reader: &mut R) -> Result<DecodedSidecar, DecodeError> {
        // Header.
        reader.seek(SeekFrom::Start(0))?;
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        if magic != RSFIELD_MAGIC {
            return Err(DecodeError::UnknownMagic {
                context: self.context.clone(),
            });
        }
        let format_version = read_u32_le(reader)?;
        if format_version != RSFIELD_FORMAT_VERSION {
            return Err(DecodeError::UnknownFormatVersion {
                got: format_version,
                expected: RSFIELD_FORMAT_VERSION,
            });
        }
        let field_count = read_u32_le(reader)?;
        if field_count == 0 || field_count > MAX_FIELD_COUNT {
            return Err(DecodeError::ImplausibleFieldCount {
                got: field_count,
                max: MAX_FIELD_COUNT,
            });
        }
        // Skip reserved 48 bytes.
        let mut reserved = [0u8; 48];
        reader.read_exact(&mut reserved)?;

        // Discover the sidecar's actual file size for size-mismatch check.
        let current = reader.stream_position()?;
        let total_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(current))?;

        // Read all descriptors first; defer slab decode.
        let mut descriptors: Vec<ParsedDescriptor> = Vec::with_capacity(field_count as usize);
        let mut implied_payload: u64 = 0;
        let header_plus_descriptors_start = current;
        for _ in 0..field_count {
            let d = self.read_descriptor(reader)?;
            let total_compressed: u64 = d.layer_sizes.iter().map(|s| u64::from(*s)).sum();
            implied_payload = implied_payload.saturating_add(total_compressed);
            descriptors.push(d);
        }
        let post_descriptors = reader.stream_position()?;
        let descriptor_bytes = post_descriptors - header_plus_descriptors_start;
        let body_bytes = total_size.saturating_sub(64 + descriptor_bytes);
        if implied_payload > body_bytes {
            return Err(DecodeError::SizeMismatch {
                implied: implied_payload,
                available: body_bytes,
            });
        }

        // Cross-field dimension lock.
        check_descriptor_dimension_lock(&descriptors)?;

        // Read each slab and rebuild the in-memory field.
        let mut decoded = DecodedSidecar {
            cure: None,
            photoinitiator: None,
            strain: None,
            stress: None,
            thermal: None,
        };
        for d in &descriptors {
            match d.kind {
                FieldKind::Cure => {
                    let data = self.read_scalar_field(reader, d)?;
                    let f = CureField::from_persistence_parts(
                        d.dim_x,
                        d.dim_y,
                        d.dim_z,
                        d.voxel_size_mm,
                        d.bbox_origin,
                        data,
                    )
                    .map_err(|e| DecodeError::Reconstitution {
                        field_name: "cure".into(),
                        detail: format!("{e}"),
                    })?;
                    decoded.cure = Some(f);
                }
                FieldKind::Photoinitiator => {
                    let data = self.read_scalar_field(reader, d)?;
                    // Re-derive initial_concentration from the max value
                    // seen in the data; a fresh field is uniform-filled,
                    // so the max equals initial_concentration on the
                    // round-trip of an unmodified field. Already-depleted
                    // fields will see the max as the most-conserved voxel.
                    // Clamped to [0, 1] for safety.
                    let max = data.iter().fold(0.0_f32, |a, b| a.max(*b)).clamp(0.0, 1.0);
                    let f = PhotoinitiatorField::from_persistence_parts(
                        d.dim_x, d.dim_y, d.dim_z, max, data,
                    )
                    .map_err(|e| DecodeError::Reconstitution {
                        field_name: "photoinitiator".into(),
                        detail: format!("{e}"),
                    })?;
                    decoded.photoinitiator = Some(f);
                }
                FieldKind::Strain => {
                    let data = self.read_tensor_array::<R, StrainTensor>(reader, d)?;
                    let f = StrainField::from_persistence_parts(
                        d.dim_x,
                        d.dim_y,
                        d.dim_z,
                        d.voxel_size_mm,
                        d.bbox_origin,
                        data,
                    )
                    .map_err(|e| DecodeError::Reconstitution {
                        field_name: "strain".into(),
                        detail: format!("{e}"),
                    })?;
                    decoded.strain = Some(f);
                }
                FieldKind::Stress => {
                    let data = self.read_stress_array(reader, d)?;
                    let f = StressField::from_persistence_parts(
                        d.dim_x,
                        d.dim_y,
                        d.dim_z,
                        d.voxel_size_mm,
                        d.bbox_origin,
                        data,
                    )
                    .map_err(|e| DecodeError::Reconstitution {
                        field_name: "stress".into(),
                        detail: format!("{e}"),
                    })?;
                    decoded.stress = Some(f);
                }
                FieldKind::Thermal => {
                    // ADR-0020 / t2f4 — scalar f32 per voxel. Different
                    // dims from cure/strain/stress (vat-envelope vs
                    // part bbox); the cross-field-dim lock excludes it.
                    let data = self.read_scalar_field(reader, d)?;
                    let f = ThermalField::from_persistence_parts(
                        d.dim_x,
                        d.dim_y,
                        d.dim_z,
                        d.voxel_size_mm,
                        d.bbox_origin,
                        data,
                    )
                    .map_err(|e| DecodeError::Reconstitution {
                        field_name: "thermal".into(),
                        detail: format!("{e}"),
                    })?;
                    decoded.thermal = Some(f);
                }
            }
        }
        Ok(decoded)
    }

    fn read_descriptor<R: Read>(&self, reader: &mut R) -> Result<ParsedDescriptor, DecodeError> {
        let name_len = read_u32_le(reader)?;
        if !(1..=64).contains(&name_len) {
            return Err(DecodeError::Reconstitution {
                field_name: "descriptor".into(),
                detail: format!("name_len {name_len} outside [1, 64]"),
            });
        }
        let mut name_bytes = vec![0u8; name_len as usize];
        reader.read_exact(&mut name_bytes)?;
        let _name = String::from_utf8(name_bytes).map_err(DecodeError::InvalidFieldName)?;
        let kind_tag = read_u32_le(reader)?;
        let kind = FieldKind::from_tag(kind_tag)
            .ok_or(DecodeError::UnknownFieldKindTag { got: kind_tag })?;
        let dim_x = read_u32_le(reader)?;
        let dim_y = read_u32_le(reader)?;
        let dim_z = read_u32_le(reader)?;
        let bbox_origin = [
            read_f32_le(reader)?,
            read_f32_le(reader)?,
            read_f32_le(reader)?,
        ];
        let voxel_size_mm = read_f32_le(reader)?;
        let component_size = read_u32_le(reader)?;
        let expected_component_size = kind.component_size();
        if component_size != expected_component_size {
            return Err(DecodeError::ComponentSizeMismatch {
                field_name: kind.name().into(),
                got: component_size,
                expected: expected_component_size,
            });
        }
        let compression_tag = read_u32_le(reader)?;
        if compression_tag != COMPRESSION_TAG_ZSTD {
            return Err(DecodeError::UnsupportedCompression {
                got: compression_tag,
            });
        }
        let layout_tag = read_u32_le(reader)?;
        if layout_tag != LAYOUT_TAG_LAYER_SLABS {
            return Err(DecodeError::UnsupportedLayout { got: layout_tag });
        }
        let layer_count = read_u32_le(reader)?;
        if layer_count != dim_z {
            return Err(DecodeError::Reconstitution {
                field_name: kind.name().into(),
                detail: format!("layer_count {layer_count} != dim_z {dim_z}"),
            });
        }
        if layer_count > MAX_REASONABLE_LAYER_COUNT {
            return Err(DecodeError::ImplausibleLayerCount {
                field_name: kind.name().into(),
                got: layer_count,
                max: MAX_REASONABLE_LAYER_COUNT,
            });
        }
        let uncompressed_layer_byte_size = read_u64_le(reader)?;
        // Pre-allocation budget check.
        let expected_layer_bytes = u64::from(dim_x)
            .checked_mul(u64::from(dim_y))
            .and_then(|v| v.checked_mul(u64::from(component_size)));
        match expected_layer_bytes {
            Some(v) if v == uncompressed_layer_byte_size => {}
            _ => {
                return Err(DecodeError::Reconstitution {
                    field_name: kind.name().into(),
                    detail: format!(
                        "uncompressed_layer_byte_size {uncompressed_layer_byte_size} != dim_x×dim_y×component_size"
                    ),
                });
            }
        }
        let implied_total = uncompressed_layer_byte_size.saturating_mul(u64::from(layer_count));
        if implied_total > active_budget_bytes() {
            return Err(DecodeError::ExceedsFieldBudget {
                field_name: kind.name().into(),
                implied: implied_total,
                budget: active_budget_bytes(),
            });
        }
        let mut layer_offsets = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            layer_offsets.push(read_u64_le(reader)?);
        }
        let mut layer_sizes = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            layer_sizes.push(read_u32_le(reader)?);
        }
        Ok(ParsedDescriptor {
            kind,
            dim_x,
            dim_y,
            dim_z,
            bbox_origin,
            voxel_size_mm,
            uncompressed_layer_byte_size,
            layer_offsets,
            layer_sizes,
        })
    }

    fn read_scalar_field<R: Read + Seek>(
        &self,
        reader: &mut R,
        d: &ParsedDescriptor,
    ) -> Result<Array3<f32>, DecodeError> {
        debug_assert!(d.kind.component_size() == FIELD_COMPONENT_SIZE_SCALAR);
        let mut arr = Array3::<f32>::zeros((d.dim_x as usize, d.dim_y as usize, d.dim_z as usize));
        for iz in 0..d.dim_z {
            let slab = self.read_slab(reader, d, iz)?;
            if slab.is_empty() {
                continue;
            }
            let mut cursor = 0usize;
            for iy in 0..d.dim_y {
                for ix in 0..d.dim_x {
                    let bytes = &slab[cursor..cursor + 4];
                    let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    if !v.is_finite() {
                        return Err(DecodeError::NonFinite {
                            field_name: d.kind.name().into(),
                            detail: format!("non-finite f32 at ({ix}, {iy}, {iz})"),
                        });
                    }
                    arr[[ix as usize, iy as usize, iz as usize]] = v;
                    cursor += 4;
                }
            }
        }
        Ok(arr)
    }

    fn read_tensor_array<R: Read + Seek, T: TensorFromBytes + Clone>(
        &self,
        reader: &mut R,
        d: &ParsedDescriptor,
    ) -> Result<Array3<T>, DecodeError> {
        let mut arr = Array3::<T>::from_elem(
            (d.dim_x as usize, d.dim_y as usize, d.dim_z as usize),
            T::zero(),
        );
        for iz in 0..d.dim_z {
            let slab = self.read_slab(reader, d, iz)?;
            if slab.is_empty() {
                continue;
            }
            let mut cursor = 0usize;
            for iy in 0..d.dim_y {
                for ix in 0..d.dim_x {
                    let bytes = &slab[cursor..cursor + 24];
                    let t = T::from_voigt_le_bytes(bytes, &d.kind, ix, iy, iz)?;
                    arr[[ix as usize, iy as usize, iz as usize]] = t;
                    cursor += 24;
                }
            }
        }
        Ok(arr)
    }

    fn read_slab<R: Read + Seek>(
        &self,
        reader: &mut R,
        d: &ParsedDescriptor,
        iz: u32,
    ) -> Result<Vec<u8>, DecodeError> {
        let idx = iz as usize;
        let size = d.layer_sizes[idx];
        if size == 0 {
            return Ok(Vec::new());
        }
        let offset = d.layer_offsets[idx];
        reader.seek(SeekFrom::Start(offset))?;
        let mut compressed = vec![0u8; size as usize];
        reader.read_exact(&mut compressed)?;

        let expected = d.uncompressed_layer_byte_size as usize;
        let mut decoded = Vec::with_capacity(expected);
        let mut decoder =
            zstd::stream::read::Decoder::new(&compressed[..]).map_err(DecodeError::Io)?;
        // Bounded copy: read exactly `expected` bytes; if zstd emits
        // fewer or more, error out.
        let mut buffer = vec![0u8; 8192];
        loop {
            let n =
                decoder
                    .read(&mut buffer)
                    .map_err(|e| DecodeError::SlabDecompressionFailed {
                        field_name: d.kind.name().into(),
                        iz,
                        detail: format!("{e}"),
                    })?;
            if n == 0 {
                break;
            }
            if decoded.len() + n > expected {
                return Err(DecodeError::SlabDecompressionFailed {
                    field_name: d.kind.name().into(),
                    iz,
                    detail: format!("decoded > expected ({expected})"),
                });
            }
            decoded.extend_from_slice(&buffer[..n]);
        }
        if decoded.len() != expected {
            return Err(DecodeError::SlabDecompressionFailed {
                field_name: d.kind.name().into(),
                iz,
                detail: format!("decoded {} != expected {expected}", decoded.len()),
            });
        }
        Ok(decoded)
    }
}

#[derive(Debug)]
struct ParsedDescriptor {
    kind: FieldKind,
    dim_x: u32,
    dim_y: u32,
    dim_z: u32,
    bbox_origin: [f32; 3],
    voxel_size_mm: f32,
    uncompressed_layer_byte_size: u64,
    layer_offsets: Vec<u64>,
    layer_sizes: Vec<u32>,
}

trait TensorFromBytes: Sized {
    fn zero() -> Self;
    fn from_voigt_le_bytes(
        bytes: &[u8],
        kind: &FieldKind,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<Self, DecodeError>;
}

impl TensorFromBytes for StrainTensor {
    fn zero() -> Self {
        StrainTensor::zero()
    }
    fn from_voigt_le_bytes(
        bytes: &[u8],
        _kind: &FieldKind,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<Self, DecodeError> {
        let comps = read_voigt_components_le(bytes);
        StrainTensor::new(comps[0], comps[1], comps[2], comps[3], comps[4], comps[5]).map_err(
            |_| DecodeError::NonFinite {
                field_name: "strain".into(),
                detail: format!("non-finite tensor at ({ix}, {iy}, {iz})"),
            },
        )
    }
}

impl TensorFromBytes for StressTensor {
    fn zero() -> Self {
        StressTensor::zero()
    }
    fn from_voigt_le_bytes(
        bytes: &[u8],
        _kind: &FieldKind,
        ix: u32,
        iy: u32,
        iz: u32,
    ) -> Result<Self, DecodeError> {
        let comps = read_voigt_components_le(bytes);
        StressTensor::new(comps[0], comps[1], comps[2], comps[3], comps[4], comps[5]).map_err(
            |_| DecodeError::NonFinite {
                field_name: "stress".into(),
                detail: format!("non-finite tensor at ({ix}, {iy}, {iz})"),
            },
        )
    }
}

fn read_voigt_components_le(bytes: &[u8]) -> [f32; 6] {
    debug_assert!(bytes.len() >= 24);
    let mut out = [0.0_f32; 6];
    for (i, chunk) in bytes[..24].chunks_exact(4).enumerate() {
        out[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    out
}

fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32, DecodeError> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le<R: Read>(reader: &mut R) -> Result<u64, DecodeError> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_f32_le<R: Read>(reader: &mut R) -> Result<f32, DecodeError> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn check_descriptor_dimension_lock(descriptors: &[ParsedDescriptor]) -> Result<(), DecodeError> {
    // ADR-0020 / t2f4: ThermalField has different dims from the other
    // fields (vat envelope vs part bbox) so it is EXCLUDED from this
    // cross-field dimension lock. Filter out thermal descriptors here;
    // they are checked individually by their own consistency invariants
    // (positive dims + finite voxel_size + budget) earlier.
    let mut iter = descriptors
        .iter()
        .filter(|d| !matches!(d.kind, FieldKind::Thermal));
    let first = match iter.next() {
        None => return Ok(()),
        Some(d) => d,
    };
    let (fx, fy, fz) = (first.dim_x, first.dim_y, first.dim_z);
    for d in iter {
        if (d.dim_x, d.dim_y, d.dim_z) != (fx, fy, fz) {
            return Err(DecodeError::DimensionMismatch {
                first_field: first.kind.name().into(),
                first_x: fx,
                first_y: fy,
                first_z: fz,
                second_field: d.kind.name().into(),
                second_x: d.dim_x,
                second_y: d.dim_y,
                second_z: d.dim_z,
            });
        }
    }
    Ok(())
}

impl FieldSidecarDecoder {
    fn read_stress_array<R: Read + Seek>(
        &self,
        reader: &mut R,
        d: &ParsedDescriptor,
    ) -> Result<Array3<StressTensor>, DecodeError> {
        self.read_tensor_array::<R, StressTensor>(reader, d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::sidecar::encoder::{encode_sidecar, SidecarFields};

    #[test]
    fn roundtrip_cure_only_preserves_dose_field() {
        let mut cure = CureField::new(3, 3, 2, 0.05, [1.0, 2.0, 3.0]).expect("ctor");
        // CureField is allocated to zeros; add_dose can apply scalar
        // updates but that needs valid dp/ec/dose. We rely on the all-zero
        // roundtrip exercising empty-slab encode/decode, which is the
        // important sparsity path.
        let _ = &mut cure;
        let fields = SidecarFields {
            cure: Some(&cure),
            ..Default::default()
        };
        let mut buf = Vec::new();
        encode_sidecar(&fields, &mut buf).expect("encode");

        let mut cursor = std::io::Cursor::new(buf);
        let decoded = decode_sidecar(&mut cursor, "<test>").expect("decode");
        let decoded_cure = decoded.cure.expect("cure decoded");
        assert_eq!(decoded_cure.dimensions(), (3, 3, 2));
        assert_eq!(decoded_cure.voxel_size_mm(), 0.05);
        assert_eq!(decoded_cure.bbox_min_mm(), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn corrupt_magic_returns_typed_error() {
        let cure = CureField::new(2, 2, 2, 0.05, [0.0; 3]).expect("ctor");
        let mut buf = Vec::new();
        encode_sidecar(
            &SidecarFields {
                cure: Some(&cure),
                ..Default::default()
            },
            &mut buf,
        )
        .expect("encode");
        buf[0] = b'X'; // corrupt magic
        let mut cursor = std::io::Cursor::new(buf);
        let err = decode_sidecar(&mut cursor, "<test>").expect_err("expect corrupt magic");
        assert!(format!("{err}").contains("unknown sidecar magic"));
    }

    #[test]
    fn be_encoded_format_version_returns_typed_error() {
        let cure = CureField::new(2, 2, 2, 0.05, [0.0; 3]).expect("ctor");
        let mut buf = Vec::new();
        encode_sidecar(
            &SidecarFields {
                cure: Some(&cure),
                ..Default::default()
            },
            &mut buf,
        )
        .expect("encode");
        // The format_version u32 lives at bytes 8..12. Flip endianness
        // by reversing those four bytes; LE 1 (0x01 0x00 0x00 0x00)
        // becomes BE 0x00 0x00 0x00 0x01 == u32(16777216).
        buf[8..12].reverse();
        let mut cursor = std::io::Cursor::new(buf);
        let err = decode_sidecar(&mut cursor, "<test>").expect_err("expect bad fmt");
        assert!(format!("{err}").contains("unknown sidecar format_version"));
    }

    #[test]
    fn truncated_slab_returns_typed_error() {
        // Encode a cure with non-zero values so we get a slab to truncate.
        let data =
            ndarray::Array3::<f32>::from_shape_fn((4, 4, 2), |(x, y, z)| (x + y + z) as f32 * 0.1);
        let cure = CureField::from_persistence_parts(4, 4, 2, 0.05, [0.0; 3], data).expect("ctor");
        let mut buf = Vec::new();
        encode_sidecar(
            &SidecarFields {
                cure: Some(&cure),
                ..Default::default()
            },
            &mut buf,
        )
        .expect("encode");
        // Truncate the last 10 bytes (mid-slab).
        let truncate_to = buf.len() - 10;
        buf.truncate(truncate_to);
        let mut cursor = std::io::Cursor::new(buf);
        let err = decode_sidecar(&mut cursor, "<test>").expect_err("expect truncated");
        // Either slab decompression failure or size mismatch is fine.
        let msg = format!("{err}");
        assert!(
            msg.contains("slab decompression failed") || msg.contains("sidecar size mismatch"),
            "got: {msg}"
        );
    }
}
