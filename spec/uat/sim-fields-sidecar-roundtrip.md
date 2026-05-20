---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
---

# UAT: `--voxel-cure-mm` paired sim.json + sidecar round-trips

## Rationale

ADR-0019 introduces the binary sidecar that carries all four voxel
fields (cure / photoinitiator / strain / stress) outside the JSON
envelope. The load-bearing UAT is end-to-end: a Tier-2 voxel-mode CTB
run via `resinsim sim --voxel-cure-mm <N>` produces both a
`<stem>.sim.json` AND a `<stem>.fields.bin`, and the pair reloads
losslessly via `resinsim report health --in <PATH>` or
`resinsim-viz --load-sim <PATH>`.

UAT-1 below pins the pipeline-level contract. The unit-level
roundtrip + byte-identity invariants are covered in
`crates/resinsim-core/tests/sidecar_roundtrip_integration.rs`.

See also:
- `docs/adr/0019-voxel-field-on-disk-persistence.md` — the
  load-bearing ADR.
- `docs/patterns/voxel-field-sidecar-binary-format.md` — RSFIELD format
  spec.
- `cli-sim-rejects-tampered-sidecar.md` — sha256 mismatch scenario.
- `viz-load-sim-missing-sidecar.md` — missing-bin scenario.

## UAT-1: `--voxel-cure-mm` produces paired files

```gherkin
Scenario: UAT-1 --voxel-cure-mm emits paired sim.json + fields.bin
  Given a CTB input with per-layer masks
  And a resin and printer profile validated against the recipe
  When the user invokes `resinsim sim --file <CTB> --resin <resin> \
    --printer <printer> --voxel-cure-mm 0.05 --out model.sim.json`
  Then a file `model.sim.json` is written
  And a file `model.fields.bin` is written alongside it
  And `model.sim.json` carries a top-level `fields_sidecar` object
  And `fields_sidecar.path` is the relative filename `model.fields.bin`
  And `fields_sidecar.sha256` is the hex-encoded SHA-256 of `model.fields.bin`
  And `fields_sidecar.byte_size` equals the file size of `model.fields.bin`
  And `fields_sidecar.fields_present` includes `"cure"` and `"photoinitiator"`
```

## UAT-2: reload restores voxel fields

```gherkin
Scenario: UAT-2 reload reattaches voxel fields to the aggregate
  Given a paired `model.sim.json` + `model.fields.bin` produced by a
    --voxel-cure-mm run
  When the user invokes `resinsim report health --in model.sim.json`
  Then the process exits with code 0
  And no warning about missing voxel fields appears in stderr
```

## UAT-3: overwrite-silently policy

```gherkin
Scenario: UAT-3 running `resinsim sim --out` twice overwrites both files
  Given a previously-produced pair `model.sim.json` + `model.fields.bin`
  When the user invokes `resinsim sim --file <CTB> --resin <resin> \
    --printer <printer> --voxel-cure-mm 0.05 --out model.sim.json`
  Then both files are overwritten silently
  And no `--force` flag is required
  And no error mentions an existing file
```

## UAT-4: Tier-1 simulation does NOT write a sidecar

```gherkin
Scenario: UAT-4 Tier-1 scalar simulation omits the sidecar
  Given a CTB input
  When the user invokes `resinsim sim --file <CTB> --resin <resin> \
    --printer <printer> --out tier1.sim.json` (without --voxel-cure-mm)
  Then `tier1.sim.json` is written
  And `tier1.fields.bin` is NOT written
  And the envelope JSON does NOT contain a `fields_sidecar` key
```
