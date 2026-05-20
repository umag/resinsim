---
issue: t2f3-shrinkage-strain-stress
date: 2026-05-20
---

# Pattern: volume-conserving anisotropic redistribution preserves the calibration anchor

## The setting

You have a model that calibrates against a single scalar anchor (e.g.
`ResinProfile.linear_shrinkage_pct` from a vendor datasheet — a
single number representing total linear shrinkage). The model
originally produced an isotropic field where that anchor IS the
per-axis magnitude.

You want to add directional anisotropy (e.g. Z direction shrinks more
than XY because layer-by-layer cure constrains XY) WITHOUT requiring
re-calibration of the existing anchor.

## The trick

Define a ratio r = factor_z / factor_xy (the only knob the new
anisotropy adds). Solve for factor_xy and factor_z subject to the
volume-conservation invariant `2 · factor_xy + factor_z = 3`:

```text
factor_xy = 3 / (2 + r)
factor_z  = r · 3 / (2 + r)
```

Apply to the isotropic value:

```text
ε_xx = ε_yy = factor_xy · ε_iso
ε_zz       = factor_z  · ε_iso
```

Properties:
- **r = 1.0** recovers `factor_xy = factor_z = 1.0` (legacy isotropic).
- **Trace invariance**: `ε_xx + ε_yy + ε_zz = 3 · ε_iso` for any r > 0.
- The vendor-data-sheet meaning of `linear_shrinkage_pct` is preserved
  — it's the average linear shrinkage, regardless of how that average
  is redistributed across axes.

## Why this matters

Existing TOMLs continue to calibrate the model correctly. Resin
manufacturers don't need to publish per-axis data (they don't —
KB-164 surveys this). The single-number anchor stays valid.

When future per-axis measurements become available (e.g. Athena II
tensile + DIC), the `shrinkage_anisotropy_z_ratio` field on
`ResinProfile` accepts the measured ratio without touching the
existing `linear_shrinkage_pct` anchor.

## Generalisation

The pattern applies to any anisotropy along one axis of a 3-axis
tensor field where the single-number anchor calibrates the average:
- Thermal expansion: α_z vs α_xy with vendor α_avg as the anchor.
- Optical penetration depth: Dp_z vs Dp_xy if MSLA optics introduce
  axis-dependent attenuation.
- Cure-kinetics activation energy: Ea_z vs Ea_xy if layer interface
  behaves differently.

The 2-axes (rotational-symmetric anisotropy) form has a single ratio
parameter; multi-axis anisotropy (full orthotropy) needs three
parameters and a different mapping but the same invariance principle.

## See also

- KB-164 (Z/XY shrinkage anisotropy specific to photopolymer cure)
- `StrainTensor::from_free_shrinkage` in
  `crates/resinsim-core/src/values/strain_tensor.rs`
- `docs/patterns/anti/hydrostatic-strain-dead-warpage-detector.md` —
  the deviatoric-stress fix this pattern enables
