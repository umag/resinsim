---
id: KB-122
issue: resinsim
kind: mechanism
date: 2026-07-05
source: https://www.alexwhittemore.com/efficacy-of-antialiasing-on-msla-prints/
---

# Lateral light bleed (PSF) and grayscale dose-sharing

## Finding

A single LCD/DLP pixel does not deposit a clean square of UV — it deposits a
**Gaussian-blurred dose footprint (point-spread function)** whose shoulders
reach into neighbouring pixels. Consequently a **sub-threshold grayscale pixel
forms no solid material on its own, but its partial dose adds to an adjacent
full-white pixel's overlapping footprint, pushing that neighbour over the cure
threshold** — "supporting adjacent curing" (dose sharing), not independent
partial voxels.

This directly corroborates **ADR-0018** (3D light-crosstalk Gaussian
convolution). Practitioner experience building grayscale antialiasing ("3DAA")
pipelines (reported 2026-07-05) is that "a significant portion of the grayscale
does not mechanically cure into anything on the model, but it *supports* the
adjacent curing," and that the pixel-bleed intuition is "largely correct."

## Mechanism

Cured geometry = the **PSF-blurred dose field thresholded at the critical
exposure Ec**:

```
E(x, y) = ( ideal_mask · grayscale_weight ) ⊛ PSF        (dose field)
solid  ⇔ E(x, y) ≥ Ec                                     (Jacobs threshold)
Cd     = Dp · ln(E₀ / Ec)                                 (cure depth, KB-100/103)
```

- The **PSF** combines LCD/lens source spread + diffraction + volumetric resin
  scatter. Modelled as Gaussian to first order (Kang, Park & Cho 2012; lineage
  Sun et al. 2005 → Zhou & Chen 2009).
- A grayscale pixel emits a **fraction of full dose** (LCD transmittance / DMD
  PWM). Below the threshold it gels nothing on its own; its dose only matters
  through the **overlap** with a neighbour's PSF, letting cured material grow
  laterally off the adjacent fully-cured voxel (sub-pixel rendering).
- Grayscale AA therefore smooths **XY** stair-stepping but does **not** touch
  **Z** layer lines (a Z-discretisation grayscale XY blending cannot reach).

## Quantitative anchors

| Quantity | Value | Source |
|---|---|---|
| PSF σ (lateral blur) | order of the pixel pitch — tens of µm (Gaussian radius `w₀`/`u₀`) | Sun 2005 / Kang 2012; ADR-0018 |
| Wei et al. beam waist ω₀ | 30 µm @ 42 µm pitch → σ = 15 µm; σ/pitch ≈ 0.36 | ADR-0018 / PMC11267290 |
| Penetration depth Dp | ~40–200 µm (clear/light resins @405 nm; pigmented lower) | NIST PMC5828039 |
| Critical exposure Ec | ~1–20 mJ/cm² @405 nm (PR48 6.3; ~3× lower than 365 nm) | NIST PMC5828039 |
| Grayscale usable range | imperfections at 190/255; below ~170 pixels stop *bonding*; below ~160 void (~3 s exposure) — only upper ~½ usable | UVtools |
| Ember grayscale resolution | 32 gray steps → ~1.5 µm cure-depth/step → 3.6° draft within one 50 µm pixel | 3ders (Greene) |
| Slicer AA controls | ChiTuBox grey 0–8, blur 2–8; typical start grey 3 + blur 2 | Liqcreate |

## Implication for resinsim

1. **Validates ADR-0018.** The empirical XY Gaussian pre-convolution
   (`crosstalk_sigma_xy_um`) IS the "pixel bleed supports adjacent curing"
   mechanism: off-mask/grayscale dose lands first (Stage 1), then Beer-Lambert
   + the Ec threshold decide what becomes solid. resinsim's regime BA/DD
   already models this phenomenon.
2. **Known gap = XY fidelity is gated on t2f5.** At the default 0.5 mm mask
   voxel, `σ_xy_voxels ≈ 0.016` collapses the XY kernel to identity (ADR-0018
   "Scaling caveat"). The mechanism is *representable* but not *resolved* until
   voxel-resolution decoupling.
3. **To model grayscale/3DAA exposure explicitly** (not just crosstalk): weight
   the per-pixel intensity grid by `gray/255` **before** the σ_xy convolution.
   That is a concrete future input — a grayscale-aware exposure mask — and would
   let resinsim reproduce the Ember 32-gray draft-angle result and evaluate
   the grayscale/greyscale-halo support-curing ideas (see KB-154).
4. **Sub-threshold ≠ wasted, ≠ solid.** Below-Ec dose is neither cured material
   nor irrelevant; it is dose that raises a neighbour over Ec. Any grayscale
   support-curing feature must model it as *dose contributed to neighbours*, not
   as partial material at the grey pixel.

## Caveats

- Real scatter is forward-peaked (g ≈ 0.5–0.9), not Gaussian; the Gaussian σ is
  a second-moment approximation (ADR-0018 limitation).
- Dp/Ec are method-sensitive: an interlaboratory study found Dp varied up to
  ~7× and Ec up to ~70× between labs — use order-of-magnitude, calibrate
  per-resin.

## Sources

- Efficacy of Antialiasing on MSLA Prints (dose-sharing / "grows off adjacent
  cured surface") — https://www.alexwhittemore.com/efficacy-of-antialiasing-on-msla-prints/
- Anti-aliasing (UVtools wiki — gray ladders, bonding thresholds, "half pixel") —
  https://github.com/sn4k3/UVtools/wiki/Anti-aliasing
- Kang, Park & Cho, "A pixel based solidification model for projection based
  stereolithography," *Sensors & Actuators A* 178 (2012) 223 —
  https://www.sciencedirect.com/science/article/abs/pii/S0924424712000404
- Sun, Fang, Wu & Zhang, micromirror Gaussian model, *Sensors & Actuators A*
  121 (2005) 113 — https://www.sciencedirect.com/science/article/abs/pii/S0924424704008672
- Zhou & Chen, "Calibrating Large-area Mask Projection Stereolithography…," SFF
  Symposium 2009 (multi-pixel energy summation).
- Measuring UV Curing Parameters of Commercial Photopolymers (NIST) —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC5828039/
- Autodesk grayscale sub-pixel DLP (32 grays, 1.5 µm/step, 3.6°) —
  https://www.3ders.org/articles/20160815-autodesk-offers-grayscale-trick-for-dlp-3d-printing-at-sub-pixel-resolution.html
- Explained & tested: Anti-Aliasing & Blur in resin printing (Liqcreate) —
  https://www.liqcreate.com/supportarticles/explained-tested-anti-aliasing-aa-and-blur-in-resin-3d-printing/

## See also

- ADR-0018 — 3D light-crosstalk via XY pre-conv + Z post-conv (this KB is its
  external-evidence corroboration).
- KB-100 / KB-101 / KB-103 — Dp/Ec and Beer-Lambert primitives.
- KB-160 — photoinitiator depletion (couples to dose).
- KB-154 — oxygen induction dose (why a below-material grey flood still does
  chemical work around supports).
