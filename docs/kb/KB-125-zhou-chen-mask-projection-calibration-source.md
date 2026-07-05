---
id: KB-125
issue: resinsim
kind: source
date: 2026-07-06
source: https://repositories.lib.utexas.edu/handle/2152/88371
---

# Source: Zhou & Chen — Calibrating Large-area Mask Projection Stereolithography

**Zhou & Chen, "Calibrating Large-area Mask Projection Stereolithography for Its
Accuracy and Resolution Improvements," *Solid Freeform Fabrication Symposium*
2009 (USC).**

## What it is

The multi-pixel energy-summation calibration model — the middle link between Sun
2005 and Kang 2012 in the light-crosstalk lineage.

## Key data

- Light energy at each location is the **superposition of contributions from
  multiple neighbouring pixels** — the explicit statement that adjacent pixels'
  beams overlap and share dose.
- Provides the pixel-blur calibration method (grayscale/energy mapping) for
  accuracy and resolution improvement in mask-projection SL.

## Cites (key upstream references)

- Bertsch, Jézéquel & André 1997, doi:10.1016/S1010-6030(96)04585-6; Chatwin et
  al., "UV microstereolithography system that uses SLM technology," *Appl. Opt.*
  1998, doi:10.1364/AO.37.007514.
- Sun et al. 2005 (KB-124), doi:10.1016/j.sna.2004.12.011; Lu et al., DMD-based
  tissue scaffolds, *J. Biomed. Mater. Res. A* 2006, doi:10.1002/jbm.a.30601.
- Limaye & Rosen, "Process planning for mask projection stereolithography,"
  *Rapid Prototyp. J.* 2007, doi:10.1108/13552540710776151.
- Note: its calibration is empirical/geometric — it does **not** cite Jacobs or
  the working curve.

## Used by

KB-122 (multi-pixel dose summation); ADR-0018 (Gaussian-superposition prior art).

## Link

Search "Zhou Chen Calibrating Large-area Mask Projection Stereolithography 2009
SFF" (SFF Symposium proceedings / Semantic Scholar).
