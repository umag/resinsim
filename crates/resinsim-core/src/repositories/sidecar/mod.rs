//! Voxel field binary sidecar (RSFIELD format).
//!
//! ADR-0019, t2f3.5. The five files in this module exist as a small
//! sub-module rather than a single flat `sidecar.rs` because the binary
//! format + security-hardened decoder warrants more lines of code than
//! fits one readable file. Sibling repository modules (`simulation_repo.rs`,
//! `printer_repo.rs`, `resin_repo.rs`) stay flat per ADR-0009 convention.
//!
//! # Layout
//!
//! See `docs/patterns/voxel-field-sidecar-binary-format.md` for the
//! authoritative on-disk format spec. In short: 64-byte fixed header
//! (magic `RSFIELD\0`, format_version, field_count, reserved) + N
//! variable-length FieldDescriptor records + zstd-compressed per-layer
//! slab payload.
//!
//! # Security
//!
//! The decoder treats sidecar bytes as **untrusted input from disk** and
//! defends against:
//! - Integer overflow on claimed `dim_x × dim_y × dim_z × component_size`
//! - `MAX_FIELD_ALLOCATION_BYTES` exceeded (rejected at descriptor-parse,
//!   BEFORE any allocation)
//! - `MAX_REASONABLE_LAYER_COUNT` / `MAX_FIELD_COUNT` overflow
//! - Sum-of-layer_sizes exceeding sidecar file size
//! - Magic / format_version mismatch
//! - zstd decompression bomb (per-slab bounded decompression; convenience
//!   API `zstd::decode_all` BANNED in this module)
//! - Cross-field dimension consistency mismatch
//! - Non-finite f32 values on full-field reconstruction (via the
//!   per-field `validate()` path)
//!
//! All produce typed errors carrying stable substrings (see
//! [`error::SidecarError`] / [`error::DecodeError`]).

#![cfg(feature = "field-sim")]

pub mod decoder;
pub mod encoder;
pub mod error;
pub mod format;

pub use decoder::{decode_sidecar, DecodedSidecar, FieldSidecarDecoder};
pub use encoder::{encode_sidecar, FieldSidecarEncoder, SidecarFields, SidecarOutput};
pub use error::{DecodeError, EncodeError};
pub use format::{
    FieldKind, FIELD_COMPONENT_SIZE_SCALAR, FIELD_COMPONENT_SIZE_TENSOR, MAX_FIELD_COUNT,
    MAX_REASONABLE_LAYER_COUNT, RSFIELD_FORMAT_VERSION, RSFIELD_MAGIC, SIDECAR_HEADER_LEN,
};
