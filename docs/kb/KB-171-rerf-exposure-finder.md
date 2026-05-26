---
id: KB-171
issue: resinsim
kind: calibration-geometry
date: 2026-04-16
source: ChiTuBox community standard
---

# RERF (Resin Exposure Range Finder)

Standard exposure calibration method for MSLA printers.

## Design

8 identical test models arranged on a single build plate.
Each model receives a different exposure time: base + N × Ts

Example with base=1.5s, step=0.25s:
| Position | Exposure (s) | E at 4 mW/cm² (mJ/cm²) |
|----------|-------------|------------------------|
| 1 | 1.50 | 6.0 |
| 2 | 1.75 | 7.0 |
| 3 | 2.00 | 8.0 |
| 4 | 2.25 | 9.0 |
| 5 | 2.50 | 10.0 |
| 6 | 2.75 | 11.0 |
| 7 | 3.00 | 12.0 |
| 8 | 3.25 | 13.0 |

Each model contains:
- Holes of varying diameter (test negative space resolution)
- Pillars of varying diameter (test positive feature resolution)
- Flat surfaces (test surface quality)

## Simulation prediction

For each exposure level, simulation should predict:
- Cure depth: Cd = Dp × ln(E / Ec)
- Whether Cd > layer_height (sufficient cure)
- Which features survive at each exposure level

Example (Liqcreate Premium Black, Dp=170µm, Ec=5.0 mJ/cm²):

| Position | E (mJ/cm²) | Cd (µm) | Sufficient for 50µm? |
|----------|-----------|---------|---------------------|
| 1 | 6.0 | 31.0 | No — undercured |
| 2 | 7.0 | 57.0 | Yes — marginal |
| 3 | 8.0 | 79.5 | Yes |
| 4 | 9.0 | 99.3 | Yes |
| 5 | 10.0 | 117.7 | Yes — 2.4× margin |

Optimal exposure ≈ position 3-4 (1.5-2× cure margin above layer height).
