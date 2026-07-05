---
id: KB-155
issue: resinsim
kind: formula
date: 2026-07-05
source: http://web.mit.edu/doylegroup/pubs/Macromolecules_Dendukuri_08.pdf
---

# Oxygen-inhibition kinetics constants and induction-dose formulas

Verified rate constants, solubility, and induction-dose scaling underpinning
KB-154. Values read from the primary-source tables (Dendukuri 2008, MIT mirror).

## Rate constants (Dendukuri 2008, Table 2)

| Constant | Value | Note |
|---|---|---|
| Propagation `k_p` (R•+monomer) | 25 m³/(mol·s) = 2.5×10⁴ M⁻¹s⁻¹ | ref. 29 |
| Inhibition `k_O` (macroradical+O₂) | 5×10⁵ m³/(mol·s) = 5×10⁸ M⁻¹s⁻¹ | ref. 15 (also used by CLIP model) |
| Initiation `k_i` (primary radical+monomer) | 2520 m³/(mol·s) | |
| Initiation quantum yield `φ` | 0.6 | |
| Molar absorptivity `ε` | 1.6 m³/(mol·m) | |

**Ratio `k_O/k_p ≈ 2×10⁴`** (inside the 10⁴–10⁵ band; upper end 5×10⁵ if `k_p`
is a slow monofunctional acrylate ~10³ M⁻¹s⁻¹).

**Critical nuance:** that is a *rate-constant* ratio. Because dissolved O₂ is
scarce (`[O₂]≈1.5×10⁻³ M`) vs monomer (`[M]≈5 M`), the *initial reaction-rate*
ratio is only **`r_O₂/r_p ≈ 10²`**. So O₂ depletes quickly yet still gates cure:
k-ratio ~10⁴–10⁵, net-rate ratio ~10². Do not call it "10⁴–10⁵× faster per
collision."

## Solubility and diffusion

- **Dissolved [O₂]_eqb ≈ 1.5×10⁻³ mol/L** (Dendukuri Table 2, air-saturated;
  Hoyle 2004: *"10⁻² to 10⁻³ M in most photocurable resins"*). Structure-
  dependent across acrylates: **0.59–2.07×10⁻³ mol/L** (Scherzer 2005).
- **≈ 30 ppm by mass** (~4× the ~8 ppm of air-saturated *water* — do not use the
  water figure for resin).
- **D_O₂ ≈ 1.08×10⁻¹⁰ m²/s** (CLIP model, conversion-dependent
  `D = D₀·exp(−0.358/f)`, f = free-volume fraction — O₂ mobility drops as resin
  cures). Literature spread is wide: **3×10⁻¹¹ → 6×10⁻¹⁰ m²/s** — cite 1.08×10⁻¹⁰
  to CLIP specifically, not as universal.

## Onset threshold (Dendukuri)

Verbatim: *"the oxygen concentration has to decrease by 3 orders of magnitude or
more before the termination step starts competing with the inhibition step."*
→ [O₂] must fall to **≲0.1% of air-saturated** before propagation outcompetes
inhibition. Gelation at **ξ_c = 0.98** (only ~2% of double bonds converted).

## Damköhler scaling (Dendukuri, dimensionless)

```
Da  = φ·ε·I₀·[PI]·H² / (D_O·[O₂]_eqb)          (O₂ consumption ÷ O₂ diffusion)
Eq 15:  τ_i ≈ π/(4·Da)                          →  τ_i ∝ Da⁻¹
Eq 16:  δ_{i,c} ∝ Da^{−1/2}                      (inhibition/lubrication layer thickness)
Eq 17:  cure onset requires  Da ≥ 4
```
Consequence: to hold cure behaviour constant as layer/gap thickness `H` changes,
`[PI]·I₀` must scale as `1/H²`. The inhibition-layer thickness is independent of
gap height in dimensional form — the basis for CLIP dead-zone constancy.

## Induction-time analytic formula (Frontiers Chem. 2019, 7:760, Eq. 27)

```
T_ID = π·k·Y₀ / (4·k′·B) ,   B = b·I·C
```
Y₀ = initial dissolved [O₂]; k = rate-constant coupling ratio; k′ = radical–
substrate coupling constant; B = light-driven radical generation (I = intensity,
C = photoinitiator conc.). Shares the **π/4 prefactor** with Dendukuri's
`τ_i ≈ π/(4Da)` — two independent models converge on
**T_ID ∝ [O₂]₀ / (photoinitiator × intensity)**.

## Fraction of Ec attributable to oxygen

Not published as a fixed fraction of the Jacobs critical exposure Ec. The
defensible statement: the entire pre-gel dose delivered during the induction
period is the O₂-consumption dose = `I₀·τ_ind`, and cure requires `Da ≥ 4`
(below which O₂ ingress out-competes radical generation and nothing cures).
Order-of-magnitude induction dose is low single-digit mJ/cm² — same order as
clear-resin Ec (e.g. PR48 Ec = 6.3 mJ/cm² @405 nm, KB-101/100), but pigmented
mSLA resins add optical Ec on top.

## Sources

- Dendukuri et al., "Modeling of Oxygen-Inhibited Free Radical
  Photopolymerization…," *Macromolecules* 2008 (full text) —
  http://web.mit.edu/doylegroup/pubs/Macromolecules_Dendukuri_08.pdf
- Hoyle, "An Overview of Oxygen Inhibition in Photocuring," RadTech 2004 —
  https://radtech.org/proceedings/2004/papers/104.pdf
- Simplified 2D simulation of photopolymerization + O₂ (CLIP; D_O₂) —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
- Radical-mediated photopolymerization kinetics / curing depth, *Frontiers
  Chem.* 2019, 7:760 (T_ID) — https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/
- Scherzer, O₂ solubility in acrylates, *Macromol. Chem. Phys.* 2005 —
  https://onlinelibrary.wiley.com/doi/10.1002/macp.200400300
- Ligon et al., *Chem. Rev.* 2014, 114, 557–589 (paywalled; corroborated above).

## See also

- KB-154 — oxygen-inhibition induction dose (synthesis entry this backs).
- KB-153 — Ec(T) Arrhenius; KB-160 — photoinitiator depletion.
- KB-116 / KB-117 — film-side O₂ (release layer, CLIP dead zone).
