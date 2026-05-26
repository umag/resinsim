---
id: KB-103
issue: resinsim
kind: formula
date: 2026-04-16
source: https://www.liqcreate.com/supportarticles/resin-cure-depth-ec-dp/
---

# Beer-Lambert working curve formula and test vectors

## Equations

Cure depth (Jacobs working curve):
```
Cd = Dp × ln(E / Ec)
```

Light intensity at depth z:
```
I(z) = I₀ × exp(-z / Dp)
```

Energy dose:
```
E = I₀ × t_exposure
```

Where:
- Cd = cure depth (µm)
- Dp = penetration depth — depth at which intensity drops to 1/e (37%) of surface (µm)
- Ec = critical energy — minimum dose to initiate gelation (mJ/cm²)
- E = energy dose at surface (mJ/cm²)
- I₀ = surface irradiance (mW/cm²)
- t_exposure = exposure time (seconds)

## Valid ranges

| Parameter | Min | Max | Unit |
|-----------|-----|-----|------|
| Dp | 40 | 600 | µm |
| Ec | 0.5 | 30 | mJ/cm² |
| E | 1 | 100 | mJ/cm² |
| I₀ | 2 | 30 | mW/cm² |

## Test vectors

| Dp (µm) | Ec (mJ/cm²) | E (mJ/cm²) | Expected Cd (µm) | Notes |
|----------|-------------|-------------|-------------------|-------|
| 100 | 10 | 27.183 | 100.0 | ln(e) = 1 exactly |
| 100 | 10 | 10.0 | 0.0 | E = Ec → ln(1) = 0 |
| 100 | 10 | 5.0 | -69.3 | E < Ec → undercured (negative) |
| 170 | 5.0 | 10.0 | 117.8 | Liqcreate Premium Black at 405nm |
| 170 | 5.0 | 5.0 | 0.0 | Exact threshold |
| 350 | 6.87 | 20.0 | 373.7 | Liqcreate Premium White at 405nm |
| 42 | 18.3 | 50.0 | 42.3 | PR48 at 365nm (academic) |
| 568 | 6.9 | 50.0 | 1117.7 | VeroClear at 405nm — deep penetration |

## Edge cases

| Input | Expected behavior |
|-------|-------------------|
| Ec = 0 | Error: division by zero in ln(E/0) |
| Dp = 0 | Cd = 0 for all E (no penetration) |
| E = 0 | Error: ln(0) undefined |
| Dp < 0 | Error: non-physical |
| E = Ec | Cd = 0 exactly |
| E >> Ec | Cd grows logarithmically (diminishing returns) |

## Intensity test vectors

| I₀ (mW/cm²) | z (µm) | Dp (µm) | Expected I(z) (mW/cm²) |
|-------------|---------|---------|------------------------|
| 5.0 | 0 | 170 | 5.0 |
| 5.0 | 170 | 170 | 1.839 |
| 5.0 | 340 | 170 | 0.677 |
| 5.0 | 50 | 170 | 3.726 |
