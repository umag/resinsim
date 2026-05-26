---
id: KB-102
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://www.osti.gov/servlets/purl/2473631
---

# NIST interlaboratory Dp/Ec reproducibility

NIST-coordinated study across 24 laboratories measuring PR48 resin (2024).

## Standardized results

| Wavelength | Dp (µm) | Dp uncertainty | Ec (mJ/cm²) | Ec uncertainty |
|-----------|---------|---------------|-------------|----------------|
| 385nm | 39.2 | ±3.7 | 12.3 | ±3.0 |
| 405nm | 69.3 | ±3.8 | 17.9 | ±2.3 |

## Without standardized method

Before applying NIST standardized procedure:
- Dp varied **7×** across labs
- Ec varied **70×** across labs

This demonstrates that measurement conditions (light source calibration, film thickness control, temperature) dominate the uncertainty, not resin batch variation.

## Implications for simulation

- Dp and Ec are reliable to ~±10% with careful measurement
- Without calibration, published values may be off by an order of magnitude
- Property test ranges should use Dp 40-600µm, Ec 0.5-30 mJ/cm² to cover realistic variation
- Athena II calibration (KB-180) should follow NIST standardized method
