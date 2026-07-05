---
id: KB-159
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/
---

# Source: Modeling Kinetics, Curing Depth & Efficacy of Radical Photopolymerization

**"Modeling the Kinetics, Curing Depth, and Efficacy of Radical-Mediated
Photopolymerization: The Role of Oxygen Inhibition, Viscosity, and Dynamic Light
Intensity," *Frontiers in Chemistry* 2019, 7:760.**

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

## Used by

KB-154 (induction-time form), KB-155 (`T_ID` formula).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/
