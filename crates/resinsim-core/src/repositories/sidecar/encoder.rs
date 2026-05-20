//! Sidecar encoder. ADR-0019. Writes the four optional voxel fields
//! into the RSFIELD binary format.
//!
//! # Memory profile
//!
//! Two-pass in-memory: compress each slab into a per-slab `Vec<u8>`,
//! compute offsets, then write the header + descriptors + payload to
//! the sink. Peak memory is the total compressed-bytes size (typically
//! 1-2 GB for a real Mars 5 Ultra run, bounded by zstd's compression
//! ratio). Per `feedback_memory_tradeoffs.md` the user prefers
//! simple-and-peak-RAM-hungry over streaming for v1.
//!
//! # Determinism
//!
//! zstd encoder is single-threaded with explicit `Level(3)` (or env
//! `RESINSIM_ZSTD_LEVEL` override) so the same input produces
//! byte-equal output across runs. Golden files in
//! `crates/resinsim-core/tests/golden/` depend on this.
//!
//! # Banned APIs
//!
//! - `zstd::encode_all` — wraps `Encoder` but does not document the
//!   single-threaded posture; use `zstd::stream::write::Encoder::new`
//!   directly so we control the parameters.

use std::io::Write;

use crate::values::field_budget::active_budget_bytes;
use crate::values::{CureField, PhotoinitiatorField, StrainField, StressField};

use super::error::EncodeError;
use super::format::{
    FieldKind, COMPRESSION_TAG_ZSTD, LAYOUT_TAG_LAYER_SLABS, MAX_FIELD_COUNT,
    MAX_REASONABLE_LAYER_COUNT, RSFIELD_FORMAT_VERSION, RSFIELD_MAGIC, SIDECAR_HEADER_LEN,
};

/// Convenience handle to the four field references the encoder accepts.
/// At least one must be `Some`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SidecarFields<'a> {
    pub cure: Option<&'a CureField>,
    pub photoinitiator: Option<&'a PhotoinitiatorField>,
    pub strain: Option<&'a StrainField>,
    pub stress: Option<&'a StressField>,
}

impl<'a> SidecarFields<'a> {
    pub fn field_count(&self) -> u32 {
        [
            self.cure.is_some(),
            self.photoinitiator.is_some(),
            self.strain.is_some(),
            self.stress.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count() as u32
    }
}

/// Counters returned alongside the encoded bytes.
#[derive(Debug, Clone, Copy)]
pub struct SidecarOutput {
    pub byte_size: u64,
    pub field_count: u32,
}

/// Encode `fields` to the RSFIELD binary format and write to `sink`.
/// Returns the total byte count written.
pub fn encode_sidecar<W: Write>(
    fields: &SidecarFields<'_>,
    sink: &mut W,
) -> Result<SidecarOutput, EncodeError> {
    let encoder = FieldSidecarEncoder::new();
    encoder.encode(fields, sink)
}

/// Stateful encoder. Today this just carries a zstd level; future
/// versions may carry dictionary handles or other parameters.
pub struct FieldSidecarEncoder {
    zstd_level: i32,
}

impl Default for FieldSidecarEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldSidecarEncoder {
    /// Construct an encoder pinned to zstd level 3 (or the
    /// `RESINSIM_ZSTD_LEVEL` env override). Single-threaded.
    pub fn new() -> Self {
        let zstd_level = std::env::var("RESINSIM_ZSTD_LEVEL")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(3);
        Self { zstd_level }
    }

    /// Override the zstd level explicitly (tests).
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = level;
        self
    }

