---
id: KB-141
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://blog.honzamrazek.cz/2022/01/prints-not-sticking-to-the-build-plate-layer-separation-rough-surface-on-a-resin-printer-resin-viscosity-the-common-denominator/
---

# Resin viscosity range and temperature dependence

## Viscosity by resin type (at 25°C)

| Resin | Viscosity (mPa·s) | Category |
|-------|-------------------|----------|
| Siraya Simple (water-wash) | 60 | Ultra-low |
| Siraya Fast | ~150 | Standard |
| Elegoo ABS-Like V2 | 150-200 | Standard |
| Anycubic Standard | ~200 | Standard |
| Liqcreate Premium Black | ~300 | Standard |
| Siraya Sculpt | ~600 | High |
| Liqcreate Composite-X | 1400 | Very high |
| Dental resins | 1600-7300 | Specialty |

Printable range: 60-1500 mPa·s. Above 1500 requires heated vat or dilution.

## Temperature dependence

Measured: 82% viscosity reduction from 25°C to 50°C (general resin).
This implies Arrhenius Ea ≈ 52 kJ/mol (see KB-150 for calculation).

One published Ea value: 36 kJ/mol (academic, may be for a different resin class).

| Temperature (°C) | Approx µ/µ₀ (82% drop model) |
|------------------|------------------------------|
| 20 | 1.25 |
| 25 | 1.00 (reference) |
| 30 | 0.78 |
| 35 | 0.60 |
| 40 | 0.45 |
| 45 | 0.32 |
| 50 | 0.18 |

## Critical thresholds

| Temperature | Effect |
|------------|--------|
| < 15°C | Viscosity too high — layer separation, poor adhesion, rough surfaces |
| 15-20°C | Marginal — extend rest times, reduce lift speed |
| 20-35°C | Optimal printing range |
| > 50°C | Microbubble formation, premature gelation risk |

## Practical impact

- A 5°C room temperature drop has more impact on print success than switching resin brands
- Viscosity directly affects: reflow time, peel force, layer adhesion, surface finish
- Below 20°C: failure rates increase significantly
