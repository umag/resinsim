---
id: KB-117
issue: resinsim
kind: measured-data
date: 2026-07-05
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC10302688/
---

# Separation-force reduction methods and CLIP dead-zone dataset

Verified measured data underpinning KB-116 (film-side oxygen / release-force
mechanism). Force-reduction figures from the high-speed VPP review
(PMC10302688) unless noted.

## CLIP dead-zone (Tumbleston 2015, Eq. 1)

```
Dead-zone thickness ≈ C · (Φ₀·αPI / Dc₀)^(−1/2)
```
- `Φ₀` = incident photon flux; `αPI` = photoinitiator conc. × absorptivity;
  `Dc₀` = resin reactivity; `C ≈ 30` (Teflon AF 2400, 100 µm window, air below).
- Higher flux/absorption ⇒ **thinner** dead zone (more radicals consume O₂
  faster). Empirical operational floor **~20–30 µm** (below → window-adhesion
  defects).
- **Gas dependence:** pure O₂ below the window ≈ **doubles** the dead zone;
  **N₂ makes it vanish** (no continuous printing). Up to ~370 µm at specific
  window/island geometries.
- Enables continuous pull **300 → >1000 mm/hr**; parts >25 cm; 50 µm features.

## Separation-force reduction methods

| Method | Force reduction | Source |
|---|---|---|
| Two-channel PDMS interface | to **~4–5% of baseline** (~95% lower) | PMC10302688 ref. |
| Slippery S-PDMS vs standard PDMS | **~13× lower** | PMC10302688 |
| Soft hydrogel interface vs FEP | **~1/3 the force** | Nat. Commun. 2021 |
| Piezoelectric vibration-assisted | **~75% lower** vs direct pull | PMC10302688 ref. 71 |
| Acoustic/loudspeaker vibration | **~60% lower** (500→200 gm peak) | PMC10302688 ref. 70 |
| Rotation-assisted | **14% measured / up to 44% simulated** (1.065→0.596 N) | Hu et al. 2023 |
| Tilting mechanism | **~20% lower** | PMC10302688 ref. 69 |
| Film de-tensioning before separation | **6.35 N → ~1 N** | large-scale SLA tensioning study |

**Rigid vs flexible interface (do not conflate — two different comparisons):**
- **~100×** — rigid vs flexible interface, LCD VPP (ACS Appl. Polym. Mater. 2025).
- **~10×** — quartz (rigid) vs PDMS (PMC10302688 ref. 60).
- **Teflon/FEP ~10× HIGHER than soft PDMS** — stiffer film peels *worse*, not a
  reduction (ACS 2025). NOT "FEP reduces force 10×."
- Inside Pan alone: ~25 N (rigid glass) vs 0.73 N (4 mm PDMS) at 1.56 mm/s.

## Film peel-STRESS (AmeraLabs, force-per-area, fresh films)

| Film | Peel stress (kPa) |
|---|---|
| ACF (300 µm) | 12 |
| FEP 50 µm | 13 |
| nFEP 127 µm | 13 |
| FEP 127 µm | 17 |
| FEP 150 µm | 18 |

ACF ≈ 30% below thick FEP; **nFEP ≈ equal to thin FEP** — nFEP's benefit is
release consistency / clarity / durability, not headline peel force.

## Passive-oxygen-reservoir caveat (anecdotal)

A passive O₂-permeable film without active O₂ feed: dissolved-O₂ reservoir
consumed after **~10 exposure cycles**, ~**30 min** air recovery (Honza Mrázek,
LCD experiment — his *opinion*, and **that experiment FAILED**: the silicone
film *increased* peel force ~5× vs FEP and did not achieve continuous printing).
Cite as depletion-kinetics anecdote, NOT proof that passive O₂ reservoirs work.
CLIP's persistent dead zone requires a *constant pure-O₂ flow* under the window.

## Sources

- A Review of Critical Issues in High-Speed Vat Photopolymerization, *Polymers*
  2023 — https://pmc.ncbi.nlm.nih.gov/articles/PMC10302688/
- Tumbleston et al., CLIP, *Science* 2015 (dead-zone Eq. 1; gas dependence) —
  https://fab.cba.mit.edu/classes/865.18/additive/clip.pdf
- Impact of Interface Flexibility on Separation Force in LCD VPP, *ACS Appl.
  Polym. Mater.* 2025 (100×, Teflon 10× higher) —
  https://pubs.acs.org/doi/full/10.1021/acsapm.5c00167
- Hu et al., Rotation-Assisted Separation, 2023 (14%/44%) —
  https://pmc.ncbi.nlm.nih.gov/articles/PMC10049864/
- Rapid DLP via soft hydrogel separation interface, *Nat. Commun.* 2021 —
  https://www.nature.com/articles/s41467-021-26386-6
- AmeraLabs, ACF vs nFEP vs FEP films —
  https://ameralabs.com/blog/are-fep-alternatives-worth-it-acf-vs-nfep-vs-fep-films/
- Honza Mrázek, Continuous printing on LCD (O₂ reservoir; failed) —
  https://blog.honzamrazek.cz/2022/12/continuous-printing-on-lcd-resin-printer-no-more-wasted-time-on-peeling/

## See also

- KB-116 — oxygen-inhibited release layer (synthesis entry this backs).
- KB-113 / KB-110 / KB-114 — film peel stress, formula.
- KB-185 / KB-186 — peel-front geometry and separation-force equations.
