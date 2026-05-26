---
id: KB-184
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Suction force quantification for sealed geometries

## Gap

Sealed cavities (suction cups) are widely known to cause catastrophic print failures, but no quantitative force measurements exist for specific sealed geometries. The simulation needs measured suction forces to validate the SuctionDetector and calibrate ΔP estimates.

Current model: F_suction = ΔP × A_sealed, where ΔP is unknown (theoretical max: 101 kPa atmospheric).

## Athena II experiment

**Print:** Suction cup test geometry (KB-173) — sealed and drained cups at 10, 20, 30mm diameter.

**Record:** Athena II force sensor output for every layer.

**Key measurements:**
1. Force at layer where sealed cup base completes (cavity seals against FEP)
2. Compare matched pairs: sealed A1 vs. drained A2, B1 vs. B2, C1 vs. C2
3. Force difference = suction contribution: ΔF = F_sealed - F_drained

**Expected results:**

| Cup pair | Wall F (N) | Suction F (N) | Total sealed (N) | Ratio sealed/drained |
|----------|-----------|--------------|------------------|---------------------|
| A (10mm) | ~0.4 | 2-5 | 2.4-5.4 | 6-14× |
| B (20mm) | ~0.8 | 10-25 | 10.8-25.8 | 14-32× |
| C (30mm) | ~1.2 | 25-60 | 26.2-61.2 | 22-51× |

**Derive ΔP:**
```
ΔP = ΔF / A_sealed
```
If ΔP is consistent across cup sizes → model is valid.
If ΔP varies with diameter → model needs geometry correction.

**Repeat at:**
- 2+ lift speeds — suction may be speed-dependent (faster lift = less time for air ingress)
- Partially sealed geometries (one drain hole vs. two) to characterize partial seal behavior

## Output

Calibrated ΔP range for suction model.
Validation: SuctionDetector correctly identifies sealed vs. drained geometries.