    /// Encode `fields` to `sink`. Returns total bytes written.
    pub fn encode<W: Write>(
        &self,
        fields: &SidecarFields<'_>,
        sink: &mut W,
    ) -> Result<SidecarOutput, EncodeError> {
        let field_count = fields.field_count();
        if field_count == 0 {
            return Err(EncodeError::EmptyFields);
        }
        if field_count > MAX_FIELD_COUNT {
            return Err(EncodeError::ImplausibleFieldCount {
                got: field_count,
                max: MAX_FIELD_COUNT,
            });
        }

        // Cross-field dimension lock (ADR-0017/0018 invariant). The
        // sidecar contract is that all present fields share (nx, ny, nz).
        check_cross_field_dimensions(fields)?;

        // Pass 1 — compress every slab in memory. We need this layout
        // before we can emit the descriptors (which carry the
        // layer_offsets[] index).
        let mut compressed_fields: Vec<CompressedField> = Vec::new();
        if let Some(f) = fields.cure {
            compressed_fields.push(self.compress_cure_field(f)?);
        }
        if let Some(f) = fields.photoinitiator {
            compressed_fields.push(self.compress_photoinit_field(f, fields.cure_geometry())?);
        }
        if let Some(f) = fields.strain {
            compressed_fields.push(self.compress_strain_field(f)?);
        }
        if let Some(f) = fields.stress {
            compressed_fields.push(self.compress_stress_field(f)?);
        }

        // Pass 2 — compute descriptor byte sizes + layer_offsets, write
        // header + descriptors + payload.
        let descriptor_bytes_total: u64 = compressed_fields
            .iter()
            .map(|cf| cf.descriptor_byte_size())
            .sum();
        let mut payload_cursor = SIDECAR_HEADER_LEN + descriptor_bytes_total;
        for cf in compressed_fields.iter_mut() {
            for (idx, slab) in cf.slabs.iter().enumerate() {
                if slab.is_empty() {
                    cf.layer_offsets[idx] = u64::MAX;
                    cf.layer_sizes[idx] = 0;
                } else {
                    cf.layer_offsets[idx] = payload_cursor;
                    cf.layer_sizes[idx] = slab.len() as u32;
                    payload_cursor += slab.len() as u64;
                }
            }
        }
        let total_bytes = payload_cursor;

        // Write header.
        sink.write_all(&RSFIELD_MAGIC)?;
        sink.write_all(&RSFIELD_FORMAT_VERSION.to_le_bytes())?;
        sink.write_all(&field_count.to_le_bytes())?;
        // 48 reserved bytes — must be zero per format spec.
        sink.write_all(&[0u8; 48])?;

        // Write descriptors in the same order as compressed_fields.
        for cf in &compressed_fields {
            cf.write_descriptor(sink)?;
        }

        // Write payload — non-empty slabs in order.
        for cf in &compressed_fields {
            for slab in &cf.slabs {
                if !slab.is_empty() {
                    sink.write_all(slab)?;
                }
            }
        }

        Ok(SidecarOutput {
            byte_size: total_bytes,
            field_count,
        })
    }

    fn compress_cure_field(&self, field: &CureField) -> Result<CompressedField, EncodeError> {
        let (nx, ny, nz) = field.dimensions();
        check_layer_count("cure", nz)?;
        let layer_byte_size = layer_byte_size("cure", nx, ny, FieldKind::Cure.component_size())?;
        let total = layer_byte_size.saturating_mul(u64::from(nz));
        if total > active_budget_bytes() {
            return Err(EncodeError::ExceedsFieldBudget {
                field_name: "cure",
                bytes: layer_byte_size,
                layers: nz,
            });
        }
        let data = field.data();
        let mut slabs = Vec::with_capacity(nz as usize);
        for iz in 0..nz {
            let mut uncompressed = Vec::with_capacity(layer_byte_size as usize);
            let mut all_zero = true;
            for iy in 0..ny {
                for ix in 0..nx {
                    let v = data[[ix as usize, iy as usize, iz as usize]];
                    if v != 0.0 {
                        all_zero = false;
                    }
                    uncompressed.extend_from_slice(&v.to_le_bytes());
                }
            }
            slabs.push(if all_zero {
                Vec::new()
            } else {
                self.zstd_compress(&uncompressed)?
            });
        }
        Ok(CompressedField {
            kind: FieldKind::Cure,
            dim_x: nx,
            dim_y: ny,
            dim_z: nz,
            bbox_origin: field.bbox_min_mm(),
            voxel_size_mm: field.voxel_size_mm(),
            uncompressed_layer_byte_size: layer_byte_size,
            layer_offsets: vec![0u64; nz as usize],
            layer_sizes: vec![0u32; nz as usize],
            slabs,
        })
    }

