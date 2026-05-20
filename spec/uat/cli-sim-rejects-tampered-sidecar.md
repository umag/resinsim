---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
---

# UAT: `report health --in` rejects tampered or malformed sidecar with typed errors

## Rationale

ADR-0019's stable error substring table is the single source of truth
for what consumers (UAT, downstream tooling, human grep) match
against. This scenario pins the load-bearing substrings for the
security surface of the sidecar consumer path:

- `"sidecar sha256 mismatch"` — sidecar bytes don't match the
  pointer's sha256 (accidental tamper or concurrent-write race)
- `"sidecar size mismatch"` — sidecar byte_size doesn't match
- `"sidecar path traversal rejected"` — pointer.path escapes parent
- `"missing sidecar"` — sidecar file doesn't exist at the path

The decoder side also raises `"unknown sidecar magic"`, `"unknown
sidecar format_version"`, `"slab decompression failed"`, `"implausible
layer_count"`, `"implausible field_count"`, `"exceeds field budget"`,
`"non-finite in sidecar field"` — covered by unit-level tests in
`src/repositories/sidecar/decoder.rs::tests`.

See also: `crates/resinsim-core/tests/sidecar_security_integration.rs`.

## UAT-1: sha256-tampered sidecar produces typed error

```gherkin
Scenario: UAT-1 tampering the sidecar bytes after save produces sha256 mismatch
  Given a paired `model.sim.json` + `model.fields.bin` produced by a
    --voxel-cure-mm run
  When the user flips a single byte in `model.fields.bin` outside of size
  And invokes `resinsim report health --in model.sim.json`
  Then the process exits with non-zero code
  And stderr mentions "sidecar sha256 mismatch"
  And the process does not panic (no "thread 'main' panicked" in stderr)
```

## UAT-2: truncated sidecar produces size-mismatch error

```gherkin
Scenario: UAT-2 truncating the sidecar produces sidecar size mismatch
  Given a paired sim.json + fields.bin
  When the user truncates `model.fields.bin` by 10 bytes
  And invokes `resinsim report health --in model.sim.json`
  Then the process exits with non-zero code
  And stderr mentions "sidecar size mismatch"
```

## UAT-3: path-traversal sidecar pointer rejected

```gherkin
Scenario: UAT-3 sim.json with path-traversal sidecar pointer is rejected
  Given a sim.json envelope crafted with `fields_sidecar.path = "../escape.bin"`
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with non-zero code
  And stderr mentions "sidecar path traversal rejected"
```

## UAT-4: missing sidecar produces typed error

```gherkin
Scenario: UAT-4 sidecar deleted, sim.json kept produces missing sidecar
  Given a paired sim.json + fields.bin
  When the user deletes `model.fields.bin`
  And invokes `resinsim report health --in model.sim.json`
  Then the process exits with non-zero code
  And stderr mentions "missing sidecar" or "sidecar path traversal rejected"
```
