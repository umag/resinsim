---
id: KB-157
issue: resinsim
kind: source
date: 2026-07-06
source: http://web.mit.edu/doylegroup/pubs/Macromolecules_Dendukuri_08.pdf
---

# Source: Dendukuri — Modeling of Oxygen-Inhibited Free Radical Photopolymerization

**Dendukuri, Panda, Haghgooie, Kim, Hatton, Doyle, "Modeling of Oxygen-Inhibited
Free Radical Photopolymerization in a PDMS Microfluidic Device," *Macromolecules*
2008, 41, 8547–8556, doi:10.1021/ma801219w.** (Full text via MIT mirror.)

## What it is

The primary quantitative model of O₂-inhibited photopolymerization — the source
of our rate constants and the Damköhler induction-scaling laws.

## Key data

- Table 2 constants: k_p = 25, k_O = 5×10⁵, k_i = 2520 m³/(mol·s); φ = 0.6;
  ε = 1.6 m³/(mol·m); [O₂]_eqb = 1.5 mol/m³ (1.5×10⁻³ M) ⇒ **k_O/k_p ≈ 2×10⁴**.
- Damköhler scaling: `τ_i ≈ π/(4Da)` (∝ Da⁻¹); `δ_{i,c} ∝ Da⁻¹ᐟ²`; cure onset
  requires **Da ≥ 4** (Eq. 17).
- [O₂] must fall ≥3 orders of magnitude (≲0.1%) before propagation competes;
  gelation at ξ_c = 0.98 (~2% conversion).

## Cites (source papers for the Table 2 constants)

- **k_O:** Decker & Jenkins, *Macromolecules* 1985, doi:10.1021/ma00148a034.
- **k_p:** Kızılel, Pérez-Luna & Teymour, *Macromol. Theory Simul.* 2006,
  doi:10.1002/mats.200600030.
- **[O₂]_eqb:** Goodner & Bowman, *Chem. Eng. Sci.* 2002,
  doi:10.1016/S0009-2509(01)00287-1.
- **D_O:** Lin & Freeman, "Gas Permeation and Diffusion in Cross-Linked
  Poly(ethylene glycol diacrylate)," *Macromolecules* 2006,
  doi:10.1021/ma051686o — note this gives **D_O = 2.84×10⁻¹¹ m²/s** (O₂ in
  PEG-DA), a *different* value/lineage from CLIP's 1.08×10⁻¹⁰ (KB-158).

## Used by

KB-154 (induction scaling), KB-155 (constants + Damköhler derivation).

## Link

http://web.mit.edu/doylegroup/pubs/Macromolecules_Dendukuri_08.pdf ·
doi:10.1021/ma801219w
