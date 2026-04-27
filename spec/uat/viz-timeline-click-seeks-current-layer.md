---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# UAT: Click on the layer timeline chart seeks the current layer

## Rationale

Issue 05's headline interaction: clicking the bottom-panel chart at
some x-coordinate must update `CurrentLayer.index` so the slice
heatmap, layer cursor, and HUD all jump to the clicked layer. The
chart writes through the same `CurrentLayer` resource that arrow keys
already drive (issue 03), so the heatmap-follows behaviour is not
re-implemented — it falls out of sharing the resource.

The bake-once mesh contract from issue 03
(`viz-arrow-key-step-no-mesh-reupload.md`) carries forward: the click
must NOT trigger a slice-mesh re-upload. Only the `LayerCursor`
entity's `Transform.translation.z` mutates between frames.

## UAT-1: clicking at layer K writes CurrentLayer.index = K

```gherkin
Scenario: clicking the timeline at layer K updates the current layer
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        loaded for a 200-layer print, cursor at layer 0
  When the user clicks the bottom-panel chart at the x-coordinate
        nearest to layer 50
  Then CurrentLayer.index == 50
  And the HUD log emits "Layer 51/200" (1-based render of 0-based index)
  And the LayerCursor entity's Transform.translation.z equals
      z_prefix[50] + LAYER_CURSOR_EPSILON_MM
  And the cursor VLine in the chart sits at x = 50.0
  And the chart's "Layer 51" text label sits at x = 50.0,
      y ≈ peak peel_force_n across the print
```

## UAT-2: click does not re-upload the slice mesh

```gherkin
Scenario: clicking the timeline does not re-upload the slice mesh
  Given the resinsim-viz binary running with --load-ctb + --load-sim
  When the user clicks the bottom-panel chart at any in-range
        x-coordinate to seek to a different layer
  Then the slice-stack Mesh asset's ATTRIBUTE_COLOR Vec is
       byte-identical before and after
  And no entry in Assets<Mesh> is added or removed
  And the only Transform that changes between frames is the
      LayerCursor's translation.z
```

## UAT-3: clicking out-of-range x clamps to the bounds

```gherkin
Scenario: clicking past the chart's right edge clamps to the last layer
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        loaded for a 200-layer print
  When the user pans the chart so x = 1000 is visible inside the plot
        area, then clicks at x = 1000
  Then CurrentLayer.index == 199 (last layer; saturated)
  And the HUD log emits "Layer 200/200"
```