    fn compress_photoinit_field(
        &self,
        field: &PhotoinitiatorField,
        geometry_donor: Option<Geometry>,
    ) -> Result<CompressedField, EncodeError> {
        let (nx, ny, nz) = field.dimensions();
        check_layer_count("photoinitiator", nz)?;
        let layer_byte_size = layer_byte_size(
            "photoinitiator",
            nx,
            ny,
            FieldKind::Photoinitiator.component_size(),
        )?;
        let total = layer_byte_size.saturating_mul(u64::from(nz));
        if total > active_budget_bytes() {
            return Err(EncodeError::ExceedsFieldBudget {
                field_name: "photoinitiator",
                bytes: layer_byte_size,
                layers: nz,
            });
        }
        // Photoinit doesn't own bbox/voxel_size. Inherit from a peer
        // (cure / strain / stress) per ADR-0019; if no peer field is
        // present, the descriptor gets a sentinel zero geometry.
        let (bbox_origin, voxel_size_mm) =
            geometry_donor.map(|g| (g.bbox_origin, g.voxel_size_mm)).unwrap_or(([0.0; 3], 0.0));
        let data = field.data();
        let mut slabs = Vec::with_capacity(nz as usize);
        for iz in 0..nz {
            let mut uncompressed = Vec::with_capacity(layer_byte_size as usize);
            // PhotoinitiatorField initialises uniform; "empty" semantics
            // (all-zero slab) would lose the initial_concentration. We
            // always emit; sparsity wins via zstd on the (typically very
            // smooth) values.
            for iy in 0..ny {
                for ix in 0..nx {
                    let v = data[[ix as usize, iy as usize, iz as usize]];
                    uncompressed.extend_from_slice(&v.to_le_bytes());
                }
            }
            slabs.push(self.zstd_compress(&uncompressed)?);
        }
        Ok(CompressedField {
            kind: FieldKind::Photoinitiator,
            dim_x: nx,
            dim_y: ny,
            dim_z: nz,
            bbox_origin,
            voxel_size_mm,
            uncompressed_layer_byte_size: layer_byte_size,
            layer_offsets: vec![0u64; nz as usize],
            layer_sizes: vec![0u32; nz as usize],
            slabs,
        })
    }

    fn compress_strain_field(&self, field: &StrainField) -> Result<CompressedField, EncodeError> {
        let (nx, ny, nz) = field.dimensions();
        check_layer_count("strain", nz)?;
        let layer_byte_size =
            layer_byte_size("strain", nx, ny, FieldKind::Strain.component_size())?;
        let total = layer_byte_size.saturating_mul(u64::from(nz));
        if total > active_budget_bytes() {
            return Err(EncodeError::ExceedsFieldBudget {
                field_name: "strain",
                bytes: layer_byte_size,
                layers: nz,
            });
        }
        let data = field.data();
        let mut slabs = Vec::with_capacity(nz as usize);
        for iz in 0..nz {
            let mut uncompressed = Vec::with_capacity(layer_byte_size as usize);
            let mut all_zero = true;
            for iy in 0..ny {
                for ix in 0..nx {
                    let t = data[[ix as usize, iy as usize, iz as usize]];
                    let comps = t.components();
                    for c in &comps {
                        if *c != 0.0 {
                            all_zero = false;
                        }
                        uncompressed.extend_from_slice(&c.to_le_bytes());
                    }
                }
            }
            slabs.push(if all_zero {
                Vec::new()
            } else {
                self.zstd_compress(&uncompressed)?
            });
        }
        Ok(CompressedField {
            kind: FieldKind::Strain,
            dim_x: nx,
            dim_y: ny,
            dim_z: nz,
            bbox_origin: field.bbox_min_mm(),
            voxel_size_mm: field.voxel_size_mm(),
            uncompressed_layer_byte_size: layer_byte_size,
            layer_offsets: vec![0u64; nz as usize],
            layer_sizes: vec![0u32; nz as usize],
            slabs,
        })
    }

