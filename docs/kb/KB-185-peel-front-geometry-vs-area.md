---
id: KB-185
issue: resinsim
kind: formula
date: 2026-07-05
source: https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
---

# Peel-front geometry vs. area: Stefan suction vs. Kendall peel

## Finding

Separation force is **not a pure function of lit cross-section area**. It is
governed by two distinct mechanisms that scale with geometry in fundamentally
different ways, and which one dominates depends on whether the interface is
rigid or a compliant film:

- **Rigid / suction (Stefan squeeze-film):** `F ∝ R⁴ ∝ area²`, `∝ h⁻³`, `∝ V`.
- **Compliant-film peel (Kendall):** `F ∝ peel-front width b` (a length /
  perimeter), essentially **independent of bonded area**.

Real FEP printers live mostly in the **peel** regime with a suction
contribution — so **shape, not just area, drives force**. This is the precise
content behind a practitioner discussion (2026-07-05): the area↔force
correlation (KB-181) "is true of 'square' cross-sections — any cross-section
with a low aspect ratio," and a thin-walled hollow separates at far lower force
than a solid of the same footprint.

## The two regimes

**(A) Stefan / squeeze-film (rigid-like interface).** Fresh resin must flow
radially inward across the thin gap `h` to fill the widening void:

```
F = (3π·μ·V / 2h³) · R⁴          (solid circular cross-section)
```
- `μ` viscosity, `V` pull speed, `h` dead-zone gap, `R` radius.
- Super-linear in area (`R⁴ ∝ area²`), brutal in gap (`h⁻³`). Suction is worst
  at the **centre**; a long perimeter vents/refills and relieves it.

