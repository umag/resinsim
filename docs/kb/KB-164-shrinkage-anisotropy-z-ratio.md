---
issue: t2f3-shrinkage-strain-stress
adr: ADR-0018
date: 2026-05-20
---

# KB-164: Z / XY shrinkage anisotropy ratio (layer-by-layer constraint)

## Summary

`ResinProfile` gains `shrinkage_anisotropy_z_ratio: Option<f32>` in
the post-t2f3 calibration pass. When `None`, the strain pipeline falls
back to the v1 literature-engineering estimate
`DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO = 1.5`. Ratio > 1.0 means the
cured part shrinks **more in Z than in XY** — the empirical norm for
layered MSLA / DLP prints. The mapping preserves `linear_shrinkage_pct`'s
vendor-data-sheet meaning via volume conservation, so an isotropic
resin (ratio = 1.0) produces exactly the legacy `ε_xx = ε_yy = ε_zz`
strain field.

## Why anisotropic shrinkage matters

The initial t2f3 ship used isotropic free shrinkage (`ε_xx = ε_yy = ε_zz`).
This produced **hydrostatic stress** from linear elasticity (σ_xx =
σ_yy = σ_zz, no deviatoric component), and von Mises is invariant
under hydrostatic stress, so **WarpingRisk was dead-on-arrival** — it
emitted 0 events on every workload. The lilith torso run at 30 µm
surfaced this: 0 WarpingRisk against 4484 (mostly false-positive)
CohesiveFailure events.

The fix is to break the hydrostatic symmetry at the strain stage.

## Physical mechanism

During each layer's cure pass:

1. The new layer polymerises while bonded to the cured layer below.
2. **In the XY plane**, shrinkage is constrained: the newly cured
   material adheres to the rigid layer below; shrinking in XY would
   require detaching or shearing. The constraint suppresses XY
   shrinkage.
3. **In the Z direction**, the top of the new layer is exposed to
   liquid resin (or air after release). Shrinkage in Z is free to
   deform — there is no constraint above.
4. **Net result**: the cured part shrinks more in Z than in XY. Bowls
   curl upward at the edges (the classic MSLA warping signature).
   The lithium-thin layered shrinkage anisotropy is a process artifact,
   not a material property.

## v1 default value

```rust
pub const DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO: f32 = 1.5;
```

Literature anchors (no direct shrinkage measurement found in the
2026-05-20 survey; modulus anisotropy serves as a proxy):

- **PMC5344561** (Anisotropy of Photopolymer Parts Made by DLP, 2017):
  Castable Blend resin E_vertical/E_horizontal = **1.27** (untreated),
  Visijet FTX Green = **1.39** (untreated). Post-curing reduced both
  toward 1.0. The Z/XY modulus anisotropy supports a comparable Z/XY
  shrinkage anisotropy ratio in the 1.2–1.5 range.
- **Engineering heuristic** for warping: MSLA prints typically come
  off the build plate ~1.5–2x more dimensionally short in Z than
  predicted by isotropic shrinkage — consistent with a Z/XY shrinkage
  ratio of ~1.5.

**v1 uncertainty band: ±0.3** (covers 1.2–1.8 range). Calibration via
Athena II tensile + DIC on a printed test specimen is the follow-on.

## Volume-conserving mapping

With `ε_iso = -L · C` (the legacy isotropic strain magnitude that
`ResinProfile.linear_shrinkage_pct` calibrates against), and Z/XY
ratio `r`:

```text
factor_xy = 3 / (2 + r)
factor_z  = r · factor_xy = 3·r / (2 + r)
ε_xx = ε_yy = factor_xy · ε_iso
ε_zz       = factor_z  · ε_iso
shear components = 0
```

For `r = 1.0`: `factor_xy = factor_z = 1.0`, recovers legacy isotropic
mapping.

For `r = 1.5` (v1 default):
- `factor_xy = 3 / 3.5 ≈ 0.857`
- `factor_z  = 1.5 · 0.857 ≈ 1.286`
- For ceramic grey (L = 0.009, full cure): ε_xx = ε_yy = -0.00771,
  ε_zz = -0.01157. Compare against legacy isotropic -0.009 on all axes.

**Trace invariance** (volume conservation):

```text
trace(ε) = ε_xx + ε_yy + ε_zz
        = 2·factor_xy · ε_iso + factor_z · ε_iso
        = (2 · 3/(2+r) + 3r/(2+r)) · ε_iso
        = ((6 + 3r) / (2+r)) · ε_iso
        = 3 · ε_iso
```

So `linear_shrinkage_pct`'s vendor-data-sheet meaning — total
volumetric shrinkage — is preserved regardless of `r`. A property test
in `strain_tensor.rs` exercises this invariant across the full
domain.

## Consequence for WarpingRisk

With anisotropic ε, the linear-elasticity stress field σ = D : ε now
has a deviatoric component proportional to `(factor_z − factor_xy)`.
For `r = 1.5`, the difference is ~0.43 · ε_iso — enough to register
in von Mises and let `WarpingRisk` fire on real Z-direction
geometric features.

## Per-resin recommendations (v1)

| Resin | `shrinkage_anisotropy_z_ratio` | Rationale |
|-------|--------------------------------|-----------|
| `generic_standard` | 1.5 (explicit) | KB-164 midpoint; representative SLA resin baseline |
| `generic_abs_like` | (none — uses default 1.5) | Tough resins may have slightly different ratio; calibrate if measured |
| `elegoo_ceramic_grey_v2` | (none — uses default 1.5) | Ceramic-filled formulations diverge; calibrate via Athena II |
| `liqcreate_premium_black` | (none — uses default 1.5) | No vendor-published data |

## References

- KB-142 — Linear shrinkage range (legacy isotropic anchor).
- KB-161 — Cure-extent → free-shrinkage strain unit chain (upstream).
- KB-162 — Linear-elasticity 6×6 Voigt stiffness (consumer of the
  anisotropic ε field).
- KB-163 — Photopolymer E + ν defaults.
- ADR-0018 — t2f3 design decisions.
- **PMC5344561** — Anisotropy of Photopolymer Parts Made by Digital
  Light Processing (2017). Open access:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC5344561/
- Lilith torso 2026-05-20 run that surfaced the WarpingRisk = 0
  signal — the immediate post-t2f3 calibration trigger.
