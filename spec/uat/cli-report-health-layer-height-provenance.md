---
issue: ctb-layer-height-authority
date: 2026-05-19
status: proposed
---

# UAT: `report health` surfaces layer_height_provenance from sim.json

## Rationale

`ctb-layer-height-authority` introduced a custom serde shape for
`LayerHeightProvenance` in sim.json:

- **Uniform CTB**: `{ ctb_um: f32, layer_count: u32, recipe_um, ... }`
- **Variable / adaptive CTB**: `{ ctb_layer_heights_um: Vec<f32>,
  recipe_um, ... }`

Both shapes round-trip cleanly through the value object's
`Deserialize` (verified by unit tests in
`crates/resinsim-core/src/values/layer_height_provenance.rs`). What's
NOT covered at the CLI surface: `resinsim report health --in <sim.json>`
loading either shape end-to-end. A future producer/consumer drift —
e.g. a viz or LLM tool emitting a sim.json that uses the legacy
single-`ctb_um`-only shape — would silently break if the
deserialiser ever regressed.

These scenarios document the contract for follow-up implementation;
step impls intentionally not shipped here so the cucumber harness's
coverage guard isn't tripped further. Land step impls when bandwidth
allows.

## UAT-1: Uniform shape (common case)

**Acceptance:** report_health loads a sim.json with `ctb_um +
layer_count` provenance and renders the summary line correctly.

```
Scenario (proposed): report health loads sim.json with uniform provenance shape
  Given a sim.json file whose simulation.layer_height_provenance carries
    ctb_um=40.0, layer_count=4492, recipe_um=40.0
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with code 0
  And stdout reports the CTB layer_height as "40.000 µm (recipe: 40.000 µm)"
  And stdout does NOT contain the " ⚠" suffix
```

## UAT-2: Variable shape with mismatch

**Acceptance:** adaptive sim.json renders the variability summary and
the warning suffix.

```
Scenario (proposed): report health loads sim.json with variable provenance shape
  Given a sim.json file whose simulation.layer_height_provenance carries
    ctb_layer_heights_um=[30.0, 40.0, 50.0, 40.0, 30.0], recipe_um=30.0,
    mismatch with kind=variable
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with code 0
  And stdout reports the CTB layer_height as a variable range
  And stdout contains "30.000–50.000 µm"
  And stdout contains "mean 38.000 µm"
  And stdout contains the " ⚠" suffix
  And stdout contains "adaptive slicing"
```

## UAT-3: Legacy single-`ctb_um` shape still loads

**Acceptance:** hand-written or older sim.json forms that omit
`layer_count` reconstruct as a single-layer series.

```
Scenario (proposed): report health loads legacy sim.json with ctb_um only
  Given a sim.json file whose simulation.layer_height_provenance carries
    ctb_um=40.0, recipe_um=40.0 (no layer_count, no Vec)
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with code 0
  And stdout reports the CTB layer_height as "40.000 µm (recipe: 40.000 µm)"
```

## Implementation notes (for follow-up)

- Step impls live in
  `crates/resinsim-core/tests/uat_steps/cli_report_health_layer_height_provenance.rs`
- Fixture sim.json files can be assembled inline using `serde_json::json!`
  + `crates/resinsim-core/src/repositories::save_with_provenance` (write
  to a TempDir), then invoke the CLI via `cli_fixtures::invoke_resinsim`.
- Skipping for now (no step impls) keeps the harness's pre-existing
  "skipped scenarios" baseline unchanged.
