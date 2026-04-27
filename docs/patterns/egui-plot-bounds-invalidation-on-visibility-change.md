---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# Pattern: explicit auto-bounds invalidation on series-visibility change

## Context

egui_plot 0.34's `Plot` widget caches plot bounds in `egui::Memory`
keyed on the Plot's `Id` across frames. The default behaviour is
auto-fit on the first paint, then *frozen* until the user
right-clicks to reset OR `set_auto_bounds(...)` is called explicitly.

When the same Plot draws different series subsets across frames
(e.g. user toggles a checkbox to hide a series), the cached bounds
no longer reflect the visible data. The result: a hidden series's
range still constrains Y, leaving smaller-magnitude visible series
crushed into a flat line near the axis.

This bit issue 05 in plan v1: the original plan assumed
"auto-fit re-runs on toggle" because that's the documented default
behaviour. Adversarial review caught it as a HIGH finding — egui_plot
auto-fit applies on first paint, not on every frame's data change.

## Pattern

Track the visibility flag tuple in the panel's state and detect
change between frames; on change, force a re-fit:

```rust
let cur_vis = (state.show_a, state.show_b, state.show_c);
let force_refit = state.prev_visibility != cur_vis;
state.prev_visibility = cur_vis;

Plot::new("my-plot")
    .show(ui, |plot_ui| {
        if force_refit {
            plot_ui.set_auto_bounds([true, true]);
        }
        // ... draw enabled series ...
    });
```

`set_auto_bounds([true, true])` invalidates BOTH x and y bounds and
asks egui_plot to recompute from the items added in the closure.
Pass `[true, false]` to invalidate only x (rare — pan-x freedom is
usually wanted across frames), `[false, true]` for only y (the
typical y-toggle case).

## Where this lives in code

- `crates/resinsim-viz/src/ui/plots.rs::render_layer_timeline` —
  issue 05 first instance.
- `BottomPanelState.prev_visibility` (`crates/resinsim-viz/src/ui/state.rs`)
  carries the cross-frame state.

## When to apply

- Every time you toggle which series are drawn in a single Plot.
- Every time the data domain changes (e.g. log-scale flip — new y
  range, old bounds invalid).
- NOT every frame unconditionally — `set_auto_bounds` triggers a
  fit-pass that wipes any user pan/zoom state, so calling it
  per-frame defeats user navigation.

## Why not derive a fresh Plot ID

Tempting alternative: hash the visibility tuple into `Plot::new()`'s
ID so a toggle produces a new Plot widget. Egui then forgets the
old bounds entirely. But this loses any user-applied pan/zoom every
toggle (a fresh Plot has fresh bounds), AND egui's animation /
layout caches keyed on Plot ID get invalidated. Explicit
`set_auto_bounds` is surgical: keep the same Plot widget, just ask
it to re-fit once.

## Test seam

The change-detection comparison runs inside the egui closure (which
isn't unit-testable per `bevy-app-test-seam.md`'s egui caveat). The
manual UAT
`spec/uat/viz-timeline-series-toggle-rescales-y.md` is the
regression net for the pattern's correctness.

## See also

- ADR-0014 — locks this as the issue-05 contract.
- Pattern `bevy-app-test-seam.md` — the egui caveat that explains why
  the change-detection logic stays in the closure rather than being
  refactored "for testability."
