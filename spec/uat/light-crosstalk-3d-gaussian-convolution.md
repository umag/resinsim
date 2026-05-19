---
issue: t2f2-light-crosstalk-convolution
date: 2026-05-20
---

# UAT: 3D light crosstalk (Gaussian convolution)

**ADR-0018 / t2f2 note.** These scenarios verify the t2f2 light crosstalk
path: per-printer `crosstalk_sigma_xy_um` and `crosstalk_sigma_z_um` σ
parameters drive XY 2D Gaussian pre-attenuation convolution (on the
intensity grid) and Z 1D Gaussian post-attenuation convolution (on the
per-column cure-dose + PI-depletion deltas) inside the Tier-2 voxel cure
path. Four runtime regimes (AA/BA/BB/CB/DD per ADR-0018 §2) covered.

## UAT-1: Both σ fields None — t2f1 path unchanged

**Rationale.** Profiles without crosstalk calibration must run the t2f1
no-crosstalk path unchanged. Bit-exact equivalence is asserted by the
integration test `regime_AA_both_none_bit_exact_t2f1`.

```gherkin
Scenario: crosstalk both-None matches t2f1 baseline
  Given a printer profile with both crosstalk_sigma_xy_um and crosstalk_sigma_z_um absent
  And a CTB input with per-layer masks
  When the simulation runs with --voxel-cure-mm set
  Then the produced cure_field is bit-exact equal to the t2f1 baseline
  And the produced photoinitiator_field is bit-exact equal to the t2f1 baseline
```

## UAT-2: σ_xy set — XY crosstalk produces off-pixel cure dose

**Rationale.** When `crosstalk_sigma_xy_um` is set to a value large enough
that `σ_voxels >= 1` at the configured voxel resolution, the 2D Gaussian
convolution on the intensity grid produces non-zero intensity at pixels
adjacent to the source mask. The resulting cure dose at those off-mask
voxels must be observably > 0.

```gherkin
Scenario: crosstalk_sigma_xy_um produces off-pixel cure
  Given a printer profile with crosstalk_sigma_xy_um set to a value producing σ_voxels >= 1
  And a CTB input with a single-pixel solid mask at the centre
  When the simulation runs with --voxel-cure-mm set
  Then the cure_field shows non-zero cure dose at off-mask voxels adjacent to the source pixel
  And the off-pixel cure dose pattern is 4-fold symmetric about the source pixel
```

## UAT-3: σ_z set — Z crosstalk co-scatters cure dose AND PI depletion

**Rationale.** When `crosstalk_sigma_z_um` is set, the 1D Gaussian
convolution along Z on the per-column dose column spreads cure dose
into layers above and below the source layer. KB-160 multiplicative
photoinitiator depletion is applied per voxel using the (convolved) local
dose, so depletion at neighbouring layers IS observable (NOT zero) after
a single-layer source exposure.

```gherkin
Scenario: crosstalk_sigma_z_um spreads cure dose AND depletion vertically
  Given a printer profile with crosstalk_sigma_z_um set to a value producing σ_layers >= 0.5
  And a CTB input where only layer L is masked (single layer source)
  When the simulation runs with --voxel-cure-mm set
  Then the cure_field shows non-zero cure dose in layers L-1 and L+1 of the source pixel
  And the photoinitiator_field concentration at those layers is reduced (depleted) relative to the t2f1 baseline
```

## UAT-4: Both σ fields set — 3D crosstalk in a neighbourhood around the source

**Rationale.** When both σ fields are active, the combined XY pre-attenuation
+ Z post-attenuation path produces cure dose in a 3D neighbourhood
around each source voxel. The XY and Z conv operations apply at distinct
pipeline stages (XY before Beer-Lambert column-march on the intensity
grid; Z after Beer-Lambert on the per-column dose) so the SURFACE-DOSE
spatial factor decomposes cleanly as `xy_kernel × z_kernel`.

```gherkin
Scenario: both σ active produces 3D cure dose neighbourhood
  Given a printer profile with both crosstalk_sigma_xy_um and crosstalk_sigma_z_um set to produce σ_voxels >= 1 and σ_layers >= 0.5
  And a CTB input with a single-pixel single-layer source mask
  When the simulation runs with --voxel-cure-mm set
  Then the cure_field shows non-zero cure dose in a 3D neighbourhood around the source voxel
  And the off-pixel-source-layer dose ratio matches the XY kernel ratio at that offset
```

## UAT-5: σ_xy = 0.0 rejected at validate-time

**Rationale.** `Some(0.0)` is a misconfiguration (degenerate kernel
behaviour); profile validation must reject it explicitly.

