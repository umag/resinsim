---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
status: pattern
---

# Atomic multi-file write ordering

## Context

ADR-0015 documents the single-file atomic-write pattern for
`<path>.sim.json`: stage at `<path>.tmp`, then `fs::rename` to `<path>`.
On POSIX same-filesystem this is atomic — a consumer either sees the
old contents or the new contents, never a partial truncation.

ADR-0019 (`t2f3.5`) adds a paired binary sidecar `<stem>.fields.bin`.
Producers now write TWO files; consumers read both. This pattern
documents the rename ordering that preserves atomicity at the pair
level.

## Pattern

Always rename the **content file** (`.fields.bin`) before the
**pointer file** (`.sim.json`).

```text
1. Encode sidecar bytes to     <stem>.fields.bin.tmp
2. fsync                       <stem>.fields.bin.tmp
3. Compute sha256 of           <stem>.fields.bin.tmp
4. Write sim.json (with        <stem>.sim.json.tmp
   pointer = filename +
   byte_size + sha256)
5. fsync                       <stem>.sim.json.tmp
6. fs::rename .bin.tmp       → <stem>.fields.bin   (FIRST)
7. fs::rename .sim.json.tmp  → <stem>.sim.json     (SECOND)
```

## Why bin-first

The `<stem>.sim.json` is the only file a consumer reads to discover
the sidecar. If step 6 succeeds but step 7 fails:

- The new `.fields.bin` exists but no consumer can see it (no
  `.sim.json` references it).
- The previous `.sim.json` is still in place pointing at the OLD
  `.fields.bin` (also still there as long as the producer doesn't
  delete it).
- The orphan new `.fields.bin` is harmless storage drag — overwritten
  on the next successful save.

If we did the reverse (sim.json rename first, bin rename second), a
crash between the two renames would leave a `.sim.json` pointing at a
non-existent `.fields.bin` — visible from the consumer side as
`"missing sidecar"`. That's a *louder* failure mode but worse for
recovery: the producer must roll back the sim.json or re-encode the
sidecar before the next save lands correctly.

## Race windows

Even with correct ordering, multi-file pairs have race windows that
single-file atomic writes don't. ADR-0019 documents the full
enumeration; the table below summarises:

| Window | Description | v1 outcome |
|--------|-------------|------------|
| Disk-full mid-encode | `.bin.tmp` partially written | best-effort cleanup; orphan possible |
| Step 6 succeeds, step 7 fails | orphan `.fields.bin` exists with no sim.json reference | acceptable; overwritten next save |
| Two-process concurrent save | each rename lands independently | one consumer reads bin-from-A + json-from-B → sha256 mismatch loud error |
| Consumer reads json then bin; new save lands between | bin doesn't match json's pointer | sha256 mismatch loud error |

The v1 design accepts these as **loud, typed errors** (no silent
corruption). Closing them (e.g. fcntl advisory locks, COW-snapshot of
the pair) is future hardening.

## Stable error substrings raised by this pattern

- `"missing sidecar"` — sim.json references a `.fields.bin` that
  doesn't exist on disk
- `"sidecar sha256 mismatch"` — sidecar bytes don't match the
  pointer's sha256
- `"sidecar size mismatch"` — sidecar byte_size doesn't match the
  pointer's claim
- `"sidecar path traversal rejected"` — pointer.path attempts to
  escape sim.json's parent directory

## See also

- ADR-0015 — single-file atomic-write contract (the parent pattern
  this extends)
- ADR-0019 — multi-file pair design, race-window enumeration, stable
  error-substring table
- `docs/patterns/voxel-field-sidecar-binary-format.md` — RSFIELD
  binary format spec
- `crates/resinsim-core/src/repositories/simulation_repo.rs` —
  implementation (`save_envelope_to_path`, `encode_paired_sidecar`,
  `load_and_install_sidecar`)
