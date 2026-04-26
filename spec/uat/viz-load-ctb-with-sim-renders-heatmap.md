---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: `resinsim-viz --load-ctb + --load-sim` renders coloured slice-stack with cursor

## Rationale

Primary user-facing surface introduced by issue 03. Plan v2's "## UAT
coverage" section listed this scenario as needed; no existing UAT
covered it. The env-var-gated
`smoke_exit_with_load_sim_pairing_runs_heatmap_path` test verifies
entity spawn but not the user-observable Bevy window outcome.

## UAT-1: viz loads CTB + matching sim and renders a coloured slice-stack

```gherkin
Scenario: UAT-1 viz loads CTB + sim and renders a coloured slice-stack
  Given the resinsim-viz binary
  When the user invokes it with --load-ctb <ctb> --load-sim <matching.sim.json>
  Then a Bevy window opens titled "resinsim-viz"
  And stderr contains "Controls: ↑/↓ arrows step layers"
  And stderr contains a "Layer N/N | cure_depth X.X µm | ramp X.X–X.X µm" line
  And the slice-stack is rendered with per-layer vertex colours from a viridis ramp
  And a translucent layer-cursor entity is visible at the topmost layer's Z
```
