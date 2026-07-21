---
issue: peel-corrections-s0-harness-readiness
date: 2026-07-21
---

# Pattern: Single-source argmax for "which layer peaks"

## Context

Two call sites needed "the index of the peak-force layer": `inspect athena`
(peak of the real per-layer signal) and `inspect calibrate`'s `ComparisonReport`
(predicted vs actual peak layer — the KB-115 offset). The obvious path — an
inline `iter().max_by(...)` at each site — reintroduces the same argmax twice,
and the two copies drift: `Iterator::max_by` returns the **last** equal maximum,
an ad-hoc loop might return the first, and one keys on `LayerForce.index` while
another keys on `Vec` position. Plan review flagged the duplication as HIGH
(trace-existing-paths).

## Pattern

Put the argmax in ONE place and route every caller through it:

- `argmax_by<T>(items, key) -> Option<usize>` — the core: `f64::total_cmp`
  (deterministic, NaN-tolerant), tie-break to the **first** (earliest) item,
  `None` on empty.
- `peak_index(&[LayerForce]) -> Option<usize>` — thin wrapper keying on
  `peak_signal`.

`ForceComparator::compare` uses `argmax_by(&predicted, |&v| v)` for the
prediction and `peak_index(&actual[..n])` for the real series; `cmd_athena`
calls `peak_index` and maps the position back to `LayerForce.index` for display.
One argmax, identical semantics everywhere.

## Why

- **No drift.** Tie-break direction and comparison operator are defined once.
  When `cmd_athena` moved from `max_by` (last max) to `peak_index` (first max),
  the change was explicit and unit-pinned, not a silent divergence.
- **Testable in isolation.** The helper carries its own unit tests (position,
  tie-break, empty) that both callers inherit.
- **Comparable output.** Predicted/actual/CLI peak layers come from the same
  rule — essential when the deliverable is the predicted−actual peak *offset*
  (KB-115): an offset is only meaningful if both ends are measured identically.

## See also

- `single-source-arrhenius-helper.md` — same principle for a shared physics fn.
- ADR-0022 Stage 0; `crates/resinsim-core/src/services/force_series_extractor.rs`.
