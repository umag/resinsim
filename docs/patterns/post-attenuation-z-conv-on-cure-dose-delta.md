---
issue: t2f2-light-crosstalk-convolution
date: 2026-05-20
---

# Pattern: Post-attenuation 1D Z convolution on per-column cure-dose deltas

## Context

A voxel-resolved cure simulator using Beer-Lambert per-pixel column
exposure (ADR-0017 / t2f1) needs to model axial photon scatter
(volumetric resin scatter spreading cure dose into layers above and
below the source layer). The voxel field STORAGE is anisotropic — Z
is layer-indexed not mm-indexed
(`docs/patterns/voxel-field-z-dimension-is-layer-count.md`).

Two formulations exist:

1. **Pre-attenuation Z dispatch.** For each pixel exposure at
   `iz_top = layer`, also call `apply_column_exposure` at offset
   `iz_top = layer + kz` for kz in [-rz..rz], weighted by a 1D Z
   Gaussian kernel. Each shifted virtual source independently runs a
   FULL Beer-Lambert column-march from its offset position.
2. **Post-attenuation Z convolution on the per-column dose.** Run
   Beer-Lambert once at `iz_top = layer`, obtaining the dose column
   (Vec<f32> of length nz). Apply a 1D Z Gaussian convolution to the
   dose column. Then deposit the convolved column into the persistent
   fields.

## Decision

Adopt **post-attenuation Z convolution on the per-column cure-dose
column** (formulation 2 above). See ADR-0018.

## Why post-attenuation beats pre-attenuation

In the σ_z ~ layer_height regime (typical mSLA: σ_z = 40 µm, layer
height = 50 µm — σ_z_layers = 0.8), the two formulations diverge:

- **Pre-attenuation** over-spreads dose into already-cured upper
  layers, because each kz<0 dispatch runs a fresh Beer-Lambert from a
  shifted `iz_top` higher up the column — the deposited dose at iz=L
  includes contributions both from the centre dispatch (kz=0) AND from
  the kz<0 dispatches' attenuated march FROM ABOVE. Doubly-counts the
  source layer.
- **Post-attenuation** operates on the dose-vs-iz profile that has
  already absorbed the column-depth Beer-Lambert decay. The
  convolution respects this profile, so the resulting cure dose at
  iz=L is a weighted average of dose-at-L and dose-at-neighbouring-iz,
  not a fresh Beer-Lambert restart.

Post-attenuation is also empirically equivalent to "compute the unscat-
tered cure dose, then smear the dose field by σ_z" — a clear physical
intuition.

## How the orchestration works

The integration in `simulation_runner::apply_voxel_cure_for_layer`:

```rust
// (1) Build 2D intensity grid (mask × uniformity × LED power).
let mut intensity = Array2::<f32>::zeros((nx as usize, ny as usize));
for (ix, iy) in mask.iter_solid() { intensity[(ix, iy)] = ...; }

// (2) Optional XY pre-conv (σ_xy active).
if let Some(σ_xy) = printer.crosstalk_sigma_xy_um() {
    LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &xy_kernel, &mut xy_scratch)?;
}

// (3) Build Z kernel (σ_z active) + reusable per-column scratch.
let z_kernel = printer.crosstalk_sigma_z_um().map(|σ_z| {
    LightCrosstalkCalculator::build_separable_kernel(σ_z / layer_height_um)
});
let mut z_scratch = vec![0.0; nz as usize];

// (4) Per-pixel: compute dose column → Z conv → deposit.
for (ix, iy) in <full grid if xy active, else mask.iter_solid()> {
    let pi_snapshot = state.pi.column_at(ix, iy)?;
    let mut dose_col = VoxelCureCalculator::compute_column_exposure(
        &pi_snapshot, layer, nz, intensity[(ix, iy)], exposure, dp, k_d, layer_height_um,
    )?;
    if let Some(zk) = z_kernel.as_ref() {
        LightCrosstalkCalculator::apply_separable_1d_z(&mut dose_col, zk, &mut z_scratch)?;
    }
    for iz in 0..nz {
        let d = dose_col[iz as usize];
        if d == 0.0 { continue; }
        state.cure.add_dose(ix, iy, iz, d)?;
        state.pi.deplete(ix, iy, iz, k_d, d)?;
    }
}
```

