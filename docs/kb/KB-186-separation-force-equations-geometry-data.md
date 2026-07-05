---
id: KB-186
issue: resinsim
kind: formula
date: 2026-07-05
source: https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
---

# Separation-force equations and measured geometry data (Pan 2017, Kendall 1975)

Verbatim governing equations and measured tables underpinning KB-185. All
transcribed from the primary-source page images (not lossy text extraction).

## Stefan squeeze-film — solid circular (Pan et al. 2017, Eq. 6)

Derivation chain (Pan §3.1), verbatim:
```
Eq (4):  P = −(3μV/h³)·r² + C
Eq (5):  P = −(3μV/h³)·r² + (3μV/h³)·R²
Eq (6):  F = ∫₀ᴿ 2πr·P dr = (3π·μ·V / 2·h³)·R⁴
```
Pan §3.1, verbatim: *"Note that the separation force F is nonlinear with R and
h: F ∝ R⁴, F ∝ 1/h³."* This is **Stefan's viscous-adhesion law** for pulling
parallel discs apart through a viscous film (Wikipedia: `F = 3πηR⁴/(2h³)·dh/dt`,
with `V = dh/dt`). μ = resin viscosity, V = separation velocity, h = dead-zone
gap, R = part radius. The review PMC10302688 renumbers this as its Eq. 4.

## Irregular solid — area/perimeter form (Pan Eq. 13)

```
Eq (11): P = (12μV/h³)·(A/L)·λ²·r(θ) + C
Eq (12): P = (12μV/h³)·(A/L)·r(θ)·(1 − λ²)
Eq (13): F = (8·μ·V/h³)·(A/L)·∫₀²π r(θ) dθ
```
Pan §5.1, verbatim: *"The force peak increases linearly with the area/perimeter
ratio of the print geometry, agreeing well with the analytical model in
equation (13)."* (Note: Pan's Eq. 9 prints perimeter as `L = ∫₀²π θ·r(θ)dθ`, a
probable typesetting error — perimeter should be `∫√(r²+(dr/dθ)²)dθ` — but it
does not affect Eq. 13.)

## Kendall thin-film peel (1975)

Film thickness d, modulus E, width b, peel angle θ, force F, fracture energy R
(per unit area):
```
Eq (2):  (F/b)²·(1/2dE) + (F/b)·(1 − cosθ) − R = 0        (quadratic in F/b)
Eq (5):  F/b = R/(1 − cosθ)          → F = b·R/(1 − cosθ)   (inextensible / high-angle limit)
Eq (6):  F/b = (2·E·d·R)^{1/2}                              (small-angle, elastic-dominant limit)
```
Writing `f = F/b`, Eq. 2 is exactly `f(1−cosθ) + f²/(2Ed) = R`. In KB-185's
notation `G = R`. Kendall stresses R is a **fracture energy** (rises with crack
speed), NOT a thermodynamic work of adhesion. The elastic term matters only
"for materials which can support stresses approaching the elastic modulus…and
for very small peel angles" (Abstract). The Eq. 6 small-angle asymptote is why
very-low-angle flexible-film peel collapses the required force.

## Measured shape table (Pan Fig. 9 — 314 mm² solid cross-sections, PDMS interface)

| Shape | A/L | Peak separation force |
|---|---|---|
| Cylinder | 5.0 | 6.16 N |
| Hexagon | 4.8 | 5.87 N |
| Triangle | 3.9 | 5.45 N |
| Star | 2.58 | 4.9 N |

Verbatim: *"the maximum separation forces are found to be 6.16, 5.87, 5.45 and
4.9 N for cylinder, hexagon, triangle and star structures, respectively."*
Pull speed for this figure is not stated (other tests ran 1.56 mm/s); interface
is PDMS-coated throughout.

## Porousness (Pan Fig. 10/11)

`p` = bounding-box area ÷ printing (lit) area. Tested p = **1.00, 1.33, 1.49,
1.56** (a = solid square, d = most porous). Force read graphically ~1.0 N
(p=1.0) rising to ~1.7 N (p=1.56). Verbatim: *"the separation force increases
with the degree of porousness with an approximately polynomial relationship"*;
but *"If no deep holes are formed by the pervious layers…the relationship could
be approximated to be linear."* The curve is convex — the two middle points sit
below a straight-line interpolation.

## Speed and interface compliance (Pan §3.2, Fig. 6)

- Five-speed sweep (0.63 / 1.00 / 1.56 / 2.00 / 3.00 mm/s) → peaks
  0.72 / 0.98 / 1.18 / 1.54 / 2.44 N on 2 mm PDMS (0.35 / 0.50 / 0.81 / 1.01 /
  1.40 N on 4 mm PDMS) — clean **linear F–V**, confirming Eq. 6's V-linearity.
- Compliance: at 1.56 mm/s, peak fell **5.43 → 1.0 → 0.73 N** for 1 / 2 / 4 mm
  PDMS, versus **~25 N with rigid glass (no PDMS)** — a ~25×+ rigid/flexible gap
  inside Pan alone.
- Build drift: a 20 mm cylinder's force climbed **2 N → 20 N** conventionally as
  O₂ depleted; a porous air-permeable window held it **< 5 N** throughout.

## Cohesive-zone fracture (constrained-surface SL FEA)

- Critical energy-release-rate **Gc ≈ 1.862×10⁻³ J/mm² (≈ 1862 J/m²)** for
  bilinear traction–separation damage evolution.
- Crack path, verbatim: *"the interface breaks like a crack propagation, and
  the separation initiates from periphery to center."*

## Sources

- Pan et al., "Study of separation force in constrained surface projection
  stereolithography," *Rapid Prototyping Journal* 2017 (full text) —
  https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
  · DOI https://doi.org/10.1108/RPJ-12-2015-0188
- Kendall, "Thin-film peeling — the elastic term," *J. Phys. D* 1975 (full
  text) — http://bdml.stanford.edu/twiki/pub/Rise/AdhesionModels/Kendall75peeling.pdf
- A Review of Critical Issues in High-Speed Vat Photopolymerization, *Polymers*
  2023 — https://pmc.ncbi.nlm.nih.gov/articles/PMC10302688/
- Hu et al., Rotation-Assisted Separation Model, 2023 (Gc, CZM) —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC10049864/
- Stefan adhesion — https://en.wikipedia.org/wiki/Stefan_adhesion

## See also

- KB-185 — peel-front geometry vs area (the synthesis entry this backs).
- KB-181 — peel force vs area dataset; KB-184 — suction quantification.
- KB-117 — separation-force reduction methods + CLIP dead-zone dataset.
- KB-114 — resinsim peel force formula (the `σ_peel·A·f(v)` term to refactor).
