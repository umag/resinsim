---
issue: uat-unskip-light-crosstalk-3d-gaussian-convolution
date: 2026-08-15
---

# Anti-pattern: Asserting PI depletion is always deeper after Z convolution

## What goes wrong

When asserting that photoinitiator (PI) depletion at a position is "reduced
relative to the t2f1 baseline" (a no-Z-conv run), the assertion
`pi_with_z_conv < pi_baseline` fails at positions where the Beer-Lambert
column already deposits substantial dose in the baseline.

The Z convolution redistributes cure dose along the column. At positions
where the un-convolved column has high dose (near or below the source layer),
the convolution may REDUCE the local dose (spreading weight into originally-
zero positions above the source), which means LESS depletion — PI is higher,
not lower, than the baseline.

The assertion is correct at positions ABOVE the source layer (L-1, L-2, ...)
where the baseline dose is zero and the convolution introduces new dose. It
is NOT reliably correct at positions below or at the source layer.

## Why it survives

The intuition "convolution spreads energy → more dose everywhere → more
depletion everywhere" is appealing but wrong. Convolution conserves total
dose (modulo edge losses); it redistributes rather than amplifies. At any
position where the un-convolved dose was HIGHER than the weighted average
of its neighbours, the convolved dose is lower.

## What to do instead

Assert PI depletion against the INITIAL concentration (`pi.initial_concentration()`),
not against a no-Z-conv baseline. Any non-zero cure dose depletes PI below
initial; this is universally true regardless of whether the Z convolution
increased or decreased the local dose relative to the un-convolved column.

For positions above the source layer (where the baseline has zero dose),
comparison against the baseline IS correct and physically meaningful — but
the simpler `pi < initial` assertion covers this case too.

## How to catch it

The failure is a runtime assertion panic with the shape
`PI at L+1 must be depleted relative to baseline: 0.655 < 0.652` — the
"less than" direction is wrong. The tell is that the two values are close
(both near the initial concentration), with the Z-conv value slightly higher.

## See also

- `docs/adr/0018-light-crosstalk-3d-gaussian-convolution.md` — §Limitations,
  "σ_z regime accuracy" paragraph.
- `docs/patterns/post-attenuation-z-conv-on-cure-dose-delta.md` — the
  implementation pattern for per-column Z post-conv.
- `docs/patterns/anti/clamp-onto-boundary-convolution.md` — the related
  boundary-handling anti-pattern (CLAMP vs SKIP).
