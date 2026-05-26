---
id: KB-150
issue: resinsim
kind: formula
date: 2026-04-16
source: https://blog.honzamrazek.cz/2022/01/prints-not-sticking-to-the-build-plate-layer-separation-rough-surface-on-a-resin-printer-resin-viscosity-the-common-denominator/
---

# Vat thermal model formula and test vectors

## Equations

Vat temperature (lumped capacitance):
```
T_vat(t) = T_ambient + ΔT_steady × (1 - exp(-t / τ))
```

Per-layer time:
```
t_layer = layer_index × (exposure_sec + lift_cycle_sec)
```

Heat input from screen:
```
Q_screen = P_led × duty_cycle × A_exposed
duty_cycle = exposure_sec / (exposure_sec + lift_cycle_sec)
```

Viscosity (Arrhenius):
```
µ(T) = µ₀ × exp(Ea / (R × T_kelvin))
```

Simplified viscosity ratio:
```
µ(T₂) / µ(T₁) = exp(Ea/R × (1/T₂ - 1/T₁))
```

Where:
- T_ambient = room temperature (°C)
- ΔT_steady = steady-state temperature rise above ambient (°C)
- τ = thermal time constant of resin volume (seconds)
- P_led = LED array total power (W)
- A_exposed = area of LCD that is illuminated (fraction of total)
- Ea = Arrhenius activation energy for viscosity (J/mol)
- R = 8.314 J/(mol·K)
- µ₀ = viscosity at reference temperature (mPa·s)

## Parameter estimates (not measured — see KB-151, KB-183 for data gaps)

| Parameter | Estimate | Basis |
|-----------|----------|-------|
| P_led | 40-60 W | Saturn-class LED array |
| duty_cycle | 5-50% | 2s exposure / (2s + 8s lift) = 20% typical |
| τ | 600-1800 s (est.) | ~200mL resin, aluminum vat |
| ΔT_steady | 5-15°C (est.) | depends on printer, ambient, ventilation |

## Test vectors — temperature

| T_ambient (°C) | ΔT_steady (°C) | τ (s) | t (s) | Expected T (°C) | Notes |
|----------------|----------------|-------|-------|-----------------|-------|
| 22 | 10 | 1200 | 0 | 22.0 | Start of print |
| 22 | 10 | 1200 | 600 | 25.9 | 10 min, ~39% rise |
| 22 | 10 | 1200 | 1200 | 28.3 | 20 min, ~63% rise |
| 22 | 10 | 1200 | 2400 | 30.6 | 40 min, ~86% rise |
| 22 | 10 | 1200 | 6000 | 31.9 | 100 min, ~99% rise |
| 22 | 10 | 1200 | ∞ | 32.0 | steady state |

## Test vectors — viscosity (Arrhenius)

Using Ea = 36 kJ/mol (one published value), R = 8.314 J/(mol·K):

| µ₀ (mPa·s) | T₀ (°C) | T (°C) | Expected µ (mPa·s) | Ratio µ/µ₀ | Notes |
|------------|---------|--------|-------------------|-----------|-------|
| 200 | 25 | 25 | 200.0 | 1.00 | Reference |
| 200 | 25 | 30 | 155.0 | 0.775 | +5°C |
| 200 | 25 | 35 | 121.3 | 0.607 | +10°C |
| 200 | 25 | 40 | 95.7 | 0.479 | +15°C |
| 200 | 25 | 50 | 61.2 | 0.306 | +25°C |
| 200 | 25 | 50 | ~36 | ~0.18 | measured: 82% drop (discrepancy with Ea=36 → actual Ea may be higher) |

Note: the measured 82% viscosity drop (25→50°C) implies Ea ≈ 52 kJ/mol, not 36. This is a calibration target — see KB-183.

## Test vectors — per-layer temperature (integrated)

Conditions: T_ambient=22°C, ΔT=10°C, τ=1200s, exposure=2.5s, lift_cycle=7.5s (10s total per layer):

| Layer | t (s) | T_vat (°C) | µ/µ₀ (Ea=36) | Notes |
|-------|-------|-----------|-------------|-------|
| 0 | 0 | 22.0 | 1.00 | Cold start |
| 50 | 500 | 25.4 | 0.82 | 8 min in |
| 100 | 1000 | 27.7 | 0.68 | 17 min |
| 200 | 2000 | 30.1 | 0.49 | 33 min |
| 500 | 5000 | 31.7 | 0.37 | 83 min |
| 1000 | 10000 | 32.0 | 0.36 | ~steady state |

## Degradation threshold

| T_vat (°C) | Risk |
|-----------|------|
| < 20 | High viscosity → adhesion failure, poor reflow |
| 20-35 | Normal operating range |
| 35-45 | Reduced viscosity, faster cure, watch for over-exposure |
| 45-50 | Microbubble formation risk in some resins |
| > 50 | Resin degradation, premature polymerization |
