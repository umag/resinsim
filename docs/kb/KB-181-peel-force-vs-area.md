---
id: KB-181
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Peel force vs. cross-section area dataset

## Gap

No published datasets contain full force-vs-area curves for resin printing. AmeraLabs published summary statistics (KB-110), and Mrazek published peak forces (KB-111), but no one has published a complete F(A) relationship with continuous area variation.

This is the most critical dataset for validating the peel force model.

## Athena II experiment

**Print:** Graduated cylinder geometry (KB-172) — 6 cylinders of diameter 5-30mm.

**Record:** Athena II force sensor output for every layer.

**Expected data format (CSV):**
```
layer,force_n,timestamp_ms,lift_speed_mm_min,area_mm2
0,0.1,0,60,0
1,0.15,10000,60,19.6
...
100,1.5,1000000,60,98.1
```

**Analysis:**
1. Plot F vs. A for each cylinder section
2. Fit linear model: F = σ × A + intercept
3. Extract σ_peel (slope) and validate intercept ≈ 0
4. Check R² > 0.95

**Repeat at:**
- 2+ lift speeds (60 mm/min, 180 mm/min) to calibrate f(v_lift)
- 2+ resins (standard, high-viscosity) to check σ_peel variation
- 2+ film types (FEP, nFEP) if available

**Output:** Calibrated σ_peel and f(v) parameters per resin/film combination.
