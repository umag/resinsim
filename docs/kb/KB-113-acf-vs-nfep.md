---
id: KB-113
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://siraya.tech/blogs/news/a-deep-dive-into-acf-vs-nfep-films-peeling-force
---

# ACF vs nFEP force reduction

ACF (Anti-Clouding Film) reduced average peel force by ~40% vs nFEP.

Measured on Athena printer with Atlas Vulcan resin by Siraya Tech.

## Film comparison summary

| Film | Relative peel force | Notes |
|------|-------------------|-------|
| ACF | 60% (baseline) | Lowest peel, anti-clouding |
| nFEP | 100% (reference) | Standard replacement film |
| FEP (thin) | ~100% | Similar to nFEP |
| FEP (thick) | ~140% | Highest peel force |

## Simulation model

FilmCondition enum should produce these relative force multipliers:
```
ACF   → σ_peel = 12 kPa (from KB-110)
nFEP  → σ_peel = 13 kPa
FEP   → σ_peel = 13-18 kPa (thickness dependent)
```

When switching from nFEP to ACF: expect ~8% peel force reduction (12/13 = 0.92).
The 40% reduction measured by Siraya may include viscous drag differences due to film flexibility.
