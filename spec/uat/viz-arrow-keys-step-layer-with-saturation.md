---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
---

# UAT: Up/Down arrows step the cursor with saturation at 0 and max

## Rationale

Primary keyboard interaction. Plan v2 specified PrusaSlicer convention
(Up = next/higher Z, Down = previous/lower Z) with saturating
arithmetic. Direction inversion would be a silent UX regression;
saturation drift would crash on out-of-bounds.

## UAT-5: ArrowUp / ArrowDown step the layer cursor and clamp

```gherkin
Scenario: UAT-5 ArrowUp / ArrowDown step the layer cursor and clamp
  Given the resinsim-viz binary running with --load-ctb + matching --load-sim, cursor at the topmost layer
  When the user presses ArrowUp once
  Then the HUD line still reports "Layer N/N" (saturated at max)
  When the user presses ArrowDown three times
  Then the HUD line reports "Layer (N-3)/N" with the corresponding cure_depth
  When the user presses ArrowDown to step past 0
  Then the HUD line reports "Layer 1/N" (saturated at 0)
```
