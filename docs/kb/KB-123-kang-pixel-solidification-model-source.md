---
id: KB-123
issue: resinsim
kind: source
date: 2026-07-06
source: https://www.sciencedirect.com/science/article/abs/pii/S0924424712000404
---

# Source: Kang, Park & Cho — A pixel-based solidification model for projection SL

**Kang, Park & Cho, "A pixel based solidification model for projection based
stereolithography technology," *Sensors and Actuators A: Physical* 178 (2012)
223–229.** (Often mis-cited as "Zhou & Chen 2012" — this DOI is Kang/Park/Cho.)

## What it is

The primary-source solidification model backing our "PSF-blurred dose thresholded
at Ec" statement.

## Key data

- A single pixel's irradiance is modelled as a **point-spread function, Gaussian
  as a first-order approximation** — half-width `u₀` plus a directional-variation
  parameter `a`, both fit experimentally.
- Total exposure at a point = **sum of neighbouring pixels' Gaussians**; the
  cured line width is the convolution of the pixel pattern with the Gaussian,
  scaled by cure-depth/penetration-depth ratio.
- Solidifies where accumulated exposure ≥ critical exposure Ec.

## Cites (key upstream references)

- Sun et al. 2005 (KB-124), doi:10.1016/j.sna.2004.12.011; Jacobs 1992 (working
  curve); Hecht, *Optics* (Addison-Wesley, 1988) — Gaussian/optical PSF basis.
- Choi et al., "Cure depth control for complex 3D microstructure fabrication…,"
  *Rapid Prototyp. J.* 2009, doi:10.1108/13552540910925072.
- Zhou, Chen & Waltz, "Optimized mask image projection…," *J. Manuf. Sci. Eng.*
  2009, doi:10.1115/1.4000416.
- Jariwala et al., "Modeling effects of oxygen inhibition in mask-based
  stereolithography," *Rapid Prototyp. J.* 2011, doi:10.1108/13552541111124734.

## Used by

KB-122 (PSF / dose-sharing mechanism); corroborates ADR-0018.

## Link

https://www.sciencedirect.com/science/article/abs/pii/S0924424712000404
