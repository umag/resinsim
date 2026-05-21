---
issue: t2f4-thermal-diffusion
date: 2026-05-21
---

# UAT: ThermalField round-trips through the RSFIELD sidecar

## Rationale

ADR-0020 / t2f4 adds a fifth voxel-field kind (`FieldKind::Thermal`)
to the RSFIELD binary sidecar (ADR-0019). Unlike the four part-bbox-
anchored fields (`cure`, `photoinitiator`, `strain`, `stress`),
ThermalField uses the vat envelope as its coordinate anchor — so the
load-bearing roundtrip invariants are:

1. The ENCODED sidecar carries `fields_present: ["..., "thermal"]`
   alongside the existing kinds.
2. After load, `sim.thermal_field()` is `Some` with the same dims,
   voxel_size, bbox_min, and byte-for-byte voxel data as the
   pre-save aggregate (post-zstd-decompression — zstd is lossless).
3. The sidecar's cross-field-dimension lock EXCLUDES the thermal
   descriptor — the four part-bbox kinds still cross-check, but
   thermal's vat-envelope dims are allowed to diverge.

UAT-1 below pins the pipeline-level contract. The unit-level
byte-identity invariants are enforced in
`crates/resinsim-core/tests/sidecar_roundtrip_integration.rs` (a
new `thermal_only_roundtrip` test + an extension of the existing
`all_kinds_present_roundtrip` test).

See also:

- `docs/adr/0019-voxel-field-on-disk-persistence.md` — the load-
  bearing sidecar ADR.
- `docs/adr/0020-spatial-thermal-diffusion.md` §Decision x — bumped
  `RSFIELD_FORMAT_VERSION 1→2` for the new kind.
- `docs/patterns/voxel-field-sidecar-binary-format.md` — RSFIELD
  format spec, kind_tag table row `4 = Thermal`.
- `sim-fields-sidecar-roundtrip.md` — sibling pattern for the four
  part-bbox kinds.

## UAT-1: `--voxel-cure-mm` emits a thermal sidecar payload

```gherkin
Scenario: UAT-1 voxel-mode CTB run produces a sidecar carrying thermal
  Given a CTB input with per-layer masks
  And a resin and printer profile validated against the recipe (under field-sim)
  When the user invokes `resinsim sim --file <CTB> --resin <resin> \
    --printer <printer> --voxel-cure-mm 0.05 --out model.sim.json`
  Then a file `model.fields.bin` is written alongside `model.sim.json`
  And `model.sim.json` carries `fields_sidecar.fields_present` including `"thermal"`
  And the sidecar's RSFIELD header carries `format_version = 2`
  And the sidecar's descriptor stream carries a kind_tag=4 entry whose
      `dim_x × dim_y × dim_z × voxel_size_mm` matches the printer's
      `build_envelope_mm` (NOT the part bbox the other four kinds use)
```

## UAT-2: reload reattaches the thermal field with byte-identical values

```gherkin
Scenario: UAT-2 load_envelope round-trips ThermalField losslessly
  Given a previously-saved `<stem>.sim.json` + `<stem>.fields.bin` pair
        from a voxel-mode run with a populated thermal_field
  When the user invokes `resinsim report health --in <stem>.sim.json`
  Then the loaded `PrintSimulation` has `sim.thermal_field().is_some()`
  And the loaded thermal_field's dimensions, voxel_size_mm, and
      bbox_min_mm match the pre-save values byte-for-byte
  And every voxel's f32 temperature matches the pre-save value
      byte-for-byte (zstd is lossless; the encoder pins the level
      explicitly for cross-run determinism)
```

## UAT-3: legacy `format_version = 1` sidecars are rejected

```gherkin
Scenario: UAT-3 v1 sidecar produces a typed format-version error
  Given a `model.sim.json` whose `model.fields.bin` carries the legacy
        RSFIELD `format_version = 1` header
  When the user invokes `resinsim report health --in model.sim.json`
  Then the load fails with a non-zero exit code
  And stderr names `"unknown sidecar format_version"`
  And stderr surfaces the actual `got=1, expected=2` numbers
  And no partial PrintSimulation is constructed
```
