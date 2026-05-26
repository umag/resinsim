---
id: KB-173
issue: resinsim
kind: calibration-geometry
date: 2026-04-16
source: custom design for Athena II validation
---

# Suction cup test geometry

Custom calibration print for validating suction force detection and quantification.

## Design

Hollow cups of varying geometry, some sealed, some with drain holes:

| Cup | OD (mm) | Wall (mm) | Sealed? | Drain holes | Sealed area (mm²) |
|-----|---------|-----------|---------|-------------|-------------------|
| A1 | 10 | 1.0 | Yes | 0 | 50.3 |
| A2 | 10 | 1.0 | No | 2×1mm | 0 |
| B1 | 20 | 1.0 | Yes | 0 | 254.5 |
| B2 | 20 | 1.0 | No | 2×1mm | 0 |
| C1 | 30 | 1.0 | Yes | 0 | 615.8 |
| C2 | 30 | 1.0 | No | 2×1mm | 0 |
| D1 | 20 | 0.5 | Yes | 0 | 283.5 |
| D2 | 20 | 2.0 | Yes | 0 | 201.1 |

Cup height: 15mm. Sealed cups have a closed bottom face that seals against FEP.

## Expected results

**Sealed vs. drained (same geometry):**
- A1 force >> A2 force (suction dominates for sealed)
- B1 force >> B2 force
- C1 force >> C2 force
- Drained cups: F ≈ σ_peel × A_wall_cross_section (ring area only)
- Sealed cups: F ≈ σ_peel × A_wall + ΔP × A_sealed_cavity

**Force estimates (sealed, ΔP ≈ 50 kPa partial vacuum):**

| Cup | Wall area (mm²) | Peel F (N) | Suction F (N) | Total F (N) |
|-----|----------------|-----------|--------------|------------|
| A1 | 28.3 | 0.37 | 2.5 | 2.9 |
| B1 | 59.7 | 0.78 | 12.7 | 13.5 |
| C1 | 91.1 | 1.18 | 30.8 | 32.0 |

The suction term dominates by 7-26× for sealed cups.

## Validation criteria

1. SuctionDetector must identify all sealed cups (A1, B1, C1, D1, D2) as suction hazards
2. SuctionDetector must NOT flag drained cups (A2, B2, C2)
3. Predicted suction force must be within 50% of measured (given ΔP uncertainty)
4. Force ratio sealed/drained should be > 5× for 20mm+ cups
