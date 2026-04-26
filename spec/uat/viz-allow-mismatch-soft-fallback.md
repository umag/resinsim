---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: `--allow-mismatch` falls back to uncoloured rendering with a warn

## Rationale

Escape hatch documented in plan v2 step 7 specifically because hard
errors block sim development workflows. The flag's help text is
intentionally discouraging ("DANGEROUS"); a UAT pins the behaviour so
the discouraging tone does not get diluted in a future tidy.

## UAT-3: `--allow-mismatch` renders without ATTRIBUTE_COLOR and warns

```gherkin
Scenario: UAT-3 --allow-mismatch renders without ATTRIBUTE_COLOR and warns
  Given the resinsim-viz binary
  When the user invokes it with --load-ctb <100-layer.ctb> --load-sim <50-layer.sim.json> --allow-mismatch
  Then stderr contains "layer count mismatch" and "--allow-mismatch is set, rendering uncoloured"
  And the slice-stack mesh has no Mesh::ATTRIBUTE_COLOR attribute
  And no LayerCursor entity is spawned
  And the process keeps running until the user closes the window
```
