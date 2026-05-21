//! On-disk format constants and field-kind enum for the RSFIELD sidecar.
//!
//! See `docs/patterns/voxel-field-sidecar-binary-format.md`.

use serde::{Deserialize, Serialize};

/// Sidecar header magic. ASCII `"RSFIELD\0"` (8 bytes).
pub const RSFIELD_MAGIC: [u8; 8] = *b"RSFIELD\0";

/// Sidecar format version. Bumped on layout-breaking changes per the
/// format-spec doc's "Forward extensibility" section.
///
/// **v1 → v2 (t2f4 / ADR-0020 §Decision x)** is technically additive
/// (one new `kind_tag = 4` variant for `FieldKind::Thermal`) but the
/// "don't care about legacy" lifecycle direction chose to advertise
/// the change via a version bump rather than relying on older
/// decoders to silently fail with `UnknownFieldKindTag`. Effect: v1
/// sidecars are now rejected with the typed `UnknownFormatVersion`
/// error instead of partial-success-with-warn. No v1 read-path is
/// retained; existing tests regenerate fixtures in lockstep with this
/// constant.
pub const RSFIELD_FORMAT_VERSION: u32 = 2;

/// Fixed header length in bytes. The first descriptor begins at offset 64.
pub const SIDECAR_HEADER_LEN: u64 = 64;

/// Upper bound on `field_count` parsed from a sidecar header.
/// We have 4 logical field kinds today; the cap allows room for future
/// growth while bounding the descriptor-allocation surface to a defensive
/// value.
pub const MAX_FIELD_COUNT: u32 = 16;

/// Upper bound on `layer_count` parsed from a field descriptor.
/// Real Mars 5 Ultra prints are in the ~5_000-10_000 layer range; the
/// cap is two orders of magnitude above realistic + bounds the
/// descriptor-index allocation against a malicious sidecar claiming
/// `u32::MAX` layers.
pub const MAX_REASONABLE_LAYER_COUNT: u32 = 100_000;

/// Per-voxel byte size for the two scalar fields (CureField,
/// PhotoinitiatorField) — one `f32`.
pub const FIELD_COMPONENT_SIZE_SCALAR: u32 = 4;

/// Per-voxel byte size for the two tensor fields (StrainField,
/// StressField) — six `f32` in Voigt order.
pub const FIELD_COMPONENT_SIZE_TENSOR: u32 = 24;

/// Tag bytes for the layout enum on disk. Only `LayerSlabs` is emitted in
/// format_version 1; the `Dense3d` variant is reserved.
pub const LAYOUT_TAG_DENSE_3D: u32 = 0;
pub const LAYOUT_TAG_LAYER_SLABS: u32 = 1;

/// Tag bytes for the compression enum on disk. v1 always emits Zstd.
pub const COMPRESSION_TAG_NONE: u32 = 0;
pub const COMPRESSION_TAG_ZSTD: u32 = 1;

/// Tag bytes for the field-kind enum on disk.
pub const FIELD_KIND_TAG_CURE: u32 = 0;
pub const FIELD_KIND_TAG_PHOTOINITIATOR: u32 = 1;
pub const FIELD_KIND_TAG_STRAIN: u32 = 2;
pub const FIELD_KIND_TAG_STRESS: u32 = 3;
/// ADR-0020 / t2f4 — Tier-2 thermal field. Per-voxel `f32` temperature
/// in °C over the full vat envelope (NOT the part bbox).
pub const FIELD_KIND_TAG_THERMAL: u32 = 4;

/// Closed enum identifying which of the five voxel-field types a
/// descriptor refers to. Wire-tagged via `kind_tag` u32 LE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Cure,
    Photoinitiator,
    Strain,
    Stress,
    /// ADR-0020 / t2f4 — vat-envelope-anchored thermal field. f32
    /// scalar per voxel (°C). Diverges from the layer-count-Z sibling
    /// fields per `docs/patterns/thermal-field-z-dim-is-spatial.md`.
    Thermal,
}

