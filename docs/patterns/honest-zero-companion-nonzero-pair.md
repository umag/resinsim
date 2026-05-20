---
issue: t2f3.1-post-impl-calibration-followups
date: 2026-05-20
---

# Pattern: Honest-zero claim paired with upstream non-zero companion

## When to apply

When a test asserts "value X reads exactly zero on the calibrated
path because the model honestly produces zero" (the parent
`honest-zero-with-model-gap-caveat` pattern), the assertion is BLIND
to one specific regression class: a magnitude COLLAPSE that zeros the
entire upstream pipeline. A unit error MPa → Pa (×10⁻⁶), a missing
multiply, a scalar default initialisation to 0.0 — all produce the
same honest-zero output and pass the assertion silently.

The companion-test pattern pairs the honest-zero assertion at layer
N with a non-zero assertion at layer ≤ N-1 (closer to the input).
The non-zero companion catches the collapse direction the honest-
zero claim is blind to.

## Concrete example (t2f3.1 B3 + companion)

**Honest-zero claim** (`voxel_yield_fraction == Some(0.0)` on every
layer of a calibrated-resin solid sim) asserts:

- Cache IS populated (`Some`, not `None`)
- Value IS zero (free-shrinkage stress doesn't yield)

**What this catches:** σ_vm magnitude blow-up ≥~6× from unit-
conversion errors, double-counted strain contributions. 100× blow-
ups (Pa↔MPa wrong direction) trip immediately.

**What this MISSES:** magnitude collapse (everything zeros to 0.0),
sign flips (von Mises uses absolute squares, sign cancels), sub-6×
drift.

**Companion claim** (at-least-one layer has `strain_magnitude_max >
0.0`) asserts at the strain-field cache layer, one model step
upstream of the yield-fraction:

- Strain field cache IS populated
- At least one voxel has non-zero strain

**What this catches:** full-collapse-to-zero regressions (the
direction the honest-zero is blind to).

Together the pair locks both magnitude-blow-up AND magnitude-
collapse classes for the calibrated free-shrinkage path.

## How to apply

1. Identify the honest-zero claim's data path: which layer of the
   model pipeline produces the value being asserted to zero?
2. Walk one layer upstream — what's the closest signal that MUST be
   non-zero for the honest-zero to be meaningful? (For a yield-
   fraction zero, the strain magnitude or σ_vm magnitude one layer
   up is the natural pair.)
3. Write the companion test against that upstream signal. Prefer
   `any(...) > 0.0` (asserts existence, not uniformity) so the
   companion doesn't fail on incidentally-quiet layers.
4. Document the explicit "covers / does not cover" split in the
   honest-zero test's docstring so future readers don't conflate
   the two tests and don't delete one as redundant.

## Detection in review

A test asserting `value == 0` on the calibrated path should trigger
a review prompt: "is there an upstream non-zero companion?" If not,
flag MEDIUM — the test is one-sided.

## Related

- `docs/patterns/honest-zero-with-model-gap-caveat.md` — parent
  pattern (HOW to ship physically-correct criteria that produce
  honest zeros).
- `docs/patterns/anti/magic-floor-vs-honest-filter.md` — the anti-
  pattern the parent pattern breaks.
