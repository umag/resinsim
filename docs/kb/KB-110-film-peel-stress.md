---
id: KB-110
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://ameralabs.com/blog/are-fep-alternatives-worth-it-acf-vs-nfep-vs-fep-films/
---

# Film peel stress measurements (AmeraLabs)

Measured on Asiga Max X43 DLP printer with TGM-7 Grey resin, 50µm layers, 25 mW/cm² UV.

| Film | Thickness (µm) | Peel stress (kPa) |
|------|----------------|-------------------|
| ACF | 300 | 12 |
| FEP | 50 | 13 |
| nFEP | 127 | 13 |
| FEP | 127 | ~18 |
| FEP | 150 | ~18 |

Key observations:
- ACF has lowest peel stress (12 kPa) — best for reducing separation forces
- FEP and nFEP at standard thickness are similar (13 kPa)
- Thicker FEP significantly increases peel stress (~38% higher at 127µm vs 50µm)
- Film type affects peel stress by up to 50% (12 vs 18 kPa)

Siraya Tech independent measurement:
- ACF reduced average peel force by ~40% vs nFEP (on Athena printer, Atlas Vulcan resin)
- Source: https://siraya.tech/blogs/news/a-deep-dive-into-acf-vs-nfep-films-peeling-force

## Force calculation examples

For 50mm × 50mm cross-section (2500 mm² = 0.0025 m²):
- ACF: 12000 Pa × 0.0025 m² = 30.0 N
- Standard FEP: 13000 Pa × 0.0025 m² = 32.5 N
- Thick FEP: 18000 Pa × 0.0025 m² = 45.0 N

For full Saturn build plate (120mm × 68mm = 8160 mm²):
- Standard FEP: 13000 × 0.00816 = 106.1 N
- Thick FEP: 18000 × 0.00816 = 146.9 N
