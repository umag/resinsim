---
id: KB-114
issue: resinsim
kind: formula
date: 2026-04-16
source: https://ameralabs.com/blog/are-fep-alternatives-worth-it-acf-vs-nfep-vs-fep-films/
---

# Peel force formula and test vectors

## Equations

Total peel force per layer:
```
F_total = F_peel + F_suction
F_peel = σ_peel × A_layer × f(v_lift)
F_suction = ΔP × A_sealed
```

Support capacity:
```
F_max = σ_tensile × π × r_tip² × N_supports
```

Safety factor:
```
SF = F_max / F_total
```

Failure criterion:
```
F_total > F_max → PRINT FAILURE at this layer
```

Where:
- σ_peel = peel adhesion stress (kPa), depends on film type
- A_layer = cross-section area of cured layer (mm²)
- f(v_lift) = lift speed factor (dimensionless, ≥1.0)
- ΔP = pressure differential for sealed cavities (kPa), max ≈ 101 kPa (atmospheric)
- A_sealed = area of sealed cavity against FEP (mm²)
- σ_tensile = resin tensile strength (MPa)
- r_tip = support tip contact radius (mm)
- N_supports = number of supports contacting this layer

## Peel adhesion by film type

| Film | σ_peel (kPa) | Source |
|------|-------------|--------|
| ACF 300µm | 12 | AmeraLabs |
| FEP 50µm | 13 | AmeraLabs |
| nFEP 127µm | 13 | AmeraLabs |
| FEP 127µm | ~18 | AmeraLabs |
| FEP 150µm | ~18 | AmeraLabs |

## Lift speed factor

FEP: 230% force increase with 96× speed increase.
Approximation: `f(v) = (v / v_ref)^0.15` (power law fit).

| v_lift (mm/min) | f(v) approx | Notes |
|----------------|-------------|-------|
| 1 | 1.0 | reference speed |
| 10 | 1.41 | |
| 96 | 2.30 | measured on FEP |

## Test vectors — peel force

| σ_peel (kPa) | A (mm²) | f(v) | Expected F (N) | Notes |
|-------------|---------|------|----------------|-------|
| 13 | 2500 | 1.0 | 32.5 | 50mm×50mm on standard FEP |
| 13 | 100 | 1.0 | 1.3 | Small cross section |
| 13 | 8160 | 1.0 | 106.1 | Full Saturn plate (120×68mm) |
| 18 | 2500 | 1.0 | 45.0 | Same area on thick FEP |
| 12 | 2500 | 1.0 | 30.0 | Same area on ACF |
| 13 | 2500 | 2.3 | 74.75 | Fast lift speed |

## Test vectors — suction force

| ΔP (kPa) | A_sealed (mm²) | Expected F (N) | Notes |
|----------|---------------|----------------|-------|
| 101 | 100 | 10.1 | Fully sealed 10mm diameter cup |
| 101 | 706 | 71.3 | Fully sealed 30mm diameter cup |
| 0 | 706 | 0 | Same cup with drain holes (no seal) |
| 50 | 314 | 15.7 | Partial seal, 20mm diameter |

## Test vectors — support capacity

| σ_tensile (MPa) | r_tip (mm) | N | Expected F_max (N) | Notes |
|-----------------|-----------|---|-------------------|-------|
| 30 | 0.2 | 1 | 3.77 | Single 0.4mm tip |
| 30 | 0.2 | 10 | 37.7 | 10 supports |
| 50 | 0.25 | 5 | 49.1 | Stronger resin, larger tips |
| 30 | 0.1 | 20 | 18.85 | Many small tips |

## Test vectors — safety factor

| F_total (N) | F_max (N) | SF | Result |
|------------|----------|-----|--------|
| 10 | 37.7 | 3.77 | SAFE |
| 37.7 | 37.7 | 1.0 | MARGINAL |
| 50 | 37.7 | 0.754 | FAIL |
| 0.5 | 37.7 | 75.4 | SAFE (oversupported) |