    fn compress_stress_field(&self, field: &StressField) -> Result<CompressedField, EncodeError> {
        let (nx, ny, nz) = field.dimensions();
        check_layer_count("stress", nz)?;
        let layer_byte_size =
            layer_byte_size("stress", nx, ny, FieldKind::Stress.component_size())?;
        let total = layer_byte_size.saturating_mul(u64::from(nz));
        if total > active_budget_bytes() {
            return Err(EncodeError::ExceedsFieldBudget {
                field_name: "stress",
                bytes: layer_byte_size,
                layers: nz,
            });
        }
        let data = field.data();
        let mut slabs = Vec::with_capacity(nz as usize);
        for iz in 0..nz {
            let mut uncompressed = Vec::with_capacity(layer_byte_size as usize);
            let mut all_zero = true;
            for iy in 0..ny {
                for ix in 0..nx {
                    let t = data[[ix as usize, iy as usize, iz as usize]];
                    let comps = t.components();
                    for c in &comps {
                        if *c != 0.0 {
                            all_zero = false;
                        }
                        uncompressed.extend_from_slice(&c.to_le_bytes());
                    }
                }
            }
            slabs.push(if all_zero {
                Vec::new()
            } else {
                self.zstd_compress(&uncompressed)?
            });
        }
        Ok(CompressedField {
            kind: FieldKind::Stress,
            dim_x: nx,
            dim_y: ny,
            dim_z: nz,
            bbox_origin: field.bbox_min_mm(),
            voxel_size_mm: field.voxel_size_mm(),
            uncompressed_layer_byte_size: layer_byte_size,
            layer_offsets: vec![0u64; nz as usize],
            layer_sizes: vec![0u32; nz as usize],
            slabs,
        })
    }

    fn zstd_compress(&self, input: &[u8]) -> Result<Vec<u8>, EncodeError> {
        // Single-threaded by construction; level pinned for determinism.
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), self.zstd_level)?;
        encoder.write_all(input)?;
        Ok(encoder.finish()?)
    }
}

/// Per-field geometry tuple. Carried separately because
/// PhotoinitiatorField doesn't own bbox/voxel_size (it's dimension-
/// locked to CureField).
#[derive(Clone, Copy)]
struct Geometry {
    bbox_origin: [f32; 3],
    voxel_size_mm: f32,
}

impl<'a> SidecarFields<'a> {
    /// Pick a geometry donor from cure → strain → stress for the
    /// photoinit descriptor's bbox/voxel_size fields.
    fn cure_geometry(&self) -> Option<Geometry> {
        if let Some(f) = self.cure {
            Some(Geometry {
                bbox_origin: f.bbox_min_mm(),
                voxel_size_mm: f.voxel_size_mm(),
            })
        } else if let Some(f) = self.strain {
            Some(Geometry {
                bbox_origin: f.bbox_min_mm(),
                voxel_size_mm: f.voxel_size_mm(),
            })
        } else {
            self.stress.map(|f| Geometry {
                bbox_origin: f.bbox_min_mm(),
                voxel_size_mm: f.voxel_size_mm(),
            })
        }
    }
}

struct CompressedField {
    kind: FieldKind,
    dim_x: u32,
    dim_y: u32,
    dim_z: u32,
    bbox_origin: [f32; 3],
    voxel_size_mm: f32,
    uncompressed_layer_byte_size: u64,
    layer_offsets: Vec<u64>,
    layer_sizes: Vec<u32>,
    slabs: Vec<Vec<u8>>,
}

