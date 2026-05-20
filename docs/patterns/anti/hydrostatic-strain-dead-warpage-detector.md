---
issue: t2f3-shrinkage-strain-stress
date: 2026-05-20
---

# Anti-pattern: isotropic free shrinkage produces a dead-on-arrival yield detector

## The mistake

Model free shrinkage as isotropic (ε_xx = ε_yy = ε_zz) → apply linear
elasticity → use von Mises stress as the threshold for a failure
predictor.

The model produces σ_xx = σ_yy = σ_zz (hydrostatic stress, no
deviatoric component). Von Mises is invariant under hydrostatic stress
by construction (it IS the deviatoric magnitude). The yield detector
returns 0 on every workload. The bug is silent: there's no exception,
no warning, just a feature that fires zero events.

## How it manifested

t2f3 v1 shipped with isotropic ε_ii = -L · C. The first real-print run
(lilith torso, Elegoo Ceramic Grey V2, 30 µm) produced:
- σ_vm max across all 4492 layers: 0.0 MPa
- WarpingRisk emissions: 0

The model was mathematically working. The criterion was physically
correct. The signal was empty.

## Why it's structural, not a tuning issue

Hydrostatic stress contains real physical information (volumetric
compression, pore pressure analogue). But every yield criterion based
on a deviatoric measure (von Mises, Tresca, Drucker-Prager-with-zero-
friction) returns zero by definition. The fix is not threshold tuning
— it's breaking the symmetry at the strain stage.

## The fix: anisotropic free shrinkage

Real photopolymer cure is NOT isotropic at the process level. Layer-
by-layer printing constrains XY shrinkage (cured layer below holds
the new layer in plane) while leaving Z free to deform. ε_zz exceeds
ε_xx = ε_yy. With volume-conserving redistribution (trace preserved),
the calibration anchor (vendor `linear_shrinkage_pct`) keeps its
meaning while the deviatoric component becomes non-zero. KB-164
documents the canonical mapping.

After the fix: σ_vm on the same lilith run jumped from 0 to 5.71 MPa
per layer. Still below tensile_strength_mpa = 38 (so WarpingRisk
still fires 0 — that's a separate model gap, see
`docs/patterns/honest-zero-with-model-gap-caveat.md`), but at least
the detector now operates on real data.

## When to suspect this trap

Any predictor of form `if scalar_yield_measure(σ) > threshold` where
σ is derived from a single-component strain source via linear
elasticity. The smoking gun: σ_vm max = 0 on every test case. Don't
spend time tuning thresholds before checking whether the stress field
has deviatoric content at all.

## See also

- KB-162 (linear elasticity 6×6 Voigt stiffness)
- KB-164 (Z/XY shrinkage anisotropy)
- ADR-0018 §9 / Decision 10 (anisotropic shrinkage decision)
- `docs/patterns/honest-zero-with-model-gap-caveat.md` — sibling
  pattern for the next layer of model-vs-threshold mismatch
