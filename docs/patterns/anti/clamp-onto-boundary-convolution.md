---
issue: t2f2-light-crosstalk-convolution
date: 2026-05-20
---

# Anti-pattern: clamp-onto-boundary for convolution at absorbing edges

## Symptom

A Gaussian-convolution implementation uses `index.clamp(0, n-1)` to
handle out-of-bounds samples at the field boundary:

```rust
for k in 0..len {
    let src = (i as i32 + (k as i32 - radius)).clamp(0, n as i32 - 1) as usize;
    acc += kernel[k] * input[src];
}
```

This silently folds out-of-bounds kernel weight back onto the boundary
cell. For a kernel of radius rz at index 0, the centre cell receives
contributions from kernel offsets 0, 1, 2, …, rz PLUS the would-be-
contributions from offsets -rz, -rz+1, …, -1 (all redirected to
column[0] via clamp).

## Why it's wrong for mSLA

The voxel field boundary in resinsim represents a PHYSICAL surface:
the build-envelope wall (XY) or the vat floor / above-resin headspace
(Z). These are **absorbing** boundaries — photons that hit them are
absorbed into the surrounding hardware, NOT reflected back into the
resin.

Clamp-onto-boundary models a **reflective** boundary, which would
unphysically concentrate energy at the boundary cell.

## Fix

Use **SKIP** semantics — when the sample index would be out of bounds,
`continue` past the iteration without contributing:

```rust
for k in 0..len {
    let src = i as i32 + (k as i32 - radius);
    if src < 0 || src >= n as i32 {
        continue; // SKIP — clamp-to-zero edge
    }
    acc += kernel[k] * input[src as usize];
}
```

The in-bounds output is NOT renormalised. Total integrated weight
sums to less than 1 near the boundary, which is correct: energy past
the boundary is lost.

## Test predicate that discriminates

For an impulse at index 0 with σ=1 (kernel radius 3, 7 weights):

- SKIP semantics: `output[0] = kernel[3] × input[0]` (centre weight
  only; kernel offsets -3, -2, -1 sample positions -3, -2, -1 which
  are out-of-bounds and skipped).
- CLAMP semantics: `output[0] = (kernel[0] + kernel[1] + kernel[2] +
  kernel[3]) × input[0]` (all four kernel weights at offset ≤ 0 fold
  onto index 0).

For a typical σ=1 kernel, the SKIP value is ~50% of the CLAMP value
— easy to discriminate in a test.

## Where this applies

- 1D Z convolution on per-column cure-dose deltas at iz=0 and iz=nz-1
  (vat floor + above-resin headspace).
- 2D XY convolution on per-layer intensity grids at the LCD pixel
  boundary (build envelope wall).
- Any voxelised photon-transport simulation where field boundaries
  represent absorbing surfaces (not reflective).

## When clamp-onto-boundary IS correct

If the field boundary represents a **periodic** domain (e.g. a
tile pattern repeating), then the correct convention is `(src + n) %
n` — true wrap-around, not clamp-onto-boundary either. Clamp is
almost never the right choice; SKIP for absorbing, MOD for periodic.

## See also

- ADR-0018 §2 "Z-edge clamp policy: SKIP" — the resinsim-specific
  application of this pattern (cited the v3-to-v4 fix that introduced
  the explicit `continue` guard).
- `docs/patterns/post-attenuation-z-conv-on-cure-dose-delta.md` —
  the Z-direction convolution where this edge handling matters.
- `docs/patterns/bit-exact-parity-proptest-for-pure-wrapper-refactors.md`
  — the parity proptest pattern that catches regressions to clamp
  via the SKIP-vs-CLAMP discrimination gap.
