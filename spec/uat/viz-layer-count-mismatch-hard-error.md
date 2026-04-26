---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: layer-count mismatch hard-errors with exit code 3 under `--smoke-exit`

## Rationale

ADV-3 (HIGH, plan v1): plan originally said "warn + fall back to
uncoloured" — caught at adversarial review and tightened to a hard
error in plan v2. The exit code (3 = `EXIT_LAYER_COUNT_MISMATCH`) is
the contract CI scripts depend on. Worth pinning so a future
"refactor" cannot silently soft-fail again.

## UAT-2: sim with wrong layer count exits non-zero under `--smoke-exit`

```gherkin
Scenario: UAT-2 sim with wrong layer count exits non-zero under --smoke-exit
  Given the resinsim-viz binary
  When the user invokes it with --load-ctb <100-layer.ctb> --load-sim <50-layer.sim.json> --smoke-exit
  Then stderr contains "layer count mismatch: CTB has 100 layers, sim has 50"
  And stderr mentions "--allow-mismatch"
  And the process exits with code 3
  And no Bevy window remains open
```
