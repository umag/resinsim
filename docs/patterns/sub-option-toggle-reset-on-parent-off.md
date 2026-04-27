---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# Pattern: sub-option toggle resets when its parent toggles off

## Context

A UI toggle that's only meaningful when another toggle is on
(e.g. "log10 scale" only meaningful when "Safety factor" series is
displayed) creates a state-persistence question:

- **Persist:** keep the sub-option's value across parent off-and-on
  cycles. User intent preserved.
- **Reset:** clear the sub-option to its default when the parent
  toggles off. User won't be surprised by a forgotten setting.

Both are defensible. The choice depends on how surprising the
sub-option's effect is when the user re-enables the parent.

## Pattern

**Reset on parent off, in the same render pass that detects the
parent transition.** The forgotten-toggle surprise is a recurring
ergonomic failure mode; the cost of "user has to re-toggle the
sub-option after re-enabling the parent" is far smaller than the
cost of "user re-enables the parent and the chart looks weird for
30 seconds before they realise log mode was still on."

```rust
// Inside the egui closure:
ui.checkbox(&mut state.show_safety, "Safety factor");
if state.show_safety {
    ui.checkbox(&mut state.safety_log_scale, "log10");
} else if state.safety_log_scale {
    // Parent just turned off — reset the sub-option silently.
    state.safety_log_scale = false;
}
```

The order matters: render the sub-option's checkbox AFTER the parent
checkbox, and gate the sub-option's checkbox-row on the parent's
post-frame value.

## When to apply

- The sub-option is a NICHE specialisation (log scale, advanced
  filter, alternate render mode). Persist makes sense for
  general-use options.
- The sub-option's behaviour is visually surprising when applied
  out-of-context. Persist makes sense for invisible-state options
  (e.g. units preference).
- The cost of re-discovering the option is low (one extra click).

## When NOT to apply

- The sub-option is a frequently-used preference that the user is
  expected to set once (e.g. "show grid" — visible and benign).
- The sub-option is bound to data the user has invested time in
  (e.g. "current selection" — losing it on a parent toggle would
  destroy work).

## Where this lives in code

- `crates/resinsim-viz/src/ui/plots.rs::render_layer_timeline` —
  issue 05 implementation (`safety_log_scale` reset on
  `show_safety` off).
- ADR-0014's log-scale-via-transform decision section documents the
  behaviour.

## See also

- Pattern `idempotent-cache-on-selection-change.md` — adjacent
  pattern about derived-state keyed on identity, not freshness.
  This pattern is about *clearing* derived state when its driver
  changes; that pattern is about *not redundantly recomputing*
  derived state when its driver hasn't changed.
