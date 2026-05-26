---
id: KB-183
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Vat thermal time constant (τ) and steady-state ΔT

## Gap

No published values for vat thermal time constant or steady-state temperature rise for any printer. The thermal model (KB-150) requires τ and ΔT_steady to predict per-layer temperature drift, which feeds into viscosity and cure kinetics calculations.

## Athena II experiment

**Setup:**
1. Waterproof temperature probe (thermocouple or thermistor, ±0.1°C)
2. Place probe in resin vat, submerged but not touching vat walls or build plate
3. Record ambient temperature (room thermometer)

**Protocol:**
1. Fill vat with standard resin at room temperature
2. Start recording temperature at 1-second intervals
3. Start a long print (≥500 layers, ~2 hours)
4. Continue recording until temperature stabilizes (no change for 10+ minutes)
5. Stop print, continue recording cooldown

**Expected data format (CSV):**
```
timestamp_s,temperature_c,layer,ambient_c
0,22.1,0,22.0
30,22.3,3,22.0
60,22.6,6,22.0
...
7200,31.5,720,22.0
```

**Analysis:**
1. Fit: T(t) = T_ambient + ΔT × (1 - exp(-t/τ))
2. Extract τ (thermal time constant) and ΔT (steady-state rise)
3. Validate: cooldown τ should match warmup τ (same thermal mass)

**Repeat at:**
- Different resin fill levels (100mL, 200mL, 300mL) — τ should scale with volume
- Different ambient temperatures (if practical)
- Different print geometries (small vs. full-plate cross sections affect duty cycle)

**Also measure:**
- Viscosity of resin sample at room temp and at measured steady-state temp (viscometer)
- This gives direct Arrhenius calibration: Ea = R × T₁ × T₂ × ln(µ₁/µ₂) / (T₂ - T₁)

## Output

τ and ΔT_steady for Athena II → store in `data/printers/athena_ii.toml`
Ea for tested resin → store in `data/resins/<name>.toml`