```gherkin
Scenario: crosstalk_sigma_xy_um = 0.0 rejected at validate
  Given a printer profile TOML with crosstalk_sigma_xy_um = 0.0
  When the profile is loaded and validated
  Then validation fails with an error mentioning crosstalk_sigma_xy_um
```

## UAT-6: σ_z = 0.0 rejected at validate-time

**Rationale.** Same as UAT-5 for the axial σ field.

```gherkin
Scenario: crosstalk_sigma_z_um = 0.0 rejected at validate
  Given a printer profile TOML with crosstalk_sigma_z_um = 0.0
  When the profile is loaded and validated
  Then validation fails with an error mentioning crosstalk_sigma_z_um
```

## UAT-7: σ values above MAX_SIGMA_UM rejected

**Rationale.** The 5000 µm upper bound on either σ guards against
misconfigured TOMLs that would produce kernels spanning the entire
build envelope.

```gherkin
Scenario: crosstalk σ above MAX_SIGMA_UM rejected
  Given a printer profile TOML with crosstalk_sigma_xy_um = 6000.0
  When the profile is loaded and validated
  Then validation fails with an error mentioning crosstalk_sigma_xy_um and the upper bound
```

## UAT-8: Z-edge SKIP at field boundary — no dose pileup

**Rationale.** When the source layer is at z=0 (or z=nz-1), the 1D Z
convolution would sample out-of-bounds positions kz<0 (or kz>nz-1). The
v5 implementation applies **clamp-to-zero** semantics (SKIP — out-of-bounds
samples contribute zero; in-bounds output not renormalised), which
physically represents energy lost to the vat floor / above-resin
headspace. The alternative (clamp-onto-boundary) would unphysically
fold the missing weight back onto the boundary layer, producing a dose
pileup. This is the load-bearing UAT distinguishing the correct SKIP
semantics from a regression to CLAMP.

```gherkin
Scenario: Z-edge SKIP at boundary produces no dose pileup
  Given a printer profile with crosstalk_sigma_z_um set to a value producing kernel radius >= 1
  And a CTB input where only layer 0 (the first printed layer) is masked
  When the simulation runs with --voxel-cure-mm set
  Then the cure_field shows dose at iz=0 equal to (centre kernel weight × Beer-Lambert surface dose) plus small forward-kz contributions
  And the cure_field does NOT show the dose-pileup magnitude that would result from clamp-onto-boundary
```

## UAT-9: Post-attenuation Z conv shifts peak dose to L+1

**Rationale.** Surfaced during integration test 8-D when my initial
predicate `doses[L] > doses[L+1]` failed — the actual physical signature
of post-attenuation Z convolution (v5 architecture) is that the peak
dose shifts AWAY from the source layer toward L+1, because the Beer-
Lambert column has zero values above the source (iz<L is unexposed,
the column-march starts at iz_top=L) and non-zero values below (the
march decays exponentially through iz=L..nz-1). The convolution at iz=L
sees centre + forward-only neighbours (kernel offsets -3..-1 sample
out-of-bounds zeros); the convolution at iz=L+1 sees centre + both-side
neighbours. So `dose(L+1) > dose(L)` is the v5 architectural signature
distinguishing post-attenuation from a pre-attenuation dispatch (which
would peak at L).

```gherkin
Scenario: post-attenuation Z conv shifts peak dose to layer L+1 due to asymmetric support
  Given a printer profile with crosstalk_sigma_z_um set to a value producing σ_layers ≈ 0.8 (kernel radius 3)
  And a CTB input where only layer L (well inside the print) is masked as a single-pixel source
  When the simulation runs with --voxel-cure-mm set
  Then the cure_field at the source pixel shows monotone-increasing dose from iz=L-3 to iz=L
  And the cure_field at the source pixel shows dose at iz=L+1 greater than dose at iz=L
  And the cure_field at the source pixel shows monotone-decreasing dose from iz=L+1 to iz=L+3
```

## See also

- `docs/adr/0018-light-crosstalk-3d-gaussian-convolution.md` —
  design decisions captured during planning (architecture, calibration,
  approximation regime).
- `docs/patterns/post-attenuation-z-conv-on-cure-dose-delta.md` —
  implementation pattern documenting the per-column post-attenuation Z
  conv design.
- `crates/resinsim-core/tests/voxel_cure_crosstalk_integration.rs` — 5
  end-to-end tests covering UAT-1 through UAT-4 and UAT-8 (regime-by-
  regime).
- `crates/resinsim-core/src/entities/printer_profile.rs` (validation tests)
  — covers UAT-5 / UAT-6 / UAT-7 via unit tests on `PrinterProfile::validate`.