**(B) Kendall thin-film peel (compliant FEP).** The film flexes, a crack
initiates at an edge and propagates inward (cohesive-zone FEM: "fracture
initiating from the periphery… spreading to the centre"):

```
F = b · G / (1 − cosθ)           (inextensible-film limit)
```
- `b` peel-front width, `G` adhesion energy/area, `θ` peel angle.
- Force tracks **peel-front length / perimeter and geometry, NOT bonded area**.

## Aspect ratio — thin ≪ compact at equal area

Both regimes rank long/thin below compact for the same lit area. Suction form
(Pan et al. 2017) reduces to an **area-to-perimeter** dependence:

```
F = (8·μ·V / h³) · (A/L) · ∮ r(θ) dθ     → F rises linearly with A/L
```

Four shapes, identical **314 mm²**, DLP/PDMS:

| Shape | A/L | Peak separation force |
|---|---|---|
| Cylinder (compact) | 5.0 | **6.16 N** |
| Hexagon | 4.8 | 5.87 N |
| Triangle | 3.9 | 5.45 N |
| Star (thin arms) | 2.58 | **4.9 N** |

Same area, ~26% more force on the compact circle. In the peel regime a thin
strip presents a narrow *instantaneous* crack front `b` → low F. So
`F ∝ σ_peel·A` is the **low-aspect-ratio / compact-shape limit**; it breaks for
high aspect ratio.

## Hollowing, thin walls, drain holes

The "counterintuitive" cylinder test (solid vs. hollowed, same surface area)
resolves once you fix *what is held constant*:

- **Hollowing (thin walls, keep exterior)** removes cured cross-sectional area
  per layer → shorter peel front → **far lower force**, even though the
  footprint is larger — **provided drain/vent holes prevent a sealed cavity.**
  Typical walls: 1 mm (small) / 2 mm (medium) / 3 mm (large).
- **A sealed hollow = suction cup ("cupping"):** the trapped low-pressure
  cavity converts a cheap peel back into an expensive area-suction pull
  (`F ∝ R⁴`). Fix: ≥2 drain holes, ≥3.5 mm diameter, ≥1 placed high.
- **Pan et al. porousness caveat:** at **fixed lit area**, perforating a slab
  *increases* force (enclosed holes still generate suction over their
  footprint) — force rises ~polynomially with porousness `p`. Hollowing helps
  because it removes *area*; it backfires only if the void is sealed.

## Scaling table

| Model | Relationship | Source |
|---|---|---|
| Stefan squeeze-film (solid disk) | F ∝ R⁴, h⁻³, V¹ (`F=3πμV/2h³·R⁴`) | Pan Eq. 6; Stefan |
| Irregular solid suction | F ∝ A/L (area/perimeter) (`F=8μV/h³·(A/L)·∮r dθ`) | Pan Eq. 13 |
| Porous, fixed lit area | F ↑ ~polynomially with porousness p (linear if through-holes) | Pan §5.2 |
| Kendall thin-film peel | `F=b·G/(1−cosθ)`; small-angle `F/b=(2EdG)^½`; ~area-independent | Kendall 1975 |

Rigid-vs-flexible ratios are two *different* comparisons — rigid-vs-flexible
~100× (ACS 2025), quartz-vs-PDMS ~10×; **Teflon/FEP is ~10× *higher* than soft
PDMS** (stiffer ⇒ peels worse, not a reduction). Verbatim equations and the
shape / porousness / speed tables + Gc → **KB-186**.

Force levers (all reduce separation force): speed linear in V; **rotation 14%
measured / up to 44% simulated** (Hu 2023); **tilting ~20%** (a distinct
mechanism); vibration ~60%; piezo ~75%; two-channel PDMS to ~4–5% of baseline.
Full table in KB-117.

## Implication for resinsim

`PeelForceCalculator` currently computes `F_peel = σ_peel · A · f(v)` — the
compact-shape / low-aspect-ratio approximation that also **conflates peel and
suction into one area term**. Roadmap:

- **Tier-1 (cheap):** add a **shape factor from A/L** (area÷perimeter),
  computable per layer once `LayerMask` exposes a perimeter alongside
  `solid_area_mm2`; modulate `σ_peel` by it. Captures the aspect-ratio
  point at low cost.
- **Tier-2:** **split the model** into a peel-front term (Kendall,
  ∝ perimeter·G) + a suction term (Stefan/ΔP·A for sealed cavities — the
  `cavity_detector` topology pass already finds these, KB-184). This also
  aligns with **KB-115**: the base-layer force spike is
  suction/base-adhesion-dominated, not area-driven — a natural fit for the
  split.
- **Validation:** the proposed **solid-vs-hollow cylinder pair at equal surface
  area** is a clean UAT; the Athena FSS (channel T=6) measures it directly. The
  companion experiment is an **equal-area / swept-aspect-ratio** print
  (cylinder → star) reproducing the 6.16 N → 4.9 N trend.

## Caveats

- Pan Eq. 6/13 and the full Kendall algebra are now **confirmed verbatim** from
  the primaries (KB-186); the ACS-2025 100× rigid/flexible ratio and Kendall
  ±10–15% fit-error remain corroborated-not-fully-read (publisher paywall).
- Numbers above are PDMS/DLP-interface measurements; absolute values won't
  transfer to Athena II (nFEP/linear-release) — use the *scaling laws*, fit
  magnitudes per printer.

## Sources

- Pan et al., "Study of separation force in constrained surface projection
  stereolithography," *Rapid Prototyping Journal* 2017 —
  https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
- Kendall, "Thin-film peeling — the elastic term," *J. Phys. D* 1975 —
  https://iopscience.iop.org/article/10.1088/0022-3727/8/13/005
- A Review of Critical Issues in High-Speed Vat Photopolymerization, *Polymers*
  2023 — https://pmc.ncbi.nlm.nih.gov/articles/PMC10302688/
- Hu et al., "Rotation-Assisted Separation Model of Constrained-Surface
  Stereolithography," 2023 — https://pmc.ncbi.nlm.nih.gov/articles/PMC10049864/
- Impact of Interface Flexibility on Separation Force in LCD VPP, *ACS Appl.
  Polym. Mater.* 2025 — https://pubs.acs.org/doi/10.1021/acsapm.5c00167
- Stefan adhesion (squeeze-film law) — https://en.wikipedia.org/wiki/Stefan_adhesion
- Prusa KB, Hollowing (walls, ≥3.5 mm drain holes, cupping) —
  https://help.prusa3d.com/article/hollowing_117285

## See also

- KB-186 — verbatim Stefan/Kendall equations + measured shape/porousness/speed
  tables (the data behind this entry).
- KB-181 — peel force vs. area dataset (the correlation this entry qualifies).
- KB-115 — first-layer base-adhesion gap (suction/base-dominated base layers).
- KB-184 / KB-173 — suction-force quantification and suction-cup test.
- KB-114 — peel force formula (`F_peel = σ_peel·A·f(v)` — the term to refactor).
- KB-116 — oxygen-inhibited release layer (the film-state term, distinct from
  this geometry term).
