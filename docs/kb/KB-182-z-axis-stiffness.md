---
id: KB-182
issue: resinsim
kind: data-gap
date: 2026-04-16
source: gap analysis
---

# Z-axis stiffness (k_axis in N/mm)

## Gap

No published values for Z-axis stiffness of any MSLA printer. The deflection formula (KB-131) requires k_axis to predict effective layer height under peel load.

Current estimate: k ≈ 460 N/mm (derived from Mrazek data, crude).

## Athena II experiment — Method 1 (indirect, during printing)

1. Print graduated cylinders (KB-172) on Athena II
2. Record force per layer from force sensor
3. Simultaneously record actual Z position (if encoder available) or measure part height post-print
4. Calculate: Δz = (expected_height - measured_height) / N_layers
5. For each force level: k = F / Δz

## Athena II experiment — Method 2 (direct, mechanical test)

1. Remove vat and build plate
2. Mount dial indicator (0.01mm resolution) measuring build plate position
3. Apply known forces to build plate via calibrated weights or spring scale
4. Record deflection at each force: 0N, 10N, 20N, 50N, 100N, 150N, 200N
5. Plot F vs. Δz → slope = k_axis

Method 2 is more accurate but requires access to the printer's mechanical assembly.

## Expected values

| Printer class | Estimated k (N/mm) | Basis |
|--------------|-------------------|-------|
| Budget (Elegoo Mars) | 400-600 | Mrazek lag data |
| Mid-range (Saturn) | 600-1000 | Stiffer linear rail |
| Premium (Athena II) | 1000-2000 | Ball screw, stiffer frame |

## Output

k_axis value for Athena II → store in `data/printers/athena_ii.toml`
