---
id: KB-104
issue: resinsim
kind: source
date: 2026-07-06
source: https://pmc.ncbi.nlm.nih.gov/articles/PMC5828039/
---

# Source: NIST — Measuring UV Curing Parameters of Commercial Photopolymers

**Bennett, "Measuring UV curing parameters of commercial photopolymers used in
additive manufacturing," *Additive Manufacturing* 2017 (NIST).**

## What it is

The reference measurement of the Jacobs working curve `Cd = Dp·ln(E₀/Ec)` for
commercial AM resins at 365 and 405 nm. Primary anchor for our Dp/Ec numbers.

## Key data

| Resin | Dp 365 / 405 nm (µm) | Ec 365 / 405 nm (mJ/cm²) |
|---|---|---|
| Autodesk PR48 "Standard Clear" | 42 / 53 | 18.3 / 6.3 |
| Formlabs Clear | 146 / 192 | 5.2 / 12.6 |
| Stratasys VeroClear | 186 / 568 | 2.1 / 6.9 |
| VeroWhitePlus | 43 / 145 | 0.7 / 1.9 |
| TangoBlackPlus | 95 / 151 | 2.4 / 4.1 |

- Ec/Dp vary "as large as 10×" across the five resins.
- Induction period modelled as oxygen inhibition; the whole surface induction is
  attributed to O₂ scavenging.

## Used by

KB-122 (Dp/Ec + wavelength shift), KB-154 (induction dose order-of-magnitude);
companion to KB-100/101/102 (Dp/Ec datasets).

## Link

https://pmc.ncbi.nlm.nih.gov/articles/PMC5828039/
