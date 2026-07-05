---
id: KB-126
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC11267290/
---

# Source: Wei et al. — Voxel Design of Grayscale DLP 3D-Printed Soft Robots

**Wei et al., "Voxel Design of Grayscale DLP 3D-Printed Soft Robots," *Advanced
Materials Technologies* 2024 (PMC11267290).**

## What it is

The literature anchor for ADR-0018's Gaussian-beam-superposition crosstalk model
and the σ_xy default calibration.

## Key data

- Per-pixel intensity: `I(x,y) = I₀·exp[−2((x−x₀)²+(y−y₀)²)/ω₀²]` (ω₀ = beam
  waist at I₀/e²).
- Reported **ω₀ = 30 µm at 42 µm pixel pitch** ⇒ σ = ω₀/2 = 15 µm; ratio
  **σ/pixel_pitch ≈ 0.36** (generalises to other printers).
- Grayscale voxel control produces functionally-graded cure / stiffness.

## Cites (key upstream references)

- Emami & Rosen, "Modeling of light field effect in deep vat polymerization for
  grayscale lithography," *Additive Manufacturing* 2020,
  doi:10.1016/j.addma.2020.101595 — the Gaussian-beam light-field model.
- Kuang et al., "Grayscale digital light processing 3D printing for highly
  functionally graded materials," *Science Advances* 2019,
  doi:10.1126/sciadv.aav5790.
- Han, Lu, Chester & Lee, projection micro-SL hydrogel, *Sci. Rep.* 2018,
  doi:10.1038/s41598-018-20385-2.
- Wang et al., Gaussian-beam + reaction–diffusion grayscale model, *Materials &
  Design* 2021, doi:10.1016/j.matdes.2020.109381.

## Used by

ADR-0018 (crosstalk σ_xy derivation), KB-122 (beam-waist anchor).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC11267290/
