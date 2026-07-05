---
id: KB-124
issue: resinsim
kind: source
date: 2026-07-06
source: https://www.sciencedirect.com/science/article/abs/pii/S0924424704008672
---

# Source: Sun et al. — Projection micro-stereolithography Gaussian beam model

**Sun, Fang, Wu & Zhang, "Projection micro-stereolithography using digital
micro-mirror dynamic mask," *Sensors and Actuators A: Physical* 121 (2005)
113–120.**

## What it is

The foundational projection-SL solidification model that Sun/Kang/Zhou build on —
the origin of the Gaussian per-pixel irradiance treatment.

## Key data

- Each DMD micromirror projects a Gaussian-profile UV spot of radius `w₀`
  (defined at 1/e² of peak intensity).
- Cured geometry follows the Beer–Lambert working curve thresholded at Ec;
  establishes the Gaussian-beam-superposition basis for lateral light spread.

## Cites (key upstream references)

- Jacobs 1992 (working curve `Cd = Dp·ln(E₀/Ec)`); Hecht, *Optics*
  (Addison-Wesley, 1988) — Gaussian/optical PSF basis.
- Bertsch, Jézéquel & André, "microstereophotolithography using a dynamic
  mask-generator technique," *J. Photochem. Photobiol. A* 1997,
  doi:10.1016/S1010-6030(96)04585-6.
- Ikuta & Hirowatari, "Real three-dimensional micro fabrication…," IEEE MEMS
  1993.
- Texas Instruments DMD/DLP micromirror papers (Hornbeck), *TI Tech. J.* 15
  (1998).

## Used by

KB-122 (PSF Gaussian-radius parameter, lineage root).

## Link

https://www.sciencedirect.com/science/article/abs/pii/S0924424704008672
