---
id: KB-158
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
---

# Source: A Simplified 2D Simulation of Photopolymerization + O₂ Diffusion (CLIP)

**"A Simplified 2D Numerical Simulation of Photopolymerization Kinetics and
Oxygen Diffusion–Reaction for the Continuous Liquid Interface Production (CLIP)
System."**

## What it is

A reaction-diffusion CLIP model — the source of our oxygen diffusivity value and
the conversion-dependent diffusion form.

## Key data

- **D_O₂ = 1.08×10⁻¹⁰ m²/s** for oxygen in monomer (traces to Taki et al. 2017),
  conversion-dependent `D = D₀·exp(−α/f)`, α = 0.358, f = free-volume fraction —
  O₂ mobility drops as the resin cures.
- Photoinitiator concentration [PI] = 54.55 mol/m³; rate constants consistent
  with Dendukuri (k_O ≈ 5×10⁸ M⁻¹s⁻¹).
- Dead zone stays constant in thickness during steady printing while UV
  intensity and O₂ permeation are held constant.

## Used by

KB-154/155 (D_O₂), KB-116/117 (dead-zone constancy).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
