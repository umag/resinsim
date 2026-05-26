---
id: KB-112
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://protoresins.com/blog/control-peel-forces-prevent-the-vacuum-effect-in-resin-3d-printing
---

# Peel force vs. lift speed relationship

## Measured data

FEP membrane: 230% force increase with 96× speed increase.
PP membrane: 175% force increase with 96× speed increase.

## Power law model

Approximation: `f(v) = (v / v_ref)^n`

For FEP: 2.30 = 96^n → n = ln(2.30)/ln(96) = 0.182
For PP: 1.75 = 96^n → n = ln(1.75)/ln(96) = 0.122

Conservative FEP model: `f(v) = (v / v_ref)^0.18`

## Test vectors (FEP, n=0.18)

| v/v_ref | f(v) | Notes |
|---------|------|-------|
| 1 | 1.00 | Reference speed |
| 2 | 1.13 | |
| 5 | 1.33 | |
| 10 | 1.51 | |
| 20 | 1.72 | |
| 50 | 2.04 | |
| 96 | 2.30 | Measured data point |

## Practical speed ranges

| Speed (mm/min) | Typical use |
|----------------|------------|
| 30-60 | Normal lifting |
| 60-120 | Fast lifting |
| 120-180 | High-speed modes |
| 180-360 | TSMC/continuous lift |

## Academic measurement

2.32 N at 50 mm/hr for FEP membrane with ~125 mm² cross-section.
Source: ResearchGate publication
