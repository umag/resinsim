---
issue: t2f3.1-post-impl-calibration-followups
date: 2026-05-20
---

# UAT: Honest-zero yield fraction on calibrated solid geometry

**ADR-0018 §9 note.** The per-voxel `voxel_yield_fraction` criterion
fires at `tensile_strength_mpa` (no arbitrary safety factor — von Mises
generalises uniaxial tensile yield to multi-axial states). On the
current free-shrinkage stress model the criterion produces honest zero
on calibrated profiles — empirically validated on the lilith torso
during t2f3 (σ_vm peak 5.71 MPa vs Elegoo Ceramic Grey's tensile
38 MPa = 0 yield fraction across all layers). These UATs lock that
"honest zero" behaviour at the integration-test layer, paired with a
companion non-zero strain assertion that catches the magnitude-
collapse direction the honest-zero claim is blind to (see
`docs/patterns/honest-zero-companion-nonzero-pair.md`).

## UAT-1: All layers report Some(0.0) voxel yield fraction on calibrated solid

**Rationale.** Locks the empirical t2f3 baseline. A future regression
that 100×s σ_vm (e.g. Pa↔MPa unit error in the wrong direction) would
flip Some(0.0) to a non-zero fraction and trip this UAT. Inline
coverage:
`crates/resinsim-core/tests/voxel_strain_stress_integration.rs::
honest_zero_yield_fraction_on_generic_standard_solid`.

```gherkin
Scenario: Calibrated generic profile produces honest-zero yield fraction
  Given a ResinProfile generic_standard (E = 2000, ν = 0.35, z_ratio = 1.5)
    And a 4-layer 3×3 solid_mask geometry
    And voxel-mode (--voxel-cure-mm = 0.5)
  When the SimulationRunner runs to completion
  Then every layer's voxel_yield_fraction is Some(0.0)
    # Strict equality (NOT a tolerance) — yield_fraction computes exact
    # zeros via the cured_count == 0 OR yielded_count == 0 early-return
    # paths. Any non-zero value indicates a real magnitude regression.
```

## UAT-2: Strain field magnitude non-zero on at least one layer (companion guard)

**Rationale.** The honest-zero assertion is BLIND to magnitude
COLLAPSE (e.g. MPa → Pa unit error, missing multiply, scalar default
to 0.0) — a fully-collapsed strain field also produces
voxel_yield_fraction = Some(0.0) and the honest-zero UAT silently
passes. This companion UAT asserts at the strain-field cache layer
(one model layer upstream of the yield-fraction) that at least one
layer has non-zero strain magnitude. Inline coverage:
`voxel_strain_stress_integration.rs::nonzero_strain_magnitude_on_generic_standard_solid`.

```gherkin
Scenario: Strain field cache populated with non-zero magnitude on at least one layer
  Given a ResinProfile generic_standard (E = 2000, ν = 0.35, z_ratio = 1.5)
    And a 4-layer 3×3 solid_mask geometry
    And voxel-mode (--voxel-cure-mm = 0.5)
  When the SimulationRunner runs to completion
  Then at least one layer has strain_magnitude_max > 0.0
    # Pairs with UAT-1: catches the magnitude-collapse direction the
    # honest-zero claim is blind to. Together UAT-1 + UAT-2 lock both
    # the ≥6× σ_vm blow-up and the full-collapse-to-zero regression
    # classes.
```
