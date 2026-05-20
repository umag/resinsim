//! Typed errors for sidecar encode/decode.
//!
//! All errors carry **stable substrings** that downstream tests + UAT
//! scenarios match against (see ADR-0019 §"Stable error substrings"
//! and `docs/patterns/voxel-field-sidecar-binary-format.md`):
//!
//! - `"unknown sidecar magic"`
//! - `"unknown sidecar format_version"`
//! - `"slab decompression failed"`
//! - `"sidecar size mismatch"`
//! - `"dimension mismatch"`
//! - `"implausible layer_count"`
//! - `"implausible field_count"`
//! - `"exceeds field budget"`
//! - `"non-finite in sidecar field"`
//!
//! Two additional substrings (`"sidecar sha256 mismatch"`, `"missing
//! sidecar"`, `"sidecar path traversal rejected"`) are emitted from
//! `simulation_repo.rs` because they apply at the outer-envelope layer.
//!
//! Convention: never call `zstd::decode_all` — that API has no output
//! bound and is a decompression-bomb vector. Use
//! `zstd::stream::read::Decoder` over a bounded input slice + assert
//! the decoded byte count matches `uncompressed_layer_byte_size`.

use thiserror::Error;

/// Errors produced by the sidecar encoder.
///
/// Encoder errors are caller-side mistakes (passing a non-finite field,
/// claiming dims that overflow u64). They share the stable substring
/// convention with decoder errors for consistency.
#[derive(Debug, Error)]
pub enum EncodeError {
    /// Caller passed an empty `SidecarFields` (no fields to encode).
    #[error("sidecar must carry at least one field")]
    EmptyFields,

    /// Caller passed more fields than `MAX_FIELD_COUNT` permits. The
    /// public `SidecarFields` shape today caps this at 4, so this is a
    /// defensive guard.
    #[error("implausible field_count: got {got}, max {max}")]
    ImplausibleFieldCount { got: u32, max: u32 },

    /// Field dims claim a layer count beyond MAX_REASONABLE_LAYER_COUNT.
    #[error("implausible layer_count for {field_name}: got {got}, max {max}")]
    ImplausibleLayerCount {
        field_name: &'static str,
        got: u32,
        max: u32,
    },

    /// Field dims × component_size would overflow the field budget. The
    /// in-memory field already passed `enforce_field_budget` when
    /// constructed, so this error indicates a divergence — most likely
    /// a tampered field or an unsupported component_size.
    #[error("field {field_name} exceeds field budget: {bytes} bytes per layer × {layers} layers")]
    ExceedsFieldBudget {
        field_name: &'static str,
        bytes: u64,
        layers: u32,
    },

    /// Two fields disagree on (nx, ny, nz) — ADR-0017/0018 invariant
    /// dimension-locks cure_field, photoinitiator_field, strain_field,
    /// stress_field across the simulation.
    #[error(
        "dimension mismatch between {first_field} ({first_x}×{first_y}×{first_z}) and \
         {second_field} ({second_x}×{second_y}×{second_z})"
    )]
    DimensionMismatch {
        first_field: &'static str,
        first_x: u32,
        first_y: u32,
        first_z: u32,
        second_field: &'static str,
        second_x: u32,
        second_y: u32,
        second_z: u32,
    },

    /// IO error writing to the sink (disk full, broken pipe, etc).
    #[error("sidecar write failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors produced by the sidecar decoder.
