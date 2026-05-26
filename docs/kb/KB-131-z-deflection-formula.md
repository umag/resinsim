---
id: KB-131
issue: resinsim
kind: formula
date: 2026-04-16
source: https://blog.honzamrazek.cz/2019/09/testing-the-precision-of-elegoo-mars-volume-5-whats-wrong-with-the-z-axis-and-how-to-fix-it-finally/
---

# Z-axis deflection formula and test vectors

## Equations

Elastic deflection under peel load (spring model):
```
Δz = F_peel / k_axis
```

Effective layer height:
```
h_effective = h_commanded - Δz
```

Where:
- Δz = Z-axis position error (µm)
- F_peel = total peel force at this layer (N)
- k_axis = Z-axis stiffness (N/mm) — printer-specific
- h_commanded = programmed layer height (µm)
- h_effective = actual cured layer thickness (µm)

## Stiffness derivation from Mrazek data

No published k_axis values exist. Derived from Elegoo Mars measurements:
- Siraya Tech Fast: F≈120N, lag≈260µm → k ≈ 120/0.260 = **462 N/mm**
- Sculpt resin: F≈200N (est.), lag≈340µm → k ≈ 200/0.340 = **588 N/mm**
- After 30s settling: lag drops to 80-100µm (viscoelastic relaxation, not pure elastic)

Conservative estimate for budget printers: **k_axis ≈ 460 N/mm**
Better printers with ball screws: k_axis likely 1000-2000 N/mm (unverified)

## Test vectors

| F_peel (N) | k_axis (N/mm) | h_cmd (µm) | Δz (µm) | h_eff (µm) | Notes |
|-----------|-------------|-----------|---------|-----------|-------|
| 0 | 460 | 50 | 0 | 50 | No force = no deflection |
| 10 | 460 | 50 | 21.7 | 28.3 | Light load |
| 50 | 460 | 50 | 108.7 | -58.7 | Deflection > layer height = CATASTROPHIC |
| 120 | 460 | 50 | 260.9 | -210.9 | Mrazek measured value (Fast resin) |
| 200 | 460 | 50 | 434.8 | -384.8 | Mrazek measured value (Sculpt resin) |
| 120 | 1500 | 50 | 80.0 | -30.0 | Better Z-axis (ball screw) |
| 10 | 1500 | 50 | 6.7 | 43.3 | Stiff axis + light load = minimal effect |
| 50 | 460 | 100 | 108.7 | -8.7 | Thicker layers help |

## Failure criteria

| Condition | Severity | Action |
|-----------|----------|--------|
| h_effective < 0 | CRITICAL | Layer compressed into previous — Z can't reach target |
| h_effective < 0.5 × h_commanded | WARNING | Significant layer thickness variation |
| Δz > h_commanded | CRITICAL | Deflection exceeds single layer — print quality severely degraded |

## Notes

- The spring model is a simplification. Real Z-axis has:
  - Backlash hysteresis (0.03mm typical)
  - Viscoelastic settling (4s time constant)
  - First-layer compression from leveling error (~0.1-1.0mm)
- For simulation: use spring model for Tier 1, add settling if Tier 2 timing is modeled
- k_axis is a DATA GAP — see KB-182 for Athena II measurement experiment
