---
id: KB-159
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/
---

# Source: Modeling Kinetics, Curing Depth & Efficacy of Radical Photopolymerization

**Lin, Liu, Chen & Cheng, "Modeling the Kinetics, Curing Depth, and Efficacy of
Radical-Mediated Photopolymerization: The Role of Oxygen Inhibition, Viscosity,
and Dynamic Light Intensity," *Frontiers in Chemistry* 2019, 7:760,
doi:10.3389/fchem.2019.00760.**

## What it is

A kinetic model giving a closed-form oxygen-induction-time formula — the source
of our `T_ID` expression.

## Key data

- **Induction time (Eq. 27): `T_ID = π·k·Y₀ / (4·k′·B)`**, `B = b·I·C` (light ×
  photoinitiator). Y₀ = initial dissolved [O₂]. Defined as the time until
  [O₂] = 0.
- Shares the π/4 prefactor with Dendukuri's `τ_i ≈ π/(4Da)` — two models
  converge on `T_ID ∝ [O₂]₀ / (photoinitiator × intensity)`.
- External O₂ supply 0–7×10⁻⁶ mM/s measurably reduces cure depth; higher
  intensity at fixed dose → higher conversion (shorter O₂ window).

## Cites (key upstream references)

- O'Brien & Bowman, "Modeling the Effect of Oxygen on Photopolymerization
  Kinetics," *Macromol. Theory Simul.* 2006, doi:10.1002/mats.200500056; Cramer,
  O'Brien & Bowman, *Polymer* 2008, doi:10.1016/j.polymer.2008.08.051.
- Dendukuri et al. 2008 (KB-157), doi:10.1021/ma801219w.
- Chen, Pathreeker, Biria & Hosein, micropillar O₂-inhibition study,
  *Macromolecules* 2017, doi:10.1021/acs.macromol.7b01274.
- Alvankarian & Majlis, "Exploiting the Oxygen Inhibitory Effect…," *PLoS ONE*
  2015, doi:10.1371/journal.pone.0119658.

## Used by

KB-154 (induction-time form), KB-155 (`T_ID` formula).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/
