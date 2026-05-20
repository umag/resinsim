---
issue: t2f3-shrinkage-strain-stress
adr: ADR-0018
date: 2026-05-20
---

# KB-161: Cure-extent → free-shrinkage strain model

## Summary

Photopolymer resin shrinks by 0.9–2.4 % (linear) on full cure
(KB-142 range). The per-voxel free-shrinkage strain at any point in a
simulation is the product of the resin's full-cure shrinkage and the
voxel's **cure-extent fraction** — a dimensionless `[0, 1]` measure of
how fully cured that voxel is. This KB locks the unit chain and the
boundary conditions that downstream code MUST follow to avoid the
unit-accounting bugs flagged in the t2f3 round-2 plan review.

## Unit chain

1. **CureField stores cumulative absorbed dose** `D(x, y, z)` in
   mJ/cm² (Beer-Lambert input, KB-103). Mutated via `add_dose`;
   monotonically non-decreasing.
2. **Beer-Lambert → cure depth (per voxel):**
   ```text
   Cd(x, y, z) = Dp × ln( D(x, y, z) / Ec(T) )    when D > Ec(T)
              = 0                                  otherwise (undercured)
   ```
   Units: `[µm] = [µm] × ln([mJ/cm² / mJ/cm²])`. `Ec(T)` is the
   Arrhenius-corrected critical energy (KB-153 / single-source
   helper at `CureCalculator::ec_at_temp`).
3. **Cure-extent fraction:**
   ```text
   C(x, y, z) = clamp( Cd(x, y, z) / h_layer, 0, 1 )
   ```
   `h_layer` is the **effective layer height** in µm (CTB-authoritative
   per ADR-0005, NOT the recipe layer height when they disagree). The
   clamp is the boundary contract: a voxel that cures past its layer
   height is "fully cured" (C = 1); a voxel that doesn't reach Ec is
   "uncured liquid" (C = 0).
4. **Free shrinkage strain (per voxel, isotropic):**
   ```text
   ε_ii(x, y, z) = - L × C(x, y, z)     for i = x, y, z
   ε_ij(x, y, z) = 0                    for i ≠ j
   ```
   `L = ResinProfile.linear_shrinkage_pct / 100`, dimensionless
   fraction. The sign convention is **negative for compressive**: the
   cured voxel occupies less space than the uncured monomer. This
   matches the downstream linear-elasticity sign convention in
   KB-162.

## Invariant: `|ε| ≤ L · √3`

For any C ∈ [0, 1], the magnitude of the Voigt-form strain tensor is
bounded:

```text
|ε|_Frobenius = √(3·L²·C²)  = L·C·√3  ≤  L·√3
```

`ShrinkageCalculator::free_shrinkage_strain_at_voxel` is property-
tested to honour this bound across the cure-extent domain. A failure
of this invariant means the unit chain above was violated (the most
common bug being `linear_shrinkage_pct` used as a fraction rather
than as a percentage — a 100× error).

## Cured-layer-locks-strain

Once a voxel cures, its free-shrinkage strain is fixed. Late-layer
light penetration into already-cured voxels (which the CureField
DOES record cumulatively per ADR-0017 §6) does NOT update the strain
— cured polymer no longer undergoes free shrinkage. The
implementation enforces this via `StrainField::lock_strain_at`,
which errors on overwrite, and the SimulationRunner per-layer loop
that walks only `iz = current_layer` voxels per pass.

## Boundary conditions

- **C = 0** (uncured liquid): ε = `StrainTensor::zero()`. Cured-layer-
  locks-strain does not apply yet (the voxel may cure on a later
  layer).
- **C = 1** (fully cured): ε_ii = -L exactly; ε.magnitude() = L·√3.
- **Strain gradient at bbox edges**: undefined. The
  `StrainField::gradient_layer_max` accessor skips boundary voxels
  per the FailurePredictor's "reports what it can detect" policy
  (ADR-0018 decision 5). This produces a known false-negative at the
  part's outermost layer — surfaced in KB-163 as a follow-up
  calibration target.

## Uncertainty band

- `linear_shrinkage_pct` from a vendor data sheet: ±10% typically
  (KB-142 range).
- Heterogeneous cure (incomplete polymerization, off-stoichiometric
  initiator): ±20% on the actual ε achieved at full nominal cure.
- Off-stoichiometric initiator concentration (KB-160 photoinitiator
  depletion exhausting the field before D reaches the nominal full-
  cure threshold): adds ε underestimation.

Combined uncertainty for v1 predictions: **±20% on ε magnitude**.
Calibration via measured cured-vs-green dimensions on a calibration
test bar (Athena II) is the follow-on path.

## References

- KB-100 — Beer-Lambert penetration depth + critical energy.
- KB-103 — Cure depth `Cd = Dp × ln(E/Ec)` primitive.
- KB-142 — Linear-shrinkage standard range 0.9–1.5 % for
  photopolymer; up to 2.4 % for tough ABS-like.
- KB-153 — Arrhenius Ec(T) correction.
- KB-160 — Photoinitiator depletion (cure-extent saturation floor).
- KB-162 — Linear-elasticity stress accumulator (downstream).
- KB-163 — Photopolymer Young's modulus + Poisson's ratio defaults
  (uncertainty band reference).
- ADR-0017 — Voxel cure field + photoinitiator depletion (CureField
  contract).
- ADR-0018 — Per-voxel shrinkage strain + residual stress
  (this KB's anchor).