## Single-source Beer-Lambert preservation

The cure-dose column is computed by `VoxelCureCalculator::compute_column_exposure`,
a **pure functional sibling** of `apply_column_exposure`. Both share
the same Beer-Lambert math via the helper (literally the same code
path inside the calculator module). The original
`apply_column_exposure` becomes a thin wrapper:

```rust
pub fn apply_column_exposure(cure, pi, ix, iy, iz_top, ...) -> Result<()> {
    let pi_snapshot = pi.column_at(ix, iy)?;
    let dose_col = compute_column_exposure(&pi_snapshot, iz_top, nz, ...)?;
    for iz in iz_top..nz {
        let d = dose_col[iz];
        if d == 0.0 { break; }
        cure.add_dose(ix, iy, iz, d)?;
        pi.deplete(ix, iy, iz, k_d, d)?;
    }
    Ok(())
}
```

This refactor is gated by a **bit-exact parity proptest** (`parity_apply_vs_compute_proptest`)
that randomises inputs and asserts both forms produce
byte-identical CureField + PhotoinitiatorField output. The parity test
is the load-bearing regression guard.

## Memory characteristics

Per layer, the per-column post-conv path allocates:
- One 2D `Array2<f32>` of shape (nx, ny) for the intensity grid (~95 KB
  at Mars 5 Ultra default 430×220).
- One 2D `Array2<f32>` scratch buffer (~95 KB, same shape) — only when
  σ_xy active.
- One `Vec<f32>` PI snapshot of length nz per pixel iteration (~18 KB at
  nz=4492; REUSED across pixels via the implicit allocator).
- One `Vec<f32>` dose column of length nz (~18 KB; REUSED).
- One `Vec<f32>` Z scratch column of length nz (~18 KB; PRE-ALLOCATED
  once and reused).

Total: ~200 KB per layer + 18 KB per-pixel transient. Cheap.

## Co-scattering of cure dose AND PI depletion

The KB-160 photoinitiator depletion law is multiplicative-exponential:
`C_after = C_before × exp(-k_d × delta_dose)`. This is **non-linear**
in delta_dose. Naïve linear convolution of the depletion AMOUNT would
be physically incorrect.

The fix: convolve the cure-dose column (LINEAR in dose), THEN compute
depletion locally per voxel using the convolved dose AT THAT VOXEL.
Because the multiplicative law composes correctly:
`C_after = C_before × exp(-k_d × convolved_dose[iz])` — depletion at
iz uses the actual photons absorbed at iz (which includes scattered
contributions from neighbouring layers), without ever needing to
linearise the non-linear law.

This is a v5 architectural improvement over v3/v4's planned signed-
delta `add_concentration` API: there's no signed-delta API, no linear-
depletion approximation, no risk of clamping bounds on PI. Just
convolved dose → local exponential depletion.

## Z-edge SKIP semantics

The 1D Z convolution's clamp-to-zero at field boundaries (out-of-bounds
samples contribute zero AND in-bounds output is NOT renormalised) is
the **SKIP** policy. Photons that would have scattered past iz=0 or
iz=nz-1 are physically lost (vat floor / above-resin headspace).

The alternative (clamp-onto-boundary, folding the out-of-bounds weight
back onto the boundary cell) would represent reflective boundaries —
wrong for an mSLA build envelope. The SKIP policy matches the XY 2D
convolution's clamp-to-zero at the LCD pixel grid boundary.

## See also

- `docs/adr/0018-light-crosstalk-3d-gaussian-convolution.md` — ADR
  documenting the architectural decision + rejected alternatives.
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — the
  storage anisotropy decision (Z = layer count, not mm) that
  motivates `σ_z_layers = σ_z_um / layer_height_um`.
- `docs/adr/0017-voxel-cure-field-and-photoinitiator-depletion.md` —
  the t2f1 voxel cure path this pattern extends.
- `docs/patterns/single-source-arrhenius-helper.md` — pattern reused
  for the `apply_column_exposure` ↔ `compute_column_exposure`
  refactor (Beer-Lambert math lives in ONE place).
