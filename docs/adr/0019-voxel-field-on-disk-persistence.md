---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
---

# ADR-0019: Voxel field on-disk persistence — binary sidecar + zstd, no v1 compat

## Status

Accepted (Phase 4 of issue `t2f3.5-voxel-field-persistence`, 2026-05-20).

## Context

`t2f3` (ADR-0018) shipped `strain_field` + `stress_field` on `PrintSimulation`
with `#[serde(skip)]` because dense JSON serialisation of two
6-component-tensor `Array3` fields at typical Mars 5 Ultra 30 µm
resolutions produces ~250 GB of output per simulation. `cure_field` +
`photoinitiator_field` (ADR-0017) survive serde JSON, but a real lilith
torso run at 30 µm produced a 6.1 GB `sim.json` for those two scalar
fields alone. That works for v1 but means `t2f6-field-inspector` cannot
consume the voxel-resolution strain/stress data from a saved simulation —
it would have to re-run the simulation every time.

Three interacting axes of the size problem were folded into this issue:

1. **Sparse representation.** Most prints occupy a small fraction of
   the build envelope; the outside-part-bbox region is identically zero
   in all four fields.
2. **Binary sidecar.** A dense layout is mmap-friendly and supports
   partial reads per layer — critical for `t2f6-field-inspector`'s
   per-layer scrubbing UX.
3. **zstd compression.** Significantly faster decode than xz/lzma at
   similar ratios; works on dense or sparse binary layouts.

The three axes interact: sparse format determines sidecar header; zstd
layers on top of either dense or sparse. Doing them separately would
redesign the sidecar format twice.

## Decision

**Persist all four voxel fields (cure / photoinitiator / strain / stress)
via a paired binary sidecar `<stem>.fields.bin` alongside `<stem>.sim.json`.**
The sidecar uses per-layer dense slabs, each single-threaded
zstd-compressed at pinned level 3. `layer_offsets[]` in each field's
descriptor enables random-access per-layer reads. Bump
`CURRENT_SCHEMA_VERSION` from 1 to 2; the v2 loader is **v2-only** — v1
envelopes are rejected via the existing schema-version-mismatch typed
error, with no migration tool provided.

## Re-survey verdict (Rust voxel ecosystem, May 2026)

`ADR-0017 §3` surveyed and rejected OpenVDB/NanoVDB/Bonxai/hash-grid for
`t2f1` v1 (October 2025) because the Rust bindings were immature. This
issue re-surveyed at planning time. The ecosystem is **unchanged**.

