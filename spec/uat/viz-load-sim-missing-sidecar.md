---
issue: t2f3.5-voxel-field-persistence
date: 2026-05-20
---

# UAT: `resinsim-viz --load-sim` produces actionable error when sidecar is missing

## Rationale

ADR-0019 makes Tier-2 simulation output a two-file pair (`.sim.json`
+ `.fields.bin`). Users moving files via `cp` / `mv` / drag-drop will
sometimes pick up only the `.sim.json` and leave the sidecar behind.
The load path's error message must name BOTH files so the user knows
what to recover.

This UAT pins the missing-sidecar error path through the GUI surface
(resinsim-viz). The substring is shared with the CLI surface
(`resinsim report health --in`) so downstream tooling can grep
uniformly.

See also:
- `sim-fields-sidecar-roundtrip.md` — happy path
- `cli-sim-rejects-tampered-sidecar.md` — sha256 / truncation /
  path-traversal scenarios
- ADR-0019 §"User-facing output shape"

## UAT-1: `--load-sim <PATH>` without sidecar produces typed error

```gherkin
Scenario: UAT-1 --load-sim with missing fields.bin reports missing sidecar
  Given a paired `model.sim.json` + `model.fields.bin` was produced by
    a previous `resinsim sim --voxel-cure-mm` run
  And the user copies ONLY `model.sim.json` into a new directory
    `/tmp/move-test/` (leaving the sidecar behind)
  When the user invokes `resinsim-viz --load-sim /tmp/move-test/model.sim.json`
  Then `resinsim-viz` exits with non-zero code
  And stderr mentions "missing sidecar"
  And stderr names the expected sidecar location next to the sim.json
  And the process does not panic
```

## UAT-2: drag-drop without sidecar produces typed error

```gherkin
Scenario: UAT-2 dragging .sim.json without .fields.bin into resinsim-viz
  Given resinsim-viz is running with no sim loaded
  When the user drags `model.sim.json` from a directory that does NOT
    contain `model.fields.bin` into the viewer window
  Then the in-app error toast / status mentions "missing sidecar"
  And resinsim-viz remains running (drop failure is not fatal)
```

## UAT-3: Tier-1 sim.json (no voxel data) loads fine without sidecar

```gherkin
Scenario: UAT-3 Tier-1 envelope without fields_sidecar pointer loads cleanly
  Given a `tier1.sim.json` envelope WITHOUT a `fields_sidecar` pointer
    (i.e. produced by `resinsim sim` without `--voxel-cure-mm`)
  When the user invokes `resinsim-viz --load-sim tier1.sim.json`
  Then the load succeeds
  And no error about a missing sidecar appears
```
