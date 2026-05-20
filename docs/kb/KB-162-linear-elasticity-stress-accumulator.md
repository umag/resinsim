---
issue: t2f3-shrinkage-strain-stress
adr: ADR-0018
date: 2026-05-20
---

# KB-162: Linear-elasticity stress accumulator (6×6 Voigt stiffness)

## Summary

Stress at a voxel is computed from strain via isotropic linear
elasticity `σ = D : ε`, where `D` is the closed-form 6×6 Voigt
stiffness matrix derived from Young's modulus `E` and Poisson's ratio
`ν`. This KB locks the closed-form coefficients, the small-strain
validity bound, and the boundary conditions that
`StressAccumulator::strain_to_stress` MUST honour.

## Closed-form stiffness

For an isotropic linear-elastic material the 6×6 stiffness `D` has
three distinct coefficients:

```text
D_diag  = E · (1 − ν) / ((1 + ν)(1 − 2ν))     normal-normal diagonal
D_off   = E · ν / ((1 + ν)(1 − 2ν))           normal-normal off-diagonal
G       = E / (2 · (1 + ν))                   shear modulus
```

The mapping σ_voigt = D · ε_voigt (where σ_voigt and ε_voigt are the
6-component Voigt vectors `[xx, yy, zz, yz, xz, xy]`) expands to:

```text
σ_xx = D_diag · ε_xx + D_off · (ε_yy + ε_zz)
σ_yy = D_diag · ε_yy + D_off · (ε_xx + ε_zz)
σ_zz = D_diag · ε_zz + D_off · (ε_xx + ε_yy)
σ_yz = 2 · G · ε_yz       ← engineering shear convention
σ_xz = 2 · G · ε_xz       ← (γ = 2 · ε_ij in StrainTensor)
σ_xy = 2 · G · ε_xy
```

The factor of 2 on the shear components reflects the engineering
shear convention adopted by `StrainTensor::xy()`, where the stored
component is the tensor shear `ε_ij`, not the engineering shear
`γ = 2·ε_ij`. The factor lands in the stress side so the calling
convention stays self-consistent (the `from_strain_linear_elastic`
unit test asserts σ_xy = 2G·ε_xy explicitly).

## Domain restrictions

- **E > 0, finite**. `ResinProfile.validate()` enforces this.
- **ν strictly in (-1, 0.5)**. The upper bound is strict because
  `ν = 0.5` is the incompressible limit and makes `(1 - 2ν) = 0` —
  divide-by-zero in `D_diag` and `D_off`. The lower bound is
  similarly strict for completeness; real materials sit in `[0, 0.5)`.
  `ResinProfile.validate()` enforces this with `is_finite() && ν > -1 && ν < 0.5`.

## Small-strain validity

Linear elasticity assumes infinitesimal strain (`|ε| ≪ 1`). The
literature upper bound for "good fit" is ~5% strain; beyond that,
geometric non-linearities matter. For photopolymer shrinkage the
range is:

- Generic Standard (KB-142): 1.5 % linear → ε_max ≈ 0.015 (well within
  small-strain).
- Tough ABS-like (KB-142): 2.4 % linear → ε_max ≈ 0.024 (still
  comfortable).
- High-shrinkage resins (KB-142 maximum): 3.0 % → ε_max ≈ 0.030 (at
  the upper edge of "small"; non-linear corrections start to be
  meaningful but the linear prediction is still useful as a first
  approximation).

A Tier-3 follow-on with finite-strain non-linear elasticity is
deliberately deferred.

## Cured-layer-locks-strain implication for stress

Per KB-161, strain at a voxel is fixed once that voxel cures. The
stress field inherits this: `StressAccumulator::strain_to_stress` is
a pure-function mapping from a single voxel's ε to its σ, so as long
as ε doesn't change, σ doesn't change. The orchestrator's per-layer
iteration order (cure → strain → stress per layer slab, never
revisited) enforces this implicitly.

If a future Tier-2 model introduces a force that re-updates already-
cured voxel strain (e.g. thermal post-cure contraction in t2f4), the
stress field will need an explicit "additive update" API. v1 ships
with `StressField::accumulate_at` as a `set` operation; the
"accumulate" naming is forward-compat reservation rather than v1
semantics.

## Von Mises threshold for `WarpingRisk`

The downstream `FailurePredictor::predict_strain_failures` (ADR-0018
decision 5) uses the layer-maximum von Mises stress as a yield-style
scalar magnitude:

```text
σ_vm = √( ½ · [(σ_xx − σ_yy)² + (σ_yy − σ_zz)² + (σ_zz − σ_xx)²]
            + 3 · (σ_yz² + σ_xz² + σ_xy²) )
```

Thresholds (v1, literature defaults):

- Warning: `σ_vm ≥ 0.5 × resin.tensile_strength_mpa()`
- Critical: `σ_vm ≥ resin.tensile_strength_mpa()`

The 0.5× warning margin is a safety factor against the calibration
uncertainty on both σ_vm (E ±50% per KB-163 when uncalibrated) AND
tensile_strength_mpa (KB-140 documented spread). Real-world tuning
via Athena II is the follow-on.

## References

- KB-140 — Tensile strength range (downstream threshold anchor).
- KB-161 — Cure-extent → free-shrinkage strain (upstream).
- KB-163 — Photopolymer E + ν literature ranges + uncertainty.
- ADR-0017 — Voxel cure field architecture.
- ADR-0018 — t2f3 design decisions.
- `docs/patterns/single-source-arrhenius-helper.md` — Ec(T) is
  upstream of this KB; not directly involved in the stress math.
