---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# UAT: Drag-to-pan in the layer timeline does NOT seek the current layer

## Rationale

Issue 05's click-to-seek is gated on
`plot_ui.response().clicked()` (egui_plot 0.34), which fires only on
a non-drag click — a press-and-release without movement. A
drag-to-pan gesture (which the user uses to navigate the chart x
range) returns `false` from `clicked()` and so does NOT invoke
`snap_plot_x_to_layer`.

This is **load-bearing for chart navigability**: without the
non-drag-click semantics, every pan gesture would also fire a seek,
making the chart impossible to navigate. The behaviour is implicit
in egui_plot's `Response::clicked()` contract, but a future
egui_plot upgrade with different click semantics could silently
break pan. This UAT pins the invariant at the Resinsim level so the
regression surfaces immediately.

## UAT-1: drag-to-pan does not seek

```gherkin
Scenario: drag-to-pan does not seek the current layer
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        for a 200-layer print, cursor at layer 100
  When the user presses the left mouse button at the chart's centre,
        drags 200 px right, then releases
  Then CurrentLayer.index == 100 (unchanged — drag is pan, not seek)
  And the chart's x-range has shifted to show later layers
  And the heatmap layer cursor entity has not moved
```

## UAT-2: single click without drag DOES seek

```gherkin
Scenario: single click (no drag) seeks the current layer
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        for a 200-layer print, cursor at layer 100
  When the user clicks (press + release without movement) at the
        chart x-coordinate nearest layer 50
  Then CurrentLayer.index == 50
  And the heatmap layer cursor entity translates to z_prefix[50]
```
