---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# ADR-0016: layer timeline chart and bottom panel

## Status
Accepted

## Context

Phase 2 step 4 of the simulation plan
(`projects/000-global/research/resinsim-physics-simulation-plan.md`)
calls for a "layer timeline chart" alongside the time-axis stacks
that issue 04 already ships in the right inspector. The two charts
serve different mental models:

- **Time-axis stack** (issue 04) — peel / suction / total / vat-temp /
  viscosity vs cumulative print time (`H:MM:SS`). The "what's
  happening when" view.
- **Layer-axis line chart** (issue 05) — peel / cure-depth / safety
  vs layer index `0..N`. The "which layer is the weak link" view,
  plus the load-bearing seek interaction: clicking the chart drives
  `CurrentLayer.index`, which the issue-03 heatmap and arrow-key HUD
  already consume.

ADR-0011 locked the left and right egui side panels. Issue 05 needs
a third anchor for the new chart that doesn't fight the existing
viewport budget. egui_plot 0.34 is the pinned plotting library;
its API surface dictates how click-to-seek and bound-invalidation
work.

## Decision

### 1. Bottom-panel anchor — third locked egui slot

`egui::TopBottomPanel::bottom("layer-timeline")` mounts after the
left + right SidePanels claim their full-vertical strips, taking the
bottom band of the remaining centre. Sized
`default_height(180.0).min_height(120.0).resizable(true)` —
PrusaSlicer-style timelines are typically 120–160 px so 180 is
spacious without crushing the 3D viewport, and 120 is the floor
beyond which axis labels collide.

The three panel systems plus `debug_camera_overlay` are registered
on `EguiPrimaryContextPass` with explicit
`.chain()`:

```rust
.add_systems(
    bevy_egui::EguiPrimaryContextPass,
    (left_panel, right_panel, bottom_panel, debug_camera_overlay).chain(),
);
```

The chain documents the layout-order dependency. Bevy's exclusive
`EguiContext` borrow already serialises these systems, but `.chain()`
removes the implicit assumption — a future refactor cannot
accidentally reorder them and discover the regression at paint time.

### 2. Click-to-seek via PlotUi — pure helper for the math

egui_plot 0.34's interaction surface for plot clicks:

```rust
plot_ui.response().clicked()        // bool — did the user click?
plot_ui.pointer_coordinate()        // Option<PlotPoint> — in plot space
```

Both are inside the `Plot::show` closure. The closure returns its
inner `R` through `PlotResponse::inner`, which is how the snapped
layer index travels back out:

```rust
.show(ui, |plot_ui| {
    if plot_ui.response().clicked() {
        if let Some(p) = plot_ui.pointer_coordinate() {
            return snap_plot_x_to_layer(p.x, layer_count);
        }
    }
    None
})
.inner   // Option<u32> — the snapped layer
```

`snap_plot_x_to_layer(x, count)` rounds-to-nearest with bounds clamp.
It's a **pure pre-helper**, deliberately extracted from the egui
closure so the click-to-seek math can be unit-tested without an
EguiPlugin (per
`docs/patterns/bevy-app-test-seam.md`'s egui caveat — egui closures
are not unit-testable; their pure callees are). The same pattern was
used by issue 04 for `build_plot_data` and `run_block_reason`. No
sibling pattern doc — the existing one covers it.

Round-to-nearest semantics for click are **intentionally different**
from arrow-key step (saturating ±1, see
`viz-arrow-keys-step-layer-with-saturation.md`). Clicks are
continuous gestures pointing at an x-value; keypresses are discrete
deltas relative to the current index. Different mental models,
different rules.

### 3. Two projections coexist by design

`build_plot_data` (issue 04, time-axis) and `build_layer_chart_data`
(issue 05, layer-axis) are **not** unified into one projection.
Reasons:

- Different x-axes (cumulative seconds vs layer index 0..N) — egui_plot
  can't share an axis link group across them.
- Different series subsets — the time-axis plots split into separate
  charts per Y unit (forces, vat-temp, viscosity); the layer-axis
  chart deliberately puts mixed-unit series on a single axis with
  toggleable visibility because the use case is "find the worst layer
  by ANY metric", not "compare absolute values across metrics".
- Different filter rules — the layer-axis projection drops non-finite
  samples at projection time (∞ safety on zero-force layers; future
  log-scale extends the rule); the time-axis projection assumes
  finite throughout because the time-axis quantities are bounded by
  construction.

Future cleanup may rename for symmetry (`build_time_series` /
`build_layer_series`) but it would touch every issue-04 test, so
out of scope here.

### 4. Log-scale on safety_factor — data transform, not native axis

egui_plot 0.34 has no native log axis. The safety_factor log-scale
toggle is implemented as a **projection-time transform**:

- `build_layer_chart_data(sim, log_safety: true)` outputs
  `(layer_index, log10(sf))` for layers whose `safety_factor` is
  **finite-and-positive**; non-positive and non-finite layers are
  dropped (log10 is undefined).
- The y-axis label changes via the series name
  (`"Safety factor (log10)"` vs `"Safety factor (×)"`) — egui_plot's
  Legend surfaces it, no separate axis-label flip needed.

We deliberately avoid a magic-number floor (e.g. `clamp(SF, 0.001, ∞)`
→ `log10 = -3`) because it would silently convert "no defined SF" into
"SF is roughly 0.001", which lies. Filter is honest; clamp is not.

The toggle is **gated on `show_safety`**: it's only visible while the
parent series is enabled, and is reset to `false` when the parent is
toggled off. Sub-option semantics — re-enabling the safety series
later starts in linear mode rather than silently remembering log.

When egui_plot ships native log axes (post-0.34), the projection-side
transform can collapse into a plot configuration; the `safety_log_scale`
toggle remains, just routes differently.

## Consequences

- The issue-04 right inspector and the issue-05 bottom panel coexist
  permanently — both are intended view contracts.
- Future Phase-2-step-5+ panel features (issue 06 material editor,
  issue 07 view modes, issue 08 Athena overlay) get the bottom band
  if they're layer-axis or time-axis charts, OR the right inspector
  if they're textual/control inspectors. The anchor convention is
  three locked slots; further panels go inside them, not as a fourth.
- The `.chain()` ordering is load-bearing — not for parallelism
  correctness (exclusive `EguiContext` borrow already serialises),
  but for layout determinism. Document in `add_systems` comment.

## Cross-references

- ADR-0010 — `resinsim-viz` presentation layer separation. `build_layer_chart_data` lives in viz; `resinsim-core` MUST NOT depend on it.
- ADR-0011 — left/right anchor lock + bevy_egui 0.39 / egui_plot 0.34 version chain. ADR-0016 extends with the third (bottom) anchor.
- `docs/patterns/bevy-app-test-seam.md` — egui caveat that mandates
  pure pre-helpers; this ADR's `snap_plot_x_to_layer` and
  `build_layer_chart_data` are direct applications.
- `viz-arrow-keys-step-layer-with-saturation.md` — the
  arrow-key seek that click-to-seek shares a target resource
  (`CurrentLayer`) with.
- `safety-factor-zero-force.md` — pins `safety_factor = ∞` on
  zero-force layers, which is what the layer-axis projection's
  finite-filter respects.