///
/// Every variant carries one of the documented stable substrings so
/// downstream UAT + grep matches the substring rather than the variant
/// name.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// Header magic bytes do not match `RSFIELD\0`.
    #[error("unknown sidecar magic in {context} (expected RSFIELD)")]
    UnknownMagic { context: String },

    /// Header `format_version` is not equal to `RSFIELD_FORMAT_VERSION`.
    /// Future versions are forward-incompatible until a new decoder
    /// branch is added.
    #[error("unknown sidecar format_version {got} (expected {expected})")]
    UnknownFormatVersion { got: u32, expected: u32 },

    /// Per-slab zstd decompression returned an error or produced a
    /// byte count different from `uncompressed_layer_byte_size`.
    #[error("slab decompression failed for {field_name} layer {iz}: {detail}")]
    SlabDecompressionFailed {
        field_name: String,
        iz: u32,
        detail: String,
    },

    /// Sum of declared slab byte sizes exceeds the actual sidecar file
    /// size (minus header + descriptors). Surface of decompression-bomb
    /// attempts that lie about payload size.
    #[error(
        "sidecar size mismatch: descriptors imply {implied} bytes of payload but file body has {available}"
    )]
    SizeMismatch { implied: u64, available: u64 },

    /// Cross-field dim consistency check failed: the four fields are
    /// dimension-locked per ADR-0017/0018.
    #[error(
        "dimension mismatch between {first_field} ({first_x}×{first_y}×{first_z}) and \
         {second_field} ({second_x}×{second_y}×{second_z})"
    )]
    DimensionMismatch {
        first_field: String,
        first_x: u32,
        first_y: u32,
        first_z: u32,
        second_field: String,
        second_x: u32,
        second_y: u32,
        second_z: u32,
    },

    /// `layer_count` field exceeds `MAX_REASONABLE_LAYER_COUNT`.
    #[error("implausible layer_count {got} (max {max}) in descriptor for {field_name}")]
    ImplausibleLayerCount {
        field_name: String,
        got: u32,
        max: u32,
    },

    /// Header `field_count` exceeds `MAX_FIELD_COUNT`.
    #[error("implausible field_count {got} (max {max})")]
    ImplausibleFieldCount { got: u32, max: u32 },

    /// Claimed `dim_x × dim_y × component_size × layer_count` exceeds
    /// the field-budget cap. Checked BEFORE any allocation.
    #[error(
        "exceeds field budget for {field_name}: implied {implied} bytes > MAX_FIELD_ALLOCATION_BYTES ({budget})"
    )]
    ExceedsFieldBudget {
        field_name: String,
        implied: u64,
        budget: u64,
    },

    /// Full-field reconstruction detected NaN or ±Infinity. Cross-
    /// references `docs/patterns/anti/rust-nan-positive-validation-gap.md`.
    #[error("non-finite in sidecar field {field_name}: {detail}")]
    NonFinite { field_name: String, detail: String },

    /// Component size in the descriptor doesn't match the field-kind
    /// expectation (scalar = 4, tensor = 24).
    #[error(
        "component_size mismatch for {field_name}: descriptor says {got}, expected {expected}"
    )]
    ComponentSizeMismatch {
        field_name: String,
        got: u32,
        expected: u32,
    },

    /// Field-kind tag in the descriptor is not one of the four known
    /// kinds.
    #[error("unknown field kind_tag {got} in descriptor")]
    UnknownFieldKindTag { got: u32 },

    /// Descriptor's `layout_tag` is not the supported `layer_slabs` value.
    #[error("unsupported layout_tag {got} (expected layer_slabs=1)")]
    UnsupportedLayout { got: u32 },

    /// Descriptor's `compression_tag` is not the supported `zstd` value.
    #[error("unsupported compression_tag {got} (expected zstd=1)")]
    UnsupportedCompression { got: u32 },

    /// Field-name bytes are not valid UTF-8.
    #[error("field name is not valid UTF-8: {0}")]
    InvalidFieldName(std::string::FromUtf8Error),

    /// IO error reading the sidecar bytes (truncation, permission, etc).
    #[error("sidecar read failed: {0}")]
    Io(#[from] std::io::Error),

    /// Encoded field could not reconstitute its in-memory value-object
    /// (e.g. negative voxel_size_mm, zero dim, non-finite bbox_origin).
    #[error("sidecar reconstitution failed for {field_name}: {detail}")]
    Reconstitution { field_name: String, detail: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Documentation guard: the error substring `"unknown sidecar magic"`
    /// must remain stable. Downstream UAT scenarios + grep tooling key
    /// off it.
    #[test]
    fn unknown_magic_carries_stable_substring() {
        let err = DecodeError::UnknownMagic {
            context: "<test>".into(),
        };
        assert!(err.to_string().contains("unknown sidecar magic"));
    }

    #[test]
    fn unknown_format_version_carries_stable_substring() {
        let err = DecodeError::UnknownFormatVersion {
            got: 999,
            expected: 1,
        };
        assert!(err.to_string().contains("unknown sidecar format_version"));
    }

    #[test]
    fn slab_decompression_failed_carries_stable_substring() {
        let err = DecodeError::SlabDecompressionFailed {
            field_name: "strain".into(),
            iz: 7,
            detail: "truncated".into(),
        };
        assert!(err.to_string().contains("slab decompression failed"));
    }

    #[test]
    fn size_mismatch_carries_stable_substring() {
        let err = DecodeError::SizeMismatch {
            implied: 1024,
            available: 256,
        };
        assert!(err.to_string().contains("sidecar size mismatch"));
    }

    #[test]
    fn dimension_mismatch_carries_stable_substring() {
        let err = DecodeError::DimensionMismatch {
            first_field: "cure".into(),
            first_x: 8,
            first_y: 8,
            first_z: 4,
            second_field: "strain".into(),
            second_x: 8,
            second_y: 9,
            second_z: 4,
        };
        assert!(err.to_string().contains("dimension mismatch"));
    }

    #[test]
    fn implausible_layer_count_carries_stable_substring() {
        let err = DecodeError::ImplausibleLayerCount {
            field_name: "strain".into(),
            got: 1_000_000,
            max: 100_000,
        };
        assert!(err.to_string().contains("implausible layer_count"));
    }

    #[test]
    fn implausible_field_count_carries_stable_substring() {
        let err = DecodeError::ImplausibleFieldCount {
            got: 32,
            max: 16,
        };
        assert!(err.to_string().contains("implausible field_count"));
    }

    #[test]
    fn exceeds_field_budget_carries_stable_substring() {
        let err = DecodeError::ExceedsFieldBudget {
            field_name: "strain".into(),
            implied: 1 << 50,
            budget: 1 << 32,
        };
        assert!(err.to_string().contains("exceeds field budget"));
    }

    #[test]
    fn non_finite_carries_stable_substring() {
        let err = DecodeError::NonFinite {
            field_name: "strain".into(),
            detail: "NaN at (3, 4, 7)".into(),
        };
        assert!(err.to_string().contains("non-finite in sidecar field"));
    }
}
