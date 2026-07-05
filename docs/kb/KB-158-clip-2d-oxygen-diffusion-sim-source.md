---
id: KB-158
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
---

# Source: A Simplified 2D Simulation of Photopolymerization + O₂ Diffusion (CLIP)

**Taki, K., "A Simplified 2D Numerical Simulation of Photopolymerization Kinetics
and Oxygen Diffusion–Reaction for the Continuous Liquid Interface Production
(CLIP) System," *Polymers (Basel)* 2020, 12(4):875, doi:10.3390/polym12040875
(PMC7240730).**

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

## Cites (parameter provenance)

- **D_O = 1.08×10⁻¹⁰ m²/s traces to** Taki, Watanabe, Tanabe, Ito & Ohshima,
  "Oxygen concentration and conversion distributions in a layer-by-layer UV-cured
  film…," *Chem. Eng. Sci.* 2017, doi:10.1016/j.ces.2016.10.050 (diurethane
  dimethacrylate / Irgacure-184 parameter set — adopted inline, not footnoted).
- Kinetics/rate constants: Taki, Watanabe, Ito & Ohshima, "Effect of Oxygen
  Inhibition on the Kinetic Constants of the UV-Radical Photopolymerization…,"
  *Macromolecules* 2014, doi:10.1021/ma402437q.

## Used by

KB-154/155 (D_O₂), KB-116/117 (dead-zone constancy).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
