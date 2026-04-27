---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# UAT: Toggling a series in the layer timeline rescales Y to fit

## Rationale

Three series — Peel force (~1–15 N), Cure depth (~100–200 µm), Safety
factor (~1.5–10×) — share one Y axis on the bottom-panel chart. Mixing
them on a single axis crushes the smaller-magnitude series unless the
chart re-fits Y when visibility changes.

egui_plot 0.34 caches plot bounds across frames keyed on Plot ID;
toggling a series off does NOT re-fit on its own. `render_layer_timeline`
detects visibility changes via `BottomPanelState.prev_visibility` and
calls `set_auto_bounds([true, true])` on the same frame. ADR-0016
documents the contract.

## UAT-1: enabling cure expands Y upward, disabling shrinks it back

```gherkin
Scenario: toggling cure depth on/off rescales Y between peel-only and peel+cure ranges
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        for a typical 200-layer print
  And only "Peel force (N)" is checked (default state per issue body)
  When the user observes the chart's Y range
  Then the Y range bounds approximate (0, peel_max × 1.1) — peel only
       (typical: 0 to ~15)

  When the user clicks the "Cure depth (µm)" checkbox to enable it
  Then the chart re-fits Y on the same frame
  And the new Y range bounds approximate (0, max(peel, cure) × 1.1)
       (typical: 0 to ~200; cure dominates)

  When the user clicks "Cure depth (µm)" again to disable it
  Then the chart re-fits Y on the same frame
  And the Y range returns to peel-only bounds
       (typical: 0 to ~15)
```

## UAT-2: defaults match issue body — peel only at first paint

```gherkin
Scenario: first-paint visibility is peel-only per issue body
  Given a fresh resinsim-viz session with --load-ctb + --load-sim
  When the bottom panel renders for the first time after Run
  Then "Peel force (N)" checkbox is checked
  And "Cure depth (µm)" checkbox is unchecked
  And "Safety factor" checkbox is unchecked
  And the "log10" sub-checkbox for safety is not visible (parent off)
```
