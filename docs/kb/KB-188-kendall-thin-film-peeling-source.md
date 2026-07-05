---
id: KB-188
issue: resinsim
kind: source
date: 2026-07-06
source: http://bdml.stanford.edu/twiki/pub/Rise/AdhesionModels/Kendall75peeling.pdf
---

# Source: Kendall — Thin-film peeling: the elastic term

**Kendall, "Thin-film peeling—the elastic term," *J. Phys. D: Appl. Phys.* 1975,
8:1449–1452, doi:10.1088/0022-3727/8/13/005.** (Full text.)

## What it is

The foundational thin-film peel-mechanics paper — the source of our compliant-film
peel equation (the regime a flexible FEP release film actually operates in).

## Key data

- **Eq. 2:** `(F/b)²·(1/2dE) + (F/b)·(1−cosθ) − R = 0` (quadratic in F/b).
- **Eq. 5:** `F/b = R/(1−cosθ)` → `F = b·R/(1−cosθ)` (inextensible / high-angle
  limit) — force ∝ peel-front width `b`, ~independent of bonded area.
- **Eq. 6:** `F/b = (2·E·d·R)^½` (small-angle elastic-dominant limit).
- `R` is a **fracture energy** (rises with crack speed), not thermodynamic work
  of adhesion. Elastic term matters near-modulus stresses and very small angles.

## Used by

KB-185 (peel regime), KB-186 (verbatim algebra).

## Link

http://bdml.stanford.edu/twiki/pub/Rise/AdhesionModels/Kendall75peeling.pdf ·
doi:10.1088/0022-3727/8/13/005
