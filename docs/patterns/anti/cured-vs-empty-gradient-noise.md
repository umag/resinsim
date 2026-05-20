---
issue: t2f3-shrinkage-strain-stress
date: 2026-05-20
---

# Anti-pattern: cured-vs-empty voxel pairs trip "interior gradient" detectors

## The mistake

Compute the maximum gradient `|∇field|` across all adjacent voxel
pairs in a Z-slab. Use this as a "high gradient = micro-crack risk"
signal. Apply a small threshold.

Every part-surface voxel has a neighbour outside the part (uncured
liquid, encoded as zero). The Frobenius diff between a cured-state
voxel and a zero-state voxel IS the magnitude of the cured-state
voxel. For L = 0.9% shrinkage (Elegoo Ceramic Grey V2), the cured
isotropic strain has magnitude L · √3 ≈ 0.0156 — which trivially
exceeds any "interior gradient" threshold that's small enough to be
sensitive.

The detector reports a hit on every layer that has a part surface
(i.e. every layer). The signal looks alarming but isn't actionable:
the user can't avoid the surface of their part.

## How it manifested

t2f3 v1 emitted 4484 CohesiveFailure warnings for the 4492-layer
lilith torso — 99.8% of layers. Every layer had a Frobenius gradient
maximum of exactly 0.0156, matching L · √3 to four decimal places.

## The fix: skip cured-vs-empty pairs

Only measure gradient between two voxels that are BOTH cured (both
non-zero). For a binary cured/empty field this is one predicate at
the inner loop:

```rust
let both_cured = |a, b| a != zero && b != zero;
if !both_cured(a, b) { continue; }
```

After the fix: same lilith run dropped from 4484 to 4082 CohesiveFailure
events. The remaining 4082 reflect REAL interior strain steps (cure-
extent varying between adjacent voxels in the part interior); they
may still need threshold tuning but at least they're the right signal.

## When to suspect this trap

Any "gradient" or "delta" measure over a voxel field where:
- one of the states means "empty / outside the part / uncured liquid"
- and the empty state is encoded as a sentinel value (zero, NaN, etc.)
- and you're measuring a metric that doesn't already discount the
  sentinel

If your detector fires on every layer, audit the loop for cured-vs-
empty pairs.

## See also

- KB-161 §"Boundary conditions" (strain gradient at bbox edges is
  undefined)
- `gradient_layer_max` implementation in
  `crates/resinsim-core/src/values/strain_field.rs`
- `docs/patterns/anti/hydrostatic-strain-dead-warpage-detector.md` —
  sibling anti-pattern surfaced by the same lilith torso run