impl CompressedField {
    /// Byte size of this field's descriptor on disk:
    /// 4 (name_len) + name.len() + 4 (kind_tag) + 4×3 (dims) +
    /// 4×3 (bbox) + 4 (voxel_size) + 4 (component_size) +
    /// 4 (compression) + 4 (layout) + 4 (layer_count) +
    /// 8 (uncompressed_layer_byte_size) +
    /// 8 × layer_count (offsets) + 4 × layer_count (sizes)
    fn descriptor_byte_size(&self) -> u64 {
        let name_bytes = self.kind.name().as_bytes().len() as u64;
        let fixed = 4 + name_bytes + 4 + 4 * 3 + 4 * 3 + 4 + 4 + 4 + 4 + 4 + 8;
        let variable = u64::from(self.dim_z) * (8 + 4);
        fixed + variable
    }

    fn write_descriptor<W: Write>(&self, sink: &mut W) -> Result<(), EncodeError> {
        let name = self.kind.name().as_bytes();
        sink.write_all(&(name.len() as u32).to_le_bytes())?;
        sink.write_all(name)?;
        sink.write_all(&self.kind.tag().to_le_bytes())?;
        sink.write_all(&self.dim_x.to_le_bytes())?;
        sink.write_all(&self.dim_y.to_le_bytes())?;
        sink.write_all(&self.dim_z.to_le_bytes())?;
        sink.write_all(&self.bbox_origin[0].to_le_bytes())?;
        sink.write_all(&self.bbox_origin[1].to_le_bytes())?;
        sink.write_all(&self.bbox_origin[2].to_le_bytes())?;
        sink.write_all(&self.voxel_size_mm.to_le_bytes())?;
        sink.write_all(&self.kind.component_size().to_le_bytes())?;
        sink.write_all(&COMPRESSION_TAG_ZSTD.to_le_bytes())?;
        sink.write_all(&LAYOUT_TAG_LAYER_SLABS.to_le_bytes())?;
        sink.write_all(&self.dim_z.to_le_bytes())?; // layer_count == dim_z
        sink.write_all(&self.uncompressed_layer_byte_size.to_le_bytes())?;
        for off in &self.layer_offsets {
            sink.write_all(&off.to_le_bytes())?;
        }
        for sz in &self.layer_sizes {
            sink.write_all(&sz.to_le_bytes())?;
        }
        Ok(())
    }
}

fn check_layer_count(field_name: &'static str, nz: u32) -> Result<(), EncodeError> {
    if nz > MAX_REASONABLE_LAYER_COUNT {
        Err(EncodeError::ImplausibleLayerCount {
            field_name,
            got: nz,
            max: MAX_REASONABLE_LAYER_COUNT,
        })
    } else {
        Ok(())
    }
}

fn layer_byte_size(
    field_name: &'static str,
    nx: u32,
    ny: u32,
    component_size: u32,
) -> Result<u64, EncodeError> {
    u64::from(nx)
        .checked_mul(u64::from(ny))
        .and_then(|v| v.checked_mul(u64::from(component_size)))
        .ok_or(EncodeError::ExceedsFieldBudget {
            field_name,
            bytes: u64::MAX,
            layers: 0,
        })
}

