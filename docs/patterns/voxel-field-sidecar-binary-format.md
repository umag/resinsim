---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
status: pattern
---

# Voxel field sidecar binary format (RSFIELD)

## Context

ADR-0019 introduces the paired binary sidecar `<stem>.fields.bin`
alongside `<stem>.sim.json`. This document specifies the on-disk format.

## File layout

A sidecar file consists of (1) a fixed 64-byte header, (2) `field_count`
variable-length **FieldDescriptor** records, then (3) the payload bytes
referenced by the descriptors' `layer_offsets`.

All multi-byte integers are **little-endian**, encoded via explicit
`u32::to_le_bytes` / `u64::to_le_bytes` / `f32::to_le_bytes`. NOT
`bytemuck::cast` — host endianness must never leak.

### Header (offset 0, length 64 bytes)

| Offset | Bytes | Field | Notes |
|--------|-------|-------|-------|
| 0 | 8 | `magic` | `b"RSFIELD\0"` (capital ASCII + NUL). Mismatch → typed `"unknown sidecar magic"` |
| 8 | 4 | `format_version` u32 LE | `RSFIELD_FORMAT_VERSION = 2` (bumped from 1 by **ADR-0020 / t2f4** §Decision x — clean break, no v1 read path retained). Mismatch → typed `"unknown sidecar format_version"` |
| 12 | 4 | `field_count` u32 LE | `0 < field_count ≤ MAX_FIELD_COUNT (16)`. Overflow → typed `"implausible field_count"` |
| 16 | 48 | `reserved` | Must be all-zero for forward extension. Decoder ignores. |

### FieldDescriptor (variable, packed)

Each descriptor follows the previous; the first starts at byte 64.

| Field | Bytes | Encoding | Notes |
|-------|-------|----------|-------|
| `name_len` | 4 | u32 LE | UTF-8 byte length of `name`. 1 ≤ name_len ≤ 64 |
| `name` | `name_len` | UTF-8 bytes | `"cure"`, `"photoinitiator"`, `"strain"`, `"stress"`, `"thermal"` |
| `kind_tag` | 4 | u32 LE | 0=Cure, 1=Photoinitiator, 2=Strain, 3=Stress, **4=Thermal (ADR-0020 / t2f4)** |
| `dim_x` | 4 | u32 LE | Voxel count along X. > 0 |
| `dim_y` | 4 | u32 LE | Voxel count along Y. > 0 |
| `dim_z` | 4 | u32 LE | Voxel count along Z = layer count. > 0; also = `layer_count` below |
| `bbox_origin_x` | 4 | f32 LE | Bbox-min X in mm |
| `bbox_origin_y` | 4 | f32 LE | Bbox-min Y in mm |
| `bbox_origin_z` | 4 | f32 LE | Bbox-min Z in mm |
| `voxel_size_mm` | 4 | f32 LE | LCD pixel pitch on X-Y |
| `component_size` | 4 | u32 LE | Per-voxel byte size. 4 = `f32` (cure / photoinit / thermal); 24 = `[f32; 6]` (strain/stress) |
| `compression_tag` | 4 | u32 LE | 0=none, 1=zstd. v1 always emits 1. |
| `layout_tag` | 4 | u32 LE | 0=dense3d (unused), 1=layer_slabs. v1 always emits 1. |
| `layer_count` | 4 | u32 LE | Equal to `dim_z`. `≤ MAX_REASONABLE_LAYER_COUNT (100_000)`. Overflow → typed `"implausible layer_count"` |
| `uncompressed_layer_byte_size` | 8 | u64 LE | `= dim_x * dim_y * component_size`. Checked against MAX_FIELD_ALLOCATION_BYTES at descriptor-parse, BEFORE any allocation. Overflow → typed `"exceeds field budget"` |
| `layer_offsets` | `8 * layer_count` | u64 LE × layer_count | Each offset is bytes from sidecar start (offset 0) to the beginning of that slab's compressed bytes. `0` is reserved (no slab can start at offset 0 — header occupies it); offset of `u64::MAX` flags an empty slab (saves the offset slot but the slab has zero bytes on disk; decoder reconstructs as all-zeros) |
| `layer_sizes` | `4 * layer_count` | u32 LE × layer_count | Compressed byte size of each slab. `0` = empty slab (interpreted as all-zeros without decompression). Sum-of-non-zero-sizes + sum-of-descriptor-bytes + header (64) must ≤ sidecar file size; otherwise typed `"sidecar size mismatch"` |

