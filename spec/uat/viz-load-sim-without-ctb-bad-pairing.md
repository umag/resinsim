---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: sim without CTB pairing exits non-zero (`EXIT_BAD_SIM_PAIRING = 4`) under `--smoke-exit`

## Rationale

UX-2 (HIGH, plan v1): the original "warn + skip" was silent in CI runs.
Plan v2 elevated to a hard error with a distinct exit code so CI
scripts can branch on the failure class.

## UAT-4: `--load-sim` without `--load-ctb` is a bad pairing

```gherkin
Scenario: UAT-4 --load-sim without --load-ctb is a bad pairing
  Given the resinsim-viz binary
  When the user invokes it with --load-sim <any.sim.json> --smoke-exit
  Then stderr contains "--load-sim was supplied without --load-ctb"
  And stderr mentions that the heatmap requires slice-stack geometry
  And the process exits with code 4
```
