---
id: KB-187
issue: resinsim
kind: source
date: 2026-07-06
source: https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
---

# Source: Pan et al. — Separation force in constrained-surface projection SL

**Pan, He, Xu, Feinerman, "Study of separation force in constrained surface
projection stereolithography," *Rapid Prototyping Journal* 2017, 23(2):353–361,
doi:10.1108/RPJ-12-2015-0188.** (Full text, open PDF.)

## What it is

The primary source for our separation-force equations and the measured
shape/porousness/speed data. (Pan derives the viscous basis from Bruus 2008 and
cites Liravi 2014 — no explicit "Stefan" reference.)

## Key data

- **Eq. 6:** `F = (3πμV/2h³)·R⁴` (solid disk); verbatim *"F ∝ R⁴, F ∝ 1/h³."*
- **Eq. 13:** `F = (8μV/h³)·(A/L)·∮r(θ)dθ` — force ∝ area/perimeter (A/L).
- Shape table (314 mm²): cylinder A/L 5 → 6.16 N; hexagon 4.8 → 5.87; triangle
  3.9 → 5.45; star 2.58 → 4.9 N.
- Porousness p = 1.00/1.33/1.49/1.56 → force rises ~polynomially (linear if
  through-holes).
- Speed linear (F–V); ~25 N rigid glass vs 0.73 N (4 mm PDMS); build drift
  2 → 20 N as O₂ depletes, held < 5 N by a porous window.

## Cites (key upstream references)

- **Bruus, *Theoretical Microfluidics* (OUP, 2008)** — the squeeze-film /
  viscous-flow basis of Eq. 6 (Pan derives the viscous model from this, not from
  an explicit "Stefan" citation).
- Liravi, Zhou & Das, "Separation force analysis based on cohesive delamination
  model…," SFF Symposium 2014 — the prior separation-force study it extends.
- Huang & Jiang, "On-line force monitoring of platform ascending rapid
  prototyping system," *J. Mater. Process. Technol.* 2005,
  doi:10.1016/j.jmatprotec.2004.05.015 — first in-situ SL pull-force measurement.
- Tumbleston et al. 2015 (KB-119); Jacobs 1992; Melchels, Feijen & Grijpma,
  *Biomaterials* 2010, doi:10.1016/j.biomaterials.2010.04.050.

## Used by

KB-185 (two-regime geometry), KB-186 (verbatim equations + tables).

## Link

https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf
· doi:10.1108/RPJ-12-2015-0188
