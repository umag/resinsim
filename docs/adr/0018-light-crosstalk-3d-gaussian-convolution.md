---
issue: t2f2-light-crosstalk-convolution
date: 2026-05-20
---

# ADR-0018: 3D light crosstalk via XY pre-conv + Z post-conv

## Status

Accepted (Phase 4 of issue `t2f2-light-crosstalk-convolution`, 2026-05-20).

## Context

The Tier-2 voxel cure path (`t2f1`, ADR-0017) computes per-voxel cure
dose by running a Beer-Lambert column-march per pixel from each layer's
`iz_top = layer`, depleting photoinitiator concentration per voxel via
the KB-160 multiplicative-exponential law. The model is anchored at
**zero lateral light spread** (per-pixel exposure stays in its own
column) and **zero axial scatter** (photons that didn't decay in the
column don't bleed to adjacent layers).

Real mSLA physics violates both assumptions:

1. **LCD pixel crosstalk + lateral resin scatter.** Each LCD pixel
   emits a Gaussian-profile beam (waist radius ω₀; see KB-121 + Wei et
   al. PMC11267290), so the LIGHT field arriving at the resin surface
   is the SUPERPOSITION of all per-pixel beams. Adjacent pixels' beams
   overlap; the cure dose at any (x, y) sums contributions from
   neighbouring pixels.
2. **Volumetric resin scatter (Mie / Henyey-Greenstein).** Pigment
   particles in photopolymer scatter photons in 3D with an anisotropy
   parameter g ≈ 0.5–0.9 (strongly forward-biased single scatter).
   Photons absorbed at depth d also spread laterally and axially
   relative to their straight-line column path.

The voxel-field STORAGE is anisotropic (XY at `mask.voxel_size_mm()`,
Z at `layer_height_um` per
`docs/patterns/voxel-field-z-dimension-is-layer-count.md`). The
modelled physical phenomenon is 3D forward-peaked photon transport.
σ values in this ADR are specified in PHYSICAL units (µm) and converted
at runtime to per-axis voxel-index kernels via the storage's anisotropic
conversion factors — the per-axis kernels are anisotropic in INDEX
space because the storage is.

## Decision

**Empirical lumped-parameter approximation** of 3D photon transport via
two active per-printer parameters:

- `PrinterProfile.crosstalk_sigma_xy_um: Option<f32>` — XY lateral σ
  (combines LCD source spread + lateral component of resin scatter).
- `PrinterProfile.crosstalk_sigma_z_um: Option<f32>` — Z axial σ
  (volumetric resin scatter along the layer axis).

Both fields default to `None` and the t2f1 no-crosstalk path is bit-
exact preserved (regime AA below).

### Two-stage pipeline (per layer exposure)

**(Stage 1 — PRE-attenuation σ_xy.)** Build a 2D `Array2<f32>` intensity
grid from `(mask.iter_solid() × UniformityCalculator::intensity_factor
× state.led_power_mw_cm2)`. When `σ_xy` is `Some`, apply a separable
2D Gaussian convolution via `LightCrosstalkCalculator::apply_separable_2d`
(`σ_xy_voxels = σ_xy_um / (mask.voxel_size_mm × 1000)`). Off-mask
pixels may now have non-zero intensity.

**(Stage 2 — Beer-Lambert column-march.)** For each (ix, iy) of the
(post-conv) intensity grid: snapshot the PI column via
`PhotoinitiatorField::column_at`, call
`VoxelCureCalculator::compute_column_exposure(pi_snapshot, …)` to
obtain the per-voxel cure dose column (Vec<f32> of length nz). This is
the **pure** sibling of `apply_column_exposure` — same Beer-Lambert
math, but operates on a snapshot + returns the deltas instead of
mutating fields in place. Single-source preserved.

**(Stage 3 — POST-attenuation σ_z.)** When `σ_z` is `Some`, apply a 1D
Gaussian convolution along Z to the dose column via
`LightCrosstalkCalculator::apply_separable_1d_z`
(`σ_z_layers = σ_z_um / layer_height_um`). The cure-dose column is
LINEAR in dose, so convolution preserves total integrated dose modulo
edge losses.

**(Stage 4 — Deposit.)** For each in-bounds iz, apply the (possibly
Z-convolved) dose to the persistent fields:
- `cure.add_dose(ix, iy, iz, dose)` — accumulate cure dose.
- `pi.deplete(ix, iy, iz, k_d, dose)` — KB-160 multiplicative
  exponential depletion. **Crucially**, depletion uses the CONVOLVED
  dose at iz (not the pre-conv dose), so photons that scattered into
  iz from neighbouring layers correctly deplete initiator at iz. The
  multiplicative law composes correctly with the convolved linear dose.

### Four runtime regimes

Based on `(σ_xy.is_some(), σ_z.is_some())`:

| Regime | σ_xy | σ_z | Behaviour |
|--------|------|-----|-----------|
| **AA** | None | None | t2f1 path unchanged (bit-exact). |
| **BA** | Some | None | XY pre-conv → per-pixel exposure (no Z spread). |
| **BB** | Some | None | (same; differs from BA in σ magnitude — see tests 7-B / 7-C). |
| **CB** | None | Some | per-pixel exposure → Z post-conv on dose column. |
| **DD** | Some | Some | XY pre-conv → per-pixel exposure → Z post-conv. |

Each regime maps 1:1 to an integration test in
`crates/resinsim-core/tests/voxel_cure_crosstalk_integration.rs`.

### Z-edge clamp policy: SKIP

When the 1D Z convolution at some iz reads out-of-bounds samples (`iz +
offset < 0` or `>= nz`), those samples contribute zero to the convolution
sum AND the in-bounds output is NOT renormalised. This is the **SKIP**
policy: energy past the field boundary is physically lost (vat floor /
above-resin headspace).

The alternative (clamp-onto-boundary, folding missing weight back onto
the boundary cell) would represent reflective boundaries — wrong physics
for an mSLA build envelope. The XY 2D convolution applies the same
clamp-to-zero policy at the LCD pixel grid boundary.

### Approximation regime

The XY pre-attenuation + Z post-attenuation formulation is an
APPROXIMATION of true 3D photon transport (which would require Monte
Carlo or radiative-transfer PDE). It is **PRE-attenuation in XY** (LCD
source crosstalk applies BEFORE Beer-Lambert) and **POST-attenuation in
Z** (resin volumetric scatter applies AFTER the Beer-Lambert
column-march). This mixed pre/post split is empirically motivated:

- σ_xy captures the LCD source's lateral spread, which IS a property
  of the entering light field, not of the resin volume — applying
  it pre-attenuation is physically correct.
- σ_z captures the resin's axial scatter contribution to dose at each
  voxel — applying it post-attenuation to the deposited dose is a
  defensible first-order approximation when the scatter mean-free-path
  is comparable to or smaller than the column-march depth.

**Limitations:**

- **σ_z regime accuracy.** The post-attenuation Z conv is exact in the
  limit `σ_z << layer_height_um` (scatter much smaller than a layer).
  For σ_z comparable to layer_height (e.g. Mars 5 Ultra default
  σ_z = 40 µm at layer_height = 50 µm — σ_z_layers = 0.8, in the
  comparable regime), the approximation slightly under-spreads
  dose into very-far layers and over-spreads into very-close layers
  vs the exact radiative-transfer solution. The error is bounded by
  the kernel mass × the difference between local Beer-Lambert decay
  factors at adjacent iz; for typical mSLA configurations it is well
  below the parameter-fitting uncertainty in σ_z itself.
- **Depletion-amount-vs-fraction.** Convolving the cure DOSE (linear)
  and computing depletion locally via KB-160 multiplicative law at
  each voxel correctly handles the fact that depletion is a non-
  linear function of dose. The remaining mismatch: the local Dp at
  each voxel uses the pre-march C(z) snapshot, not the post-scatter C
  — so if two columns' Z conv outputs overlap, their cross-coupling
  through Dp(C) is missed. The fully consistent alternative
  (iterative coupled cure-dose + depletion solve per layer) is
  rejected for v1; deferred to a t2f5+ research follow-up.
- **Forward-peaked physics.** Real scatter (g ≈ 0.5–0.9) is not
  Gaussian. The empirical 2-parameter Gaussian model (σ_xy + σ_z) is
  a second-moment approximation — it captures the WIDTH of the
  scatter but not the FORWARD BIAS. Calibration of σ_xy + σ_z from
  beam-profile measurements absorbs the forward-bias error into the
  fitted σ values, which is defensible for fitted printers but
  unreliable for cross-resin extrapolation.

The exact alternative (full radiative-transfer or Monte Carlo) is
deferred to a t2f5+ research follow-up.

### Calibration

σ_xy and σ_z are EMPIRICAL lumped parameters; users should fit them
from beam-profile measurements per printer + resin combination.
Pragmatic decomposition:

- σ_xy_total = sqrt(σ_lcd² + σ_scatter²) — LCD optics + lateral
  component of resin scatter.
- σ_z = σ_scatter — axial component of resin scatter.

For an isotropic resin scatter the two components of σ_scatter (XY
and Z) are equal at the second-moment level; the LCD contribution
appears only in σ_xy. So `σ_xy >= σ_z` is expected on a typical
printer.

#### Mars 5 Ultra defaults (ESTIMATE)

- σ_xy = 8 µm — cross-checked against two independent derivations:
  - KB-121 LED/LCD geometry: pixel_pitch_um × tan(5–10° collimation
    half-angle) ≈ 8 µm for 19 µm pixel pitch.
  - Wei et al., PMC11267290 ratio: σ/pixel_pitch ≈ 0.36 → 19 µm
    pitch gives σ ≈ 6.8 µm.
  - Two derivations agree within 20%; 8 µm is a defensible round.
- σ_z = 40 µm — mid-range of typical photopolymer scatter
  mean-free-path (20–80 µm).

Both flagged ESTIMATE in `data/printers/elegoo_mars5_ultra.toml`,
pending Athena II beam-profile measurement campaign.

## Rejected alternatives

(a) **Full-print 3D voxel-grid tensor convolution as a single pass.**
Rejected NOT for memory cost but for **temporal structure**: each
layer is exposed at a different print step with potentially different
exposure_sec / layer_height (per-layer overrides) / thermal context.
A "full-print intensity tensor" doesn't naturally exist; bolting one
together per-pass would duplicate the per-layer state already managed
by the orchestrator. The per-layer XY pre-conv + per-column Z post-conv
structure matches the natural temporal/causal structure of the print
process.

(b) **Per-layer 3D slab snapshot/delta/convolve/writeback.** Considered
in v4→v5 deliberation, rejected vs the per-column approach because
per-column needs only TWO 1D vectors of size nz per pixel iteration
(~18 KB temporary, reused across pixels), while per-slab needs O(nx ×
ny × slab_extent_z) per layer. Slab extent is bounded by Beer-Lambert
decay length (~10+ layers for Mars 5 Ultra), not just kernel radius —
so per-slab grows ~5 MB+ per layer at default resolution. Per-column
also aligns with the natural data dependency (each column is
independent in Z).

(c) **Pre-attenuation Z dispatch** (v4 architecture). Rejected because
each shifted virtual source `iz_top = layer + kz` independently ran
Beer-Lambert from its offset position, over-spreading dose into already-
cured upper layers when σ_z ~ layer_height. v5 post-attenuation
operates on cure-dose deltas that have already absorbed the column-depth
dependency, so the resulting cure distribution respects the natural
cure-with-depth profile.

(d) **Lookup-table convolution kernel.** No benefit for the small
kernels in play (radius ≤ 6 for σ ≤ 2 in voxel units). The
analytical exp(-) evaluation in `build_separable_kernel` is fast and
flexible.

(e) **GPU acceleration via wgpu.** Deferred to t2f5 (which will also
unlock variable voxel resolution decoupling). The CPU implementation
is the baseline against which any future GPU port can be parity-tested.

(f) **Defer t2f2 entirely until t2f5.** Rejected: shipping now gives
sibling solvers (t2f3 shrinkage, t2f4 thermal diffusion) a stable
crosstalk interface to build against, avoids a later retrofit, and
documents the empirical-lump approximation as a known v1 simplification.

## Scaling caveat (default voxel resolution)

At default mask voxel_size_mm = 0.5 mm, σ_xy in voxels is `σ_xy_um /
500`. For Mars 5 Ultra default σ_xy = 8 µm, σ_xy_voxels = 0.016 ⇒
kernel radius 1, centre weight ≈ 1.0, side weight ≈ exp(-1953) ≈ 0. XY
convolution collapses to identity at default resolution. **XY fidelity
is gated on t2f5** (voxel resolution decoupling).

At default layer_height_um = 50 µm, σ_z in LAYERS is `σ_z_um / 50`.
For Mars 5 Ultra default σ_z = 40 µm, σ_z_layers = 0.8 ⇒ kernel
radius ⌈2.4⌉ = 3 ⇒ 7-tap kernel. **Z-direction crosstalk IS observable
at default resolution.** This is the dominant t2f2 effect at v1 voxel
resolution.

## Prior art

The Gaussian-beam-superposition model is established in the
photopolymer-simulation literature:

> Wei et al., "Voxel Design of Grayscale DLP 3D-Printed Soft Robots",
> *Adv. Materials Technologies* 2024 / PMC11267290.

They write the per-pixel intensity as `I(x,y) = I₀ exp[-2((x-x₀)² +
(y-y₀)²)/ω₀²]` where ω₀ is the waist radius at the I₀/e² intensity
level. The 2D separable Gaussian convolution we apply is mathematically
equivalent to summing such Gaussian beams over all source pixels.

**Conversion between conventions: ω₀ = 2σ.**

Derivation: the standard Gaussian PDF intensity is `I(r) = I₀
exp(-r²/(2σ²))`. Setting this equal to `I₀/e²`:

```
exp(-r²/(2σ²)) = exp(-2)
⇒ r²/(2σ²) = 2
⇒ r² = 4σ²
⇒ r = 2σ
```

So `ω₀ = 2σ` ⇒ `σ = ω₀/2`. Wei et al.'s reported ω₀ = 30 µm for a
42 µm-pixel-pitch printer corresponds to σ = 15 µm in our convention.
The ratio σ/pixel_pitch ≈ 0.36 generalises to Mars 5 Ultra (19 µm
pixel pitch) → σ_xy ≈ 6.8 µm, closely matching our independent
collimation-geometry estimate of 8 µm.

More accurate beam models (e.g. Rayleigh-range depth expansion
`I(x,y,z) = I₀/[1+(z/z_R)²] × exp[-2r²/(ω₀²[1+(z/z_R)²])]` with
`z_R = πω₀²/λ`) are rejected for v1 because intra-layer Z extent is
much smaller than z_R for typical mSLA optics (cure column ≪ Rayleigh
range, so depth-dependent waist expansion is negligible within one
print layer).

See also:

> Williams et al., "Modelling vat photopolymerization: a comprehensive
> review and perspectives on digital twinning and advancing multi-
> wavelength processes", *Virtual & Physical Prototyping* 2026,
> doi:10.1080/17452759.2026.2632491.

This 2026 review is the gold-standard reference for the broader
vat-photopolymerization modelling landscape. Future calibration work
(Athena II beam-profile fit) should re-anchor σ_xy + σ_z against the
review's framework.

## See also

- `docs/adr/0017-voxel-cure-field-and-photoinitiator-depletion.md` —
  the t2f1 voxel cure path this ADR extends.
- `docs/patterns/post-attenuation-z-conv-on-cure-dose-delta.md` —
  implementation pattern: per-column Z post-conv, single-source
  Beer-Lambert preservation via `compute_column_exposure` refactor.
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — the
  storage anisotropy decision that motivates `σ_z_layers = σ_z_um /
  layer_height_um` (not `/ voxel_size_mm × 1000`).
- `docs/patterns/single-source-arrhenius-helper.md` — pattern reused
  via the refactored `apply_column_exposure` → `compute_column_exposure`
  helper extraction; Beer-Lambert math lives in ONE place.
- `spec/uat/light-crosstalk-3d-gaussian-convolution.md` — 8 UAT
  scenarios documenting expected behaviour.
- `crates/resinsim-core/tests/voxel_cure_crosstalk_integration.rs` —
  5 integration tests, one per runtime regime.
