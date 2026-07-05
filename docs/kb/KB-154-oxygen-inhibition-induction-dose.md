---
id: KB-154
issue: resinsim
kind: mechanism
date: 2026-07-05
source: https://pubs.acs.org/doi/10.1021/cr3005197
---

# Oxygen-inhibition induction dose (part of the cure threshold)

## Finding

Before any solid forms, the **dissolved oxygen in the illuminated resin must
first be consumed**. Oxygen's *rate constant* with radicals is ~10⁴–10⁵× larger
than monomer propagation's (`k_O/k_p ≈ 2×10⁴`), converting initiating/
propagating radicals to unreactive peroxyl radicals; net chain growth begins
only once [O₂] falls **≥3 orders of magnitude (to ≲0.1%)** of its air-saturated
value. The UV dose spent during this **induction period is effectively added to
the critical exposure Ec** — it is dose that cures nothing, spent burning off
oxygen.

This is the chemistry invoked in a practitioner discussion (2026-07-05):
"oxygen reacts so much faster and preferentially with the photoinitiators…it
locally must be consumed before resin will start to kick," and it is the same
reason support *tips* (high surface-area-to-volume, more O₂ re-supply) end up
under-cured (~40% tensile strength).

## Mechanism

Two coupled inhibition pathways:
1. **Excited-state quenching** — ground-state (triplet) O₂ quenches the excited
   photoinitiator before it fragments into radicals.
2. **Radical scavenging (dominant)** — R• + O₂ → ROO• (resonance-stabilised,
   ~10³–10⁴× slower to add to a C=C than a carbon radical). Each O₂ molecule
   parks a growing chain.

O₂ out-competes monomer despite being ~1000× less concentrated because its
rate constant is ~10⁴–10⁵× larger. **Nuance:** that is a *rate-constant* ratio
(k_O/k_p ≈ 2×10⁴). Because `[O₂]≈1.5×10⁻³ M ≪ [M]≈5 M`, the *initial reaction-
rate* ratio is only ~10² — which is exactly why O₂ depletes fast yet still gates
cure. (Rate constants + derivation: KB-155.)

**Induction / redistribution scaling:**
- Induction time `τ_i ∝ [O₂]₀ / I` (higher intensity shortens the O₂ window →
  more conversion per unit dose).
- Damköhler scaling (Dendukuri): `τ_i ≈ π/(4·Da)` (∝ Da⁻¹), uncured surface
  film `δ ∝ Da⁻¹ᐟ²`; **cure onset requires Da ≥ 4** (below it O₂ ingress wins
  and nothing cures). `Da` definition + `T_ID` formula in KB-155.
- **Re-oxygenation between layers:** O₂ re-diffuses across a ~30 µm depletion
  depth in seconds (D_O₂ ≈ 1.08×10⁻¹⁰ m²/s), and the lift/peel/reset + recoat
  cycle **mechanically re-oxygenates** the illuminated zone each layer — so the
  induction dose is *re-paid every layer*, and worst at long inter-layer dwell.

## Quantitative anchors

| Quantity | Value | Source |
|---|---|---|
| Dissolved O₂ (air-saturated acrylate) | ~1.5×10⁻³ mol/L (~30 ppm mass, ~4× water; **not** 8 ppm) | Dendukuri 2008; Hoyle 2004 |
| O₂ diffusivity in resin | D_O₂ ≈ 1.08×10⁻¹⁰ m²/s (CLIP; spread 3×10⁻¹¹–6×10⁻¹⁰) | CLIP 2D sim PMC7240730 |
| R•+O₂ vs propagation `k_p` | k_O=5×10⁵, k_p=25 m³/mol·s → k_O/k_p≈2×10⁴ | Dendukuri Table 2 |
| Polymerization onset threshold | [O₂] ↓ **≥3 orders (≲0.1%)**; gel at ~2% conversion | Dendukuri |
| Cure onset criterion | `Da ≥ 4` | Dendukuri Eq. 17 |
| Induction dose magnitude | ~few mJ/cm² — order of clear-resin Ec (PR48 Ec = 6.3 mJ/cm² @405 nm) | NIST PMC5828039 |
| Induction-time form | `T_ID = π·k·Y₀/(4·k′·B)`, Y₀ = [O₂]₀ | Frontiers 2019 Eq. 27 |

Full kinetics constants + Damköhler derivation → **KB-155**.

## Implication for resinsim

resinsim has Ec (KB-100–103), Ec(T) Arrhenius (KB-153), and photoinitiator
depletion (KB-160), but treats the cure threshold as **static** — the oxygen
induction dose is folded *implicitly* into Ec.

- **First-order model:** split `Ec = Ec_optical + Ec_O₂(induction)`, where the
  O₂ term grows with inter-layer dwell (re-diffusion) and shrinks with
  intensity. Consumers: bottom/burn-in layer overexposure, variable layer
  times, and the film-side release chemistry (**KB-116** — same O₂ pool).
- **Explains grayscale support-curing (the greyscale-halo idea).** A low-grey
  flood around supports (values ~100–115/255) does not add enough dose to
  dilate geometry (**KB-122**: sub-threshold, no solid), but it **consumes
  local O₂** — lowering the effective Ec for the adjacent support so it
  over-cures relative to the model. This is a *chemical* effect a pure
  optical-dose model misses.
- **Support-tip green strength.** High surface-area support tips see more O₂
  re-supply per unit volume → longer local induction → weaker cure → the ~40%
  tip-strength factor discussed for `support_capacity` (KB-114 formula). An
  O₂-aware cure model gives a physical basis for a tip-strength derate.

## Caveats

- Literature does **not** pin the exact fraction of Ec attributable to O₂; it
  models the whole surface induction period as oxygen and puts its dose in the
  low single-digit mJ/cm² range (order of clear-resin Ec). Pigmented mSLA
  resins add optical Ec on top.
- Acrylates are markedly more O₂-sensitive than methacrylates — a per-resin,
  per-chemistry parameter, not a universal constant.

## Sources

- Ligon et al., "Strategies to Reduce Oxygen Inhibition in Photoinduced
  Polymerization," *Chem. Rev.* 2014 — https://pubs.acs.org/doi/10.1021/cr3005197
- The effect of monomer structure on oxygen inhibition of (meth)acrylates,
  *Polymer* — https://www.sciencedirect.com/science/article/abs/pii/S0032386104006469
- Simplified 2D simulation of photopolymerization + O₂ diffusion (CLIP) —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC7240730/
- Measuring UV Curing Parameters of Commercial Photopolymers (NIST, PR48
  Ec/Dp) — https://pmc.ncbi.nlm.nih.gov/articles/PMC5828039/
- Dendukuri et al., "Modeling of Oxygen-Inhibited Free Radical
  Photopolymerization…," *Macromolecules* 2008 —
  https://pubs.acs.org/doi/10.1021/ma801219w
- Modeling kinetics, curing depth, efficacy… role of oxygen inhibition —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC6863961/

## See also

- KB-155 — oxygen-inhibition kinetics constants + induction formulas (the
  verified data behind this entry).
- KB-153 — Ec(T) Arrhenius correction (the temperature term this would extend
  with an oxygen term).
- KB-160 — photoinitiator depletion model.
- KB-116 — oxygen-inhibited release layer (film-side of the same O₂ chemistry).
- KB-122 — grayscale dose-sharing (why a sub-material grey flood still consumes
  O₂ and does chemical work).