### Payload section

After the last descriptor: a sequence of zstd-compressed slab bytes.
Each non-empty slab's compressed bytes start at the offset declared in
its `layer_offsets` entry and span `layer_sizes` bytes.

Slabs MAY be in any order on disk (e.g., interleaved between fields).
The decoder uses `layer_offsets` / `layer_sizes` exclusively.

## Slab payload encoding

Each slab is a 2D plane of `dim_x × dim_y` elements at the slab's
`iz` layer. Element traversal order is **iy-major, ix-minor**:

```rust
for iy in 0..dim_y {
    for ix in 0..dim_x {
        write_le(field[[ix, iy, iz]])
    }
}
```

For 4-byte fields (cure / photoinitiator), `write_le` emits 4 bytes via
`f32::to_le_bytes`. For 24-byte tensor fields (strain / stress),
`write_le` emits the 6 Voigt components (xx, yy, zz, yz, xz, xy) each
as `f32::to_le_bytes` in order — 24 bytes total per element.

The uncompressed slab payload is exactly
`uncompressed_layer_byte_size = dim_x * dim_y * component_size` bytes
long. The decoder enforces this on decompress; mismatch → typed
`"slab decompression failed"`.

### zstd parameters

- Level: 3 (default; env override `RESINSIM_ZSTD_LEVEL` accepted at
  encoder construction)
- Single-threaded mode (deterministic bit-exact output across runs)
- No dictionary
- Frame format: single zstd frame per slab (allows independent decode)

The encoder MUST NOT call `zstd::stream::copy_encode` / `zstd::encode_all`
with multi-threading enabled. Bit-exact golden files in
`crates/resinsim-core/tests/golden/` depend on determinism.

The decoder MUST NOT call `zstd::decode_all` — that API does not bound
the output buffer size and is a decompression-bomb vector. The decoder
allocates exactly `uncompressed_layer_byte_size` bytes, configures
`zstd::stream::read::Decoder::new` over the slab's compressed input
slice, and asserts the decoded count equals the expected size.

## Empty-slab semantics

`layer_sizes[iz] == 0` means the slab is logically all-zeros. The
decoder skips the read and zero-fills the field at iz. This is the
"prong (1) sparse" win: for parts that don't occupy the lowest raft
layers or the topmost layers above the model, those slabs are zero-byte
on disk regardless of in-memory representation. ADR-0019 §"Prong (1)"
records the back-of-envelope.

## Stable error substrings

Decoder errors carry exact substrings (single-source-of-truth in
ADR-0019 §"Stable error substrings"; reference also from
`crates/resinsim-core/src/repositories/sidecar/error.rs` module
docstring):

- `unknown sidecar magic`
- `unknown sidecar format_version`
- `slab decompression failed`
- `sidecar size mismatch`
- `dimension mismatch`
- `implausible layer_count`
- `implausible field_count`
- `exceeds field budget`
- `non-finite in sidecar field`
- `sidecar sha256 mismatch` (from `simulation_repo.rs`)
- `missing sidecar` (from `simulation_repo.rs`)
- `sidecar path traversal rejected` (from `SidecarPointer::validate`)

## Forward extensibility

Future format changes that **don't** break this layout (e.g. new
`kind_tag` values, new `compression_tag` values for a future codec)
can be added without bumping `RSFIELD_FORMAT_VERSION`. The decoder
returns a typed error on an unknown enum tag. Reserved header bytes
permit adding fixed-position metadata if needed.

A `RSFIELD_FORMAT_VERSION` bump is required only for layout-breaking
changes (e.g. moving the offset table inline, changing per-element
encoding for a kind). The version bump policy mirrors the sim.json
schema_version policy of ADR-0015 (old decoders reject new files with
the typed `"unknown sidecar format_version"` error).

## See also

- `docs/adr/0019-voxel-field-on-disk-persistence.md` — the
  load-bearing ADR including re-survey, decision rationale, prong (1)
  argument, and the stable error substrings table.
- `docs/patterns/atomic-multi-file-write-ordering.md` — the
  multi-file rename ordering that ensures consumers see a consistent
  sim.json + sidecar pair.
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — the Z
  axis convention shared with CureField/StrainField etc.
- `docs/patterns/anti/rust-nan-positive-validation-gap.md` —
  decoder's `is_finite` check on full-field reconstruction follows
  this two-layer-defence pattern.
