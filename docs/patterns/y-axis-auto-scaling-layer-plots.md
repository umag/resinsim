---
issue: viz-v2-dashboard
date: 2026-05-12
---

# Pattern: Y-axis auto-scaling for layer-axis plots — `data_y_bounds` vs `percentile_bounds`

## Context

The resinsim-viz v2 dashboard paints ~10 layer-axis plots simultaneously.
Each pane wants its default zoom to fit the steady-state variation
of its series so a glance reveals trouble, but every series has a
different distribution shape:

- **Forces** has a bottom-layer spike (~3-5× steady) and otherwise
  a tight steady-state band. Min/max with padding shows the spike
  AND the steady-state because the ratio is modest.
- **Safety factor** can carry `f32::INFINITY` on zero-force layers
  (filtered to a gap by the projection) but the remaining finite
  values may still include single huge outliers up to 1e6 (a very
  small force makes the ratio huge). Min/max here crushes the
  steady-state ~10 into a flat line at the bottom of a 0 — 1e6
  axis.
- **Cure depth** has a similar bottom-layer cure spike (slow
  bottom-exposure = much deeper cure) that dwarfs the rest.
- **Vat temperature**, **Viscosity** are smooth Arrhenius curves
  with no spikes. Min/max + padding works fine.

A single helper can't serve both shapes well.

## Pattern

Two pure helpers in `src/ui/v2/zoom.rs`, each used per-pane based
on the series shape:

```rust
/// Min..max + padding. Use for well-behaved series.
pub fn data_y_bounds(values: &[f64], padding_frac: f64) -> Option<(f64, f64)>

/// Quantile-clamped lo..hi + padding. Use when outliers exist.
pub fn percentile_bounds(
    values: &[f64],
    lo_pct: f64,
    hi_pct: f64,
    padding_frac: f64,
) -> Option<(f64, f64)>
```

Both filter non-finite values defensively. The pane picks one when
setting `Plot::default_y_bounds`. The choice is documented per pane:

| Pane          | Helper             | Why                                          |
|---------------|--------------------|----------------------------------------------|
| Forces        | `data_y_bounds`    | Bottom spike + steady-state both fit in min..max |
| Safety        | `percentile_bounds` (p2..p98) | INFINITY filtered, finite outliers up to 1e6 |
| Cure depth    | `percentile_bounds` (p2..p98) | Bottom-layer spike (~3-5× steady)            |
| Vat temp      | `data_y_bounds`    | Smooth Arrhenius curve                       |
| Area + Δarea  | `percentile_bounds` (p2..p98) | Bottom-layer area-delta spike dwarfs delta band |
| Viscosity    | `data_y_bounds`    | Smooth decay                                 |
| Z deflection  | `percentile_bounds` (p0..p99) | Keep min, clip top (bottom spike)             |

`default_y_bounds` only affects the FIRST paint; the user can
pinch out (or context-menu Reset zoom) to see clipped outliers.

## Heuristic

Ask: *if I plot every value with default auto-bounds, does the
steady-state read as a flat line?* If yes → percentile. If no →
data range.

For series with one-sided spikes (positive only — bottom-layer
peel, cure depth), use `percentile_bounds(values, 0.0, 0.99, pad)`
to keep the minimum and only clip the top tail.

For series with two-sided outliers (safety factor) use
`percentile_bounds(values, 0.02, 0.98, pad)` to clip both ends.

## See also

- `crates/resinsim-viz/src/ui/v2/zoom.rs` — the helpers + unit
  tests.
- `crates/resinsim-viz/src/ui/v2/panes/*.rs` — per-pane usage.
