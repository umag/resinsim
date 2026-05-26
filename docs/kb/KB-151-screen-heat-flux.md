---
id: KB-151
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Screen heat flux estimates and measurement needs

## Known parameters

| Parameter | Estimate | Source |
|-----------|----------|--------|
| LED array power (Saturn-class) | 40-60 W total | Manufacturer specs |
| LED count | 54-64 LEDs per array | DrLCD teardowns |
| Per-LED power | ~0.7-1.0 W | Derived |
| UV conversion efficiency | ~30-40% | Typical for 405nm LEDs |
| LCD transmission | ~1-5% (RGB), ~20-30% (mono) | Liqcreate measurements |

## Duty cycle calculation

```
duty_cycle = exposure_sec / (exposure_sec + lift_cycle_sec)
```

| Exposure (s) | Lift cycle (s) | Total (s) | Duty cycle |
|-------------|---------------|-----------|-----------|
| 2.0 | 8.0 | 10.0 | 20% |
| 2.5 | 7.5 | 10.0 | 25% |
| 3.0 | 12.0 | 15.0 | 20% |
| 8.0 | 12.0 | 20.0 | 40% (long exposure resin) |

## Heat flux estimate

Effective heat into resin ≈ LED_power × duty_cycle × (1 - UV_efficiency)
For Saturn-class: ~50W × 0.20 × 0.65 ≈ 6.5 W continuous average

## Vat thermal mass estimate

| Fill level | Volume (mL) | Mass (g) | Thermal capacity (J/K) |
|-----------|------------|---------|----------------------|
| Low (100 mL) | 100 | 110 | ~165 |
| Medium (200 mL) | 200 | 220 | ~330 |
| Full (300 mL) | 300 | 330 | ~495 |

Using specific heat ≈ 1.5 J/(g·K) for typical photopolymer resin.

## Unknowns requiring measurement (see KB-183)

- Actual heat transfer from LED array through LCD to resin
- Vat wall conduction/convection losses
- τ (thermal time constant)
- ΔT_steady (steady-state temperature rise)
- All vary with printer design, ambient temperature, ventilation