impl FieldKind {
    /// On-disk tag.
    pub fn tag(self) -> u32 {
        match self {
            Self::Cure => FIELD_KIND_TAG_CURE,
            Self::Photoinitiator => FIELD_KIND_TAG_PHOTOINITIATOR,
            Self::Strain => FIELD_KIND_TAG_STRAIN,
            Self::Stress => FIELD_KIND_TAG_STRESS,
            Self::Thermal => FIELD_KIND_TAG_THERMAL,
        }
    }

    pub fn from_tag(tag: u32) -> Option<Self> {
        match tag {
            FIELD_KIND_TAG_CURE => Some(Self::Cure),
            FIELD_KIND_TAG_PHOTOINITIATOR => Some(Self::Photoinitiator),
            FIELD_KIND_TAG_STRAIN => Some(Self::Strain),
            FIELD_KIND_TAG_STRESS => Some(Self::Stress),
            FIELD_KIND_TAG_THERMAL => Some(Self::Thermal),
            _ => None,
        }
    }

    /// On-disk human-readable name written in the descriptor.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cure => "cure",
            Self::Photoinitiator => "photoinitiator",
            Self::Strain => "strain",
            Self::Stress => "stress",
            Self::Thermal => "thermal",
        }
    }

    /// Per-voxel byte size. Scalar fields = 4; tensor fields = 24.
    pub fn component_size(self) -> u32 {
        match self {
            Self::Cure | Self::Photoinitiator | Self::Thermal => FIELD_COMPONENT_SIZE_SCALAR,
            Self::Strain | Self::Stress => FIELD_COMPONENT_SIZE_TENSOR,
        }
    }
}

/// Per-field descriptor parsed from the sidecar (write side reconstructs
/// this from each field's geometry before encoding). Kept as a flat data
/// holder; the wire encoding lives in `encoder.rs` / `decoder.rs`.
#[derive(Debug, Clone)]
pub struct FieldDescriptor {
    pub kind: FieldKind,
    pub dim_x: u32,
    pub dim_y: u32,
    pub dim_z: u32,
    pub bbox_origin: [f32; 3],
    pub voxel_size_mm: f32,
    pub component_size: u32,
    pub compression_tag: u32,
    pub layout_tag: u32,
    pub layer_count: u32,
    pub uncompressed_layer_byte_size: u64,
    pub layer_offsets: Vec<u64>,
    pub layer_sizes: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsfield_magic_is_eight_bytes() {
        assert_eq!(RSFIELD_MAGIC.len(), 8);
        assert_eq!(&RSFIELD_MAGIC, b"RSFIELD\0");
    }

    #[test]
    fn field_kind_tag_round_trips() {
        for k in [
            FieldKind::Cure,
            FieldKind::Photoinitiator,
            FieldKind::Strain,
            FieldKind::Stress,
            FieldKind::Thermal,
        ] {
            assert_eq!(FieldKind::from_tag(k.tag()), Some(k));
        }
    }

    #[test]
    fn field_kind_from_unknown_tag_returns_none() {
        assert_eq!(FieldKind::from_tag(99), None);
    }

    #[test]
    fn component_size_matches_field_kind() {
        assert_eq!(FieldKind::Cure.component_size(), 4);
        assert_eq!(FieldKind::Photoinitiator.component_size(), 4);
        assert_eq!(FieldKind::Strain.component_size(), 24);
        assert_eq!(FieldKind::Stress.component_size(), 24);
        assert_eq!(FieldKind::Thermal.component_size(), 4);
    }

    #[test]
    fn rsfield_format_version_is_v2() {
        // ADR-0020 §Decision x — bumped from 1 to 2 with t2f4. v1
        // sidecars on disk are rejected at decode time.
        assert_eq!(RSFIELD_FORMAT_VERSION, 2);
    }
}
