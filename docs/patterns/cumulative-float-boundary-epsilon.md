---
issue: t2f6-field-inspector
date: 2026-08-01
---

# Pattern: Domain-scaled epsilon on cumulative float boundary checks

## Context

Resolving `z=0.14mm` against cumulative f32 layer heights
`[50, 30, 20, 40]` µm spuriously rejected as out-of-range: the µm→mm
summation accumulates to `0.13999999…`, not `0.14`, so an exact
top-boundary query at the nominal value fell outside the computed bounds.
Found by the step-4 unit tests during t2f6 (red at GREEN stage — a real
bug, not test flake).

## Pattern

Bounds checks over ACCUMULATED float sums need a tolerance scaled to the
domain's physical resolution, not machine epsilon:

- `Z_BOUNDARY_EPSILON_MM = 1e-4` (0.1 µm) — two orders of magnitude below
  the smallest real layer height (20 µm), so it can never admit a
  neighbouring layer, but absorbs any realistic accumulation error.
- Name the constant, document the derivation next to it, and record it in
  the owning ADR (ADR-0023 here) so a future reader doesn't "clean up" the
  tolerance as sloppiness.
- Exact comparisons stay correct for NON-accumulated values (single parsed
  floats round-trip exactly; see the athena fixture conventions).

## When to use

- Any boundary predicate over a running sum of per-item floats
  (cumulative heights, elapsed times, integrated doses)
- Never as a blanket float-comparison policy — unaccumulated exact values
  should stay exact, or real regressions hide inside the tolerance

## See also

- `docs/adr/0023-field-inspector-read-side-contract.md` — records the constant
- `docs/patterns/nan-two-layer-defence.md` — the finite-ness counterpart