#[allow(unused_assignments)]
fn check_cross_field_dimensions(fields: &SidecarFields<'_>) -> Result<(), EncodeError> {
    let mut first: Option<(&'static str, u32, u32, u32)> = None;
    if let Some(f) = fields.cure {
        let (x, y, z) = f.dimensions();
        first = Some(("cure", x, y, z));
    }
    if let Some(f) = fields.photoinitiator {
        let (x, y, z) = f.dimensions();
        match first {
            None => first = Some(("photoinitiator", x, y, z)),
            Some((n, fx, fy, fz)) if (fx, fy, fz) != (x, y, z) => {
                return Err(EncodeError::DimensionMismatch {
                    first_field: n,
                    first_x: fx,
                    first_y: fy,
                    first_z: fz,
                    second_field: "photoinitiator",
                    second_x: x,
                    second_y: y,
                    second_z: z,
                });
            }
            _ => {}
        }
    }
    if let Some(f) = fields.strain {
        let (x, y, z) = f.dimensions();
        match first {
            None => first = Some(("strain", x, y, z)),
            Some((n, fx, fy, fz)) if (fx, fy, fz) != (x, y, z) => {
                return Err(EncodeError::DimensionMismatch {
                    first_field: n,
                    first_x: fx,
                    first_y: fy,
                    first_z: fz,
                    second_field: "strain",
                    second_x: x,
                    second_y: y,
                    second_z: z,
                });
            }
            _ => {}
        }
    }
    if let Some(f) = fields.stress {
        let (x, y, z) = f.dimensions();
        match first {
            None => first = Some(("stress", x, y, z)),
            Some((n, fx, fy, fz)) if (fx, fy, fz) != (x, y, z) => {
                return Err(EncodeError::DimensionMismatch {
                    first_field: n,
                    first_x: fx,
                    first_y: fy,
                    first_z: fz,
                    second_field: "stress",
                    second_x: x,
                    second_y: y,
                    second_z: z,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fields_returns_typed_error() {
        let fields = SidecarFields::default();
        let mut buf = Vec::new();
        let err = encode_sidecar(&fields, &mut buf).expect_err("empty must fail");
        assert!(matches!(err, EncodeError::EmptyFields));
    }

    #[test]
    fn cure_only_encodes_without_panicking() {
        let cure = CureField::new(2, 2, 2, 0.05, [0.0; 3]).expect("ctor");
        let fields = SidecarFields {
            cure: Some(&cure),
            ..Default::default()
        };
        let mut buf = Vec::new();
        let out = encode_sidecar(&fields, &mut buf).expect("encode");
        assert!(out.byte_size > SIDECAR_HEADER_LEN);
        assert_eq!(out.field_count, 1);
        // Magic + format_version are at the start.
        assert_eq!(&buf[0..8], &RSFIELD_MAGIC);
        assert_eq!(
            u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            RSFIELD_FORMAT_VERSION
        );
    }

    #[test]
    fn all_zero_cure_field_emits_empty_slabs() {
        let cure = CureField::new(4, 4, 3, 0.05, [0.0; 3]).expect("ctor");
        let fields = SidecarFields {
            cure: Some(&cure),
            ..Default::default()
        };
        let mut buf = Vec::new();
        let out = encode_sidecar(&fields, &mut buf).expect("encode");
        // With all-zero slabs, total bytes = header + descriptor only
        // (no payload). Descriptor = 4 (name_len) + 4 (name "cure") +
        // 4 (kind_tag) + 4×3 (dims) + 4×3 (bbox) + 4 (voxel_size) +
        // 4 (component) + 4 (compression) + 4 (layout) + 4 (layer_count)
        // + 8 (uncompressed_layer_byte_size) + 3 × (8 + 4) (offsets+sizes)
        // = 4 + 4 + 4 + 12 + 12 + 4 + 4 + 4 + 4 + 4 + 8 + 36 = 100
        assert_eq!(out.byte_size, SIDECAR_HEADER_LEN + 100);
    }

    #[test]
    fn dimension_mismatch_returns_typed_error() {
        let cure = CureField::new(2, 2, 2, 0.05, [0.0; 3]).expect("ctor");
        let strain = StrainField::new(2, 3, 2, 0.05, [0.0; 3]).expect("ctor");
        let fields = SidecarFields {
            cure: Some(&cure),
            strain: Some(&strain),
            ..Default::default()
        };
        let mut buf = Vec::new();
        let err = encode_sidecar(&fields, &mut buf).expect_err("mismatch");
        assert!(format!("{err}").contains("dimension mismatch"));
    }

    #[test]
    fn determinism_two_encodes_produce_equal_bytes() {
        let cure = CureField::new(3, 3, 2, 0.05, [1.0, 2.0, 3.0]).expect("ctor");
        let fields = SidecarFields {
            cure: Some(&cure),
            ..Default::default()
        };
        let mut a = Vec::new();
        let mut b = Vec::new();
        encode_sidecar(&fields, &mut a).expect("a");
        encode_sidecar(&fields, &mut b).expect("b");
        assert_eq!(a, b, "zstd output must be deterministic across runs");
    }
}
