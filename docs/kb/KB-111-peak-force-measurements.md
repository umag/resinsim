---
id: KB-111
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://blog.honzamrazek.cz/2019/09/testing-the-precision-of-elegoo-mars-volume-5-whats-wrong-with-the-z-axis-and-how-to-fix-it-finally/
---

# Peak force measurements during resin printing

## Honza Mrazek (Elegoo Mars)

| Resin | Peak force (N) | Notes |
|-------|---------------|-------|
| Siraya Tech Fast | up to 120 | ABS-like, standard viscosity |
| High-viscosity resins | up to 200 | Sculpt-type, thicker |
| Cured resin peel only | single-digit | Without viscous drag |

Forces combine:
1. FEP adhesion (proportional to cured area)
2. Viscous drag (proportional to viscosity × speed × area)
3. Suction from sealed geometries (proportional to sealed area × ΔP)

## Academic measurement

- 2.32 N at 50 mm/hr separation speed for FEP membrane
- Force spike observed around 125 mm² cross-section area
- Source: ResearchGate publication on peel force vs geometry

## Lift speed effect

| Speed multiplier | FEP force increase | PP force increase |
|-----------------|-------------------|-------------------|
| 1× (baseline) | 100% | 100% |
| 96× | 230% | 175% |

Source: ProtoResins blog on peel force control

## Force sensor specifications (commercial)

- HeyGears sensor: 0.1 N sensitivity, 80 Hz sampling rate
- Athena II: built-in force sensor (resolution TBD — ask developers)
