---
id: KB-172
issue: resinsim
kind: calibration-geometry
date: 2026-04-16
source: custom design for Athena II calibration
---

# Graduated cylinder force calibration geometry

Custom calibration print for Athena II force sensor validation.

## Design

6 cylinders of increasing diameter on a single build plate:

| Cylinder | Diameter (mm) | Cross-section area (mm²) | Expected F at 13 kPa (N) |
|----------|-------------|------------------------|--------------------------|
| 1 | 5 | 19.6 | 0.26 |
| 2 | 10 | 78.5 | 1.02 |
| 3 | 15 | 176.7 | 2.30 |
| 4 | 20 | 314.2 | 4.08 |
| 5 | 25 | 490.9 | 6.38 |
| 6 | 30 | 706.9 | 9.19 |

All cylinders are solid (no hollow sections) to avoid suction effects.
Height: 20mm each (400 layers at 50µm).

## Expected results

- Peel force should increase linearly with cross-section area
- Plot F(measured) vs A → slope = σ_peel for this resin + film combination
- R² should be > 0.95 for linear fit (confirms area-proportional model)
- Intercept should be near zero (no force at zero area)

## Measurement protocol

1. Print all 6 cylinders simultaneously on Athena II
2. Record force sensor data for every layer
3. Each cylinder enters the print at different Z heights (stagger start layers)
4. At each cylinder's start, force should step up by σ_peel × A_cylinder
5. Export force CSV with columns: layer, force_n, timestamp_ms

## Calibration output

Fit: `F = σ_peel × A + intercept`
Store σ_peel in `data/resins/<name>.toml` as `peel_adhesion_kpa`