| Crate | Status | Suitable here? |
|-------|--------|----------------|
| [`vdb-rs`](https://github.com/Traverse-Research/vdb-rs) (Traverse Research) | 0.5.0, August 2023 — still **read-only**; README explicitly lists "VDB Writing" as missing | ❌ no write path |
| [NanoVDB](https://github.com/AcademySoftwareFoundation/openvdb/blob/master/nanovdb/nanovdb/NanoVDB.h) Rust binding | No first-class crate; NanoVDB's `ValueT` is restricted to `float \| double \| Fp4 \| Fp8 \| Fp16 \| FpN` (one scalar per voxel) | ❌ cannot hold our 6-component tensors as one grid; would need 14 parallel grids per simulation |
| [Bonxai](https://github.com/facontidavide/Bonxai) (Faconti) | C++ header-only, no Rust binding | ❌ |
| [`voxelis`](https://crates.io/crates/voxelis) | Pure-Rust SVO-DAG; "99.999% compression" depends on structural sharing in *categorical* block-ID data | ❌ wrong domain — continuous-value tensor fields have no structural sharing |
| `svo`, `svo-rs`, `voxtree`, `building_blocks_storage` | Game-engine SVO; fixed-size leaf payloads (typically `u32` block IDs or 3D-point sets) | ❌ wrong shape for 6-component `f32` tensor per voxel |

**Next-re-survey-trigger criteria.** Re-evaluate when *any* of:

- A Rust crate reaches >100 reverse-dependents on crates.io AND supports
  write AND accepts arbitrary value-type (including 6-component tensors).
- Bonxai or OpenVDB ships first-party Rust bindings.
- Disk measurement post-shipping shows dense+zstd insufficient for
  realistic workloads, AND the next-step increment (bit-mask + packed
  values per slab) also proves insufficient.

Until then: dense+zstd stays.

## Prong (1) — Sparse representation rationale

The issue's prong (1) asked for "sparse representation for all four
voxel fields ... 50-100× smaller than dense layout without any
compression". This ADR satisfies prong (1) on the **disk axis** via
zstd-zeros-collapse on dense slabs, NOT via explicit-index sparse
encoding. The argument:

Back-of-envelope on a Mars 5 Ultra 30 µm `lilith_torso` run:

- Bbox-anchored field: nx × ny × nz = ~3000 × 2000 × 5500 ≈ 33 G voxels
- Inside-part fraction: typically 5–15 % of bbox is cured matter; for
  the lilith torso, ~10 %
- Outside-part bytes: ≈ 30 G voxels × 24 B/tensor = 720 GB (dense)
- Inside-part bytes: ≈ 3.3 G voxels × 24 B/tensor = 80 GB (dense)
- Total dense per tensor field: ~800 GB; with 2 tensor fields + 2
  scalar fields, ~1.7 TB **before compression**

After zstd-per-slab:

- Outside-part region (all zeros): zstd compresses runs of zeros to
  ~0 bytes overhead per slab — effectively free
- Inside-part region (smooth gradients): zstd LZ77 + entropy gives
  ~5-10× ratio on continuous f32 → ~10 GB per tensor field after
  compression
- 2 tensor fields + 2 scalar fields with similar ratios: total **~30 GB
  per simulation on disk**

Versus explicit-index sparse-with-indices:

- Inside-part voxels: 3.3 G voxels × (12 B index + 24 B value) = 120 GB
  (uncompressed) → ~40 GB with zstd on values
- Wins on never-allocating the outside-part region, but **loses to
  zstd-on-dense by 33%** because each non-zero voxel costs 12 bytes of
  explicit index, vs ~5 bytes of effective zstd-compressed dense
  position-encoding

Conclusion: **zstd-on-dense achieves the prong (1) target. Explicit
sparse encoding has worse disk footprint for our data shape.**

Trade-off: in-memory representation stays **dense `Array3`** per the
existing `field_budget.rs` 4 GB-per-field cap. If a future workload
needs sub-4-GB in-memory representation (truly enormous prints at
sub-30 µm), explicit sparse becomes necessary, but that's a separate
issue.

## V1 envelope drop

`schema_version=1` envelopes (cure_field + photoinitiator_field as
JSON arrays, strain_field + stress_field absent) are **no longer
supported** as of this issue. The v2 loader produces:

```
unknown schema_version 1 in <path> (expected 2) — v1 files are no
longer supported as of t2f3.5; regenerate via `resinsim sim` per
ADR-0019
```

(error substring `"unknown schema_version"` remains stable per
`cli-sim-rejects-unknown-schema-version.md`; the additional hint
sentence is additive at the end.)

**Rationale**: existing v1 `sim.json` files in the repo are test
fixtures and a single 6.1 GB `lilith_torso` development artifact —
regenerating them by re-running `resinsim sim` is cheaper than
maintaining a dual v1/v2 loader code path indefinitely. No production
or external-consumer dependency exists on v1; the canonical-
interchange policy (ADR-0015) explicitly anticipated forward-
incompatible bumps.

**Migration policy**: NONE. Users with v1 files re-run the simulation.

## Hash choice — sha2

`sha2::Sha256` (pure Rust) for sidecar integrity verification.

Trade-off considered:

- `sha2`: ubiquitous in transitive deps, "industry-standard" reduces
  reviewer cognitive load
- `blake3`: 4-8× faster, fewer dep-trees but newer

Use case is **integrity, not security** — we're catching disk
corruption / accidental tamper, not adversarial preimage attacks. Perf
isn't critical for typical sidecars (single-digit GB at worst). Pick
sha2 for v1; if a future profiler shows sha256 dominating save/load
time, swap to blake3 behind a separate ADR.

## Dependencies

```toml
zstd = { version = "=0.13.X", optional = true, default-features = false }
sha2 = { version = "=0.10.Y", optional = true }
```

Both feature-gated under `field-sim`.

- `zstd`: vendored libzstd C source via `zstd-sys`. CI must have a C
  toolchain (already does — existing crates depend on it).
- `sha2`: pure-Rust SHA-256 implementation.

**Strict `=X.Y` pin rationale**: bit-exact golden files in
`crates/resinsim-core/tests/golden/` depend on zstd's deterministic
output across patch versions. The pin documents the determinism
expectation; minor-version bumps require regenerating goldens via
`cargo test --test sidecar_golden_regenerate -- --ignored`.

## Multi-file atomic write contract

`save_to_path` writes the pair atomically. Sequence:

1. Build sidecar bytes via `FieldSidecarEncoder` writing to
   `<stem>.fields.bin.tmp` (per-slab streaming — slab-bounded memory peak)
2. `flush` + `fsync` `.bin.tmp`
3. Compute `sha256` of `.bin.tmp` (re-read from disk to match the
   consumer's view)
4. Write `<stem>.sim.json.tmp` with `SidecarPointer { path: relative
   "<stem>.fields.bin", byte_size, sha256, fields_present }`
5. `fsync` `.sim.json.tmp`
6. `fs::rename` `.bin.tmp` → `.fields.bin` **FIRST** (orphan-safe: if
   step 7 fails, the orphan is consumer-invisible because no `.sim.json`
   references it)
7. `fs::rename` `.sim.json.tmp` → `.sim.json` **SECOND**

Best-effort `.tmp` cleanup on any failure path. Overwrites silently —
matches the existing ADR-0015 POSIX policy for the `.sim.json`.

**Documented race windows** (see also
`docs/patterns/atomic-multi-file-write-ordering.md`):

| Window | Producer view | Consumer view | v1 outcome |
|--------|---------------|---------------|------------|
| Disk-full mid-(1) | error returned; orphan `.bin.tmp` leaks | unaffected | best-effort cleanup; long-running data dirs may need periodic `*.tmp` sweep |
| Step 6 succeeds, step 7 fails | error returned; orphan `.fields.bin` exists | unaffected (no `.sim.json` references it) | acceptable; overwritten on next successful save |
| Two-process concurrent save to same stem | both proceed; one wins each rename | one consumer reads bin from process A, json from process B → sha256 mismatch | typed `"sidecar sha256 mismatch"` loud error; NOT silent corruption |
| Consumer reads `.sim.json` then `.fields.bin`; new save lands between reads | n/a | sha256 mismatch | typed loud error; consumer retries |

V1 of t2f3.5 closes none of these; v1 acceptance is that **sha256
mismatch is a loud, typed error rather than silent corruption**.
Closing the consumer-read TOCTOU (fcntl `F_SETLK`, hold sidecar handle
across both reads) is a future hardening pass.

## User-facing output shape — two files, no bundle

`resinsim sim --voxel-cure-mm <N> --out model.sim.json` now produces
**two files**: `model.sim.json` AND `model.fields.bin`. Tooling
moving outputs by `cp` / `mv` / `rsync` must handle both, or use the
stem prefix.

Alternatives considered and rejected:

- **(b) Tar-zst bundle** (`model.sim.tar.zst` containing both): adds
  a tar dependency; defeats `jq` usability on the sim.json half;
  consumers like `t2f6-field-inspector` would need to extract before
  partial-read access works. Rejected — partial-read access is
  load-bearing for the t2f6 UX.
- **(c) Self-describing container** (length-prefixed sim.json bytes
  followed by sidecar bytes in one file): one file, but `jq` can't
  inspect the json directly anymore. Rejected — `jq` usability is a
  documented usage pattern from ADR-0015.

Mitigation for (a): the missing-sidecar error path names **both files
by stem** so users know what's expected, and the resinsim-viz drag-
drop UX produces an actionable error on dropping just the
`.sim.json`. See `viz-load-sim-missing-sidecar.md` UAT.

## Stable error substrings

All sidecar errors carry stable substrings for downstream grep / UAT
matching:

| Substring | Source | Triggered by | UAT scenario | Unit test |
|-----------|--------|--------------|--------------|-----------|
| `unknown sidecar magic` | `decoder::Error::MagicMismatch` | Header bytes don't match `RSFIELD\0` | (covered by negative integration) | `decoder::tests::corrupt_magic_returns_typed_error` |
| `unknown sidecar format_version` | `decoder::Error::FormatVersionMismatch` | `format_version != RSFIELD_FORMAT_VERSION (1)` | (covered by negative integration) | `decoder::tests::be_encoded_format_version_returns_typed_error` |
| `slab decompression failed` | `decoder::Error::SlabDecompression` | zstd decode returns Err mid-slab | (covered by negative integration) | `decoder::tests::truncated_slab_returns_typed_error` |
| `sidecar size mismatch` | `decoder::Error::SizeMismatch` | Sum of layer_sizes > sidecar file size minus header+descriptors | `cli-sim-rejects-tampered-sidecar.md` UAT-2 | `decoder::tests::implied_size_exceeds_file_typed_error` |
| `dimension mismatch` | `decoder::Error::DimensionMismatch` | strain.dims ≠ stress.dims ≠ cure.dims when all present | (integration) | `decoder::tests::cross_field_dim_mismatch_typed_error` |
| `implausible layer_count` | `decoder::Error::ImplausibleLayerCount` | `layer_count > MAX_REASONABLE_LAYER_COUNT (100_000)` | `cli-sim-rejects-tampered-sidecar.md` UAT-3 | `decoder::tests::layer_count_4b_typed_error` |
| `implausible field_count` | `decoder::Error::ImplausibleFieldCount` | `field_count > MAX_FIELD_COUNT (16)` | (negative integration) | `decoder::tests::field_count_overflow_typed_error` |
| `exceeds field budget` | `decoder::Error::ExceedsFieldBudget` | `uncompressed_layer_byte_size × layer_count > MAX_FIELD_ALLOCATION_BYTES` at descriptor-parse | `cli-sim-rejects-tampered-sidecar.md` UAT-4 (decompression bomb) | `decoder::tests::dim_overflow_typed_error_pre_alloc` |
| `non-finite in sidecar field` | `decoder::Error::NonFinite` | Field::validate() returns Err on full-field reconstruction | (negative integration) | `decoder::tests::nan_in_strain_typed_error` |
| `sidecar sha256 mismatch` | `simulation_repo::Error::Sha256Mismatch` | Sidecar sha256 ≠ pointer's sha256 | `cli-sim-rejects-tampered-sidecar.md` UAT-1 | `sidecar_security_integration::sha256_tamper_typed_error` |
| `missing sidecar` | `simulation_repo::Error::MissingSidecar` | `.fields.bin` not present at `SidecarPointer.path` | `viz-load-sim-missing-sidecar.md` UAT-1 | `sidecar_security_integration::missing_bin_typed_error` |
| `sidecar path traversal rejected` | `SidecarPointer::Error::PathTraversal` | path is absolute, has `..`/`.` components, contains NUL, symlink-escapes parent, or resolves to a non-regular-file | `cli-sim-rejects-tampered-sidecar.md` UAT-5/6 | `sidecar_security_integration::path_traversal_typed_error` |

This table is the single source of truth — code, tests, and UATs reference
the same substring strings.

## Fixture audit

(Populated by Phase 4 step 13 — `find . -name '*.sim.json' -not -path
'./target/*'` enumerates every fixture; per-file outcome recorded
here.)

| Path | Outcome | Notes |
|------|---------|-------|
| TBD | TBD | filled in by step 13 |

## Consequences

- Existing `v1` `sim.json` files become unreadable (loud
  `unknown schema_version` error). Users regenerate by re-running
  `resinsim sim`.
- `cure_field` + `photoinitiator_field` no longer round-trip via
  `serde_json` — all four fields persist via sidecar only.
  `print_simulation.rs` carries `#[serde(skip)]` on all four.
- `simulation_repo.rs` extends with multi-file atomic write +
  bounded-decompression sidecar load path.
- New module `crates/resinsim-core/src/repositories/sidecar/`
  (5-file split: mod, format, encoder, decoder, error) under
  `field-sim` feature gate.
- New deps: `zstd` (vendored libzstd via zstd-sys) + `sha2`,
  both feature-gated and strictly version-pinned.
- `resinsim sim --voxel-cure-mm <N>` produces TWO files instead of
  one. The CLI `--help` mentions this; `--load-sim` errors name both
  files when one is missing.
- `t2f6-field-inspector` (consumer) is unblocked — strain/stress
  voxel data is now persistent and reloadable.
- `resinsim report health --in <v1.sim.json>` returns the existing
  schema_version-rejection error with a regeneration hint appended.
- Existing UAT `cli-sim-rejects-unknown-schema-version.md` continues
  to apply — the schema_version error surface is stable.

## References

- ADR-0009 — repositories vs IO placement (sidecar lives under
  `repositories/sidecar/`).
- ADR-0015 — sim.json canonical interchange. This issue bumps
  schema_version 1 → 2 and amends §Versioning history (v1 drop policy).
- ADR-0017 — voxel cure field + photoinitiator depletion. §3
  surveyed and rejected OpenVDB/NanoVDB/Bonxai for `t2f1` v1; this
  ADR re-surveyed (verdict unchanged).
- ADR-0018 — shrinkage strain + stress accumulation. The folded
  LOW finding "v2 sim.json roundtrip test — deferred until v2 bump"
  is RESOLVED by this issue.
- `docs/patterns/voxel-field-sidecar-binary-format.md` — binary
  format spec.
- `docs/patterns/atomic-multi-file-write-ordering.md` — multi-file
  rename ordering + race-window enumeration.
- `docs/patterns/anti/rust-nan-positive-validation-gap.md` —
  cross-referenced from decoder.rs for non-finite handling.
- `feedback_memory_tradeoffs.md` — accepts dense in-memory peak.
- `feedback_no_ora_commits.md` — research-plan markdown at the
  ora root is NOT updated by this PR.
