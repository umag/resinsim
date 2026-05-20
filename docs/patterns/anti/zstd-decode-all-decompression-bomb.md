---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
status: anti-pattern
---

# Anti-pattern: `zstd::decode_all` for sidecar / untrusted-source decompression

## Symptom

A 1 KB malicious or corrupt zstd-compressed file claims to decompress
to 100+ GB. The default `zstd::decode_all(input)` convenience API
grows the output buffer indefinitely until OOM. Process is killed by
the kernel; downstream tooling sees an unrelated crash with no
attribution.

## Why it looks correct

`zstd::decode_all` is documented as the simplest API. Its signature
`fn decode_all(input: impl Read) -> Vec<u8>` reads naturally. The
upstream zstd crate gives no warnings about output unboundedness —
it's a thin wrapper over `zstd_safe::Decoder` that resizes the
output Vec to whatever the zstd frame claims.

## Why it's wrong

zstd frames carry no inherent upper bound on decompressed size. The
"content size" field in the frame header is set by the encoder and
trusted by the decoder. An attacker (or a corrupt file) controls
this number.

Real-world impact: in the `t2f3.5-voxel-field-persistence` lifecycle,
the sidecar decoder MUST defend against decompression bombs because
sidecar files come from disk and can be tampered with after the
producer wrote them.

## The right pattern

Read the producer's declared `uncompressed_layer_byte_size` from the
descriptor (which we validate against `MAX_FIELD_ALLOCATION_BYTES`
BEFORE allocating), allocate exactly that buffer, and assert the
decoded byte count equals the expected size:

```rust
use std::io::Read;

// `compressed` is bounded by the sidecar file size (read into a Vec
// of known capacity). `expected` is bounded by
// MAX_FIELD_ALLOCATION_BYTES at descriptor-parse time.
let expected = descriptor.uncompressed_layer_byte_size as usize;
let mut decoded = Vec::with_capacity(expected);
let mut decoder = zstd::stream::read::Decoder::new(&compressed[..])?;
let mut buffer = vec![0u8; 8192];
loop {
    let n = decoder.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    if decoded.len() + n > expected {
        // Decoder produced MORE than the descriptor claimed —
        // bomb attempt or corrupt input.
        return Err(SlabDecompressionFailed {
            field_name: ...,
            iz: ...,
            detail: format!("decoded > expected ({expected})"),
        });
    }
    decoded.extend_from_slice(&buffer[..n]);
}
if decoded.len() != expected {
    // Decoder produced FEWER than the descriptor claimed — corrupt
    // input or truncated slab.
    return Err(SlabDecompressionFailed { ... });
}
```

Two invariants enforced:
1. Decoder cannot produce more than `expected` bytes — overflow
   detected per-chunk inside the read loop.
2. Decoder must produce exactly `expected` bytes — undercount
   detected after the read loop.

## When to apply

Any time you `zstd::decompress` data that came from disk, network,
or any other source you don't control end-to-end. The convenience
API is fine for in-process compression where producer + consumer
are the same code path.

## See also

- `docs/adr/0019-voxel-field-on-disk-persistence.md` §"Bounded
  decompression" — the load-bearing context for this anti-pattern.
- `crates/resinsim-core/src/repositories/sidecar/decoder.rs::read_slab`
  — the canonical implementation in this codebase.
- `docs/patterns/voxel-field-sidecar-binary-format.md` §"zstd
  parameters" — banned-APIs subsection lists `zstd::decode_all`
  explicitly.
