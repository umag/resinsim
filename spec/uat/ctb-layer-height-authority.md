---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# UAT: CTB layer_height is the simulation authority

## Rationale

Per ADR-0005 ("three-axis: printer rig / resin chemistry / recipe") the
CTB file is the operating point — the file the user submitted to print —
while the resin recipe is authoring metadata describing the user's
calibration intent. When the two disagree on layer height, the simulator
must (a) honour the CTB's per-layer values at runtime, (b) emit a loud
warning to stderr in Mag's literal "GUESS / WRONG LAYER COUNT" framing,
and (c) surface the reconciliation in the produced `sim.json` so
downstream consumers (`report health`, viz) can show it. Variable-Z
CTBs (adaptive / variable layer height slicing, e.g. Chitubox v2 +
Lychee Pro) are first-class: the simulator dispatches each layer's slab
thickness individually and warns with a min/max/mean summary because no
single recipe value can describe a varying stack.

## UAT-1: Mismatch path emits the stderr warning and exits 0

**Rationale.** The simulation produces a valid result for the file the
user has (the CTB's layer height wins); the recipe disagreement is a
soft warning, not an error. Exit code 0 lets batch pipelines distinguish
"this finished successfully with a noted advisory" from "this failed".

```gherkin
Scenario: UAT-1 CTB / recipe layer_height mismatch warns to stderr but succeeds
  Given a CTB input sliced at 40 µm
  And a resin profile whose recipe.layer_height_um is 30 µm
  And a printer profile whose layer_height_range_um contains both 30 and 40 µm
  When the user invokes `resinsim sim --file <CTB> --resin <RESIN> --printer <PRINTER> --out <OUT>`
  Then the process exits with code 0
  And stderr contains "WARNING: CTB layer_height (40"
  And stderr contains "does NOT match recipe layer_height"
  And stderr contains "GUESS"
  And stderr contains "WRONG LAYER COUNT"
  And the produced sim.json's `simulation.layer_height_provenance.ctb_um` equals 40
  And the produced sim.json's `simulation.layer_height_provenance.recipe_um` equals 30
  And the produced sim.json's `simulation.layer_height_provenance.mismatch` is present
```

## UAT-2: Agreement path emits no warning and exits 0

**Rationale.** No warning noise when the CTB and recipe agree — the
expected common case. The provenance is still surfaced (so consumers
can show "CTB and recipe agree: X µm") but its `mismatch` field is
absent.

```gherkin
Scenario: UAT-2 CTB / recipe layer_height agreement emits no warning
  Given a CTB input sliced at 50 µm
  And a resin profile whose recipe.layer_height_um is 50 µm
  And a printer profile whose layer_height_range_um contains 50 µm
  When the user invokes `resinsim sim --file <CTB> --resin <RESIN> --printer <PRINTER> --out <OUT>`
  Then the process exits with code 0
  And stderr does NOT contain "WARNING: CTB layer_height"
  And the produced sim.json's `simulation.layer_height_provenance.ctb_um` equals 50
  And the produced sim.json's `simulation.layer_height_provenance.mismatch` is absent
```

## UAT-3: Adaptive slicing (variable layer height) is supported with a warning

**Rationale.** CTB files produced by adaptive / variable-layer-height
slicers (Chitubox v2, Lychee Pro, PrusaSlicer MSLA) carry genuinely
different `layer_height_um` per layer. The simulator dispatches each
layer's slab thickness individually — adaptive CTBs run end-to-end —
but the resin recipe's authored value cannot describe a varying stack,
so the warning fires with min/max/mean context. The provenance's
`mismatch.kind` discriminator is `"variable"` (not `"uniform"`) so
downstream consumers can render an adaptive-slicing badge rather than
the recipe-disagrees badge.

```gherkin
Scenario: UAT-3 adaptive-slicing CTB runs with a variable-Z warning
  Given a CTB input with per-layer layer_height_um values 30 40 50 40 30 µm
  And a resin profile whose recipe.layer_height_um is 30 µm
  And a printer profile whose layer_height_range_um contains both 30 and 40 µm
  When the user invokes `resinsim sim --file <CTB> --resin <RESIN> --printer <PRINTER> --out <OUT>`
  Then the process exits with code 0
  And stderr contains "variable layer height"
  And stderr contains "GUESS"
  And stderr contains "WRONG LAYER COUNT"
  And the produced sim.json's `simulation.layer_height_provenance.mismatch` is present
  And the produced sim.json's `simulation.layer_height_provenance.mismatch.kind` equals "variable"
```

**Why recipe=30 (not 40):** with 5 layers totalling 190 µm and recipe=30,
the recipe-implied count is `round(190/30) = 6`, which differs from
the CTB's actual 5 layers — so the warning surfaces both Mag's
"GUESS" and "WRONG LAYER COUNT" keywords. (If the recipe-implied
count happens to equal the CTB's count — e.g. recipe=40 here yields
`round(190/40) = round(4.75) = 5` — the warning's collision-aware
branch substitutes the "happens to imply N layers — same count, but
the per-layer thicknesses themselves differ" phrase. See unit test
`format_warning_variable_branch_collision_aware` in
`crates/resinsim-core/src/values/layer_height_provenance.rs` for
that branch's coverage.)
