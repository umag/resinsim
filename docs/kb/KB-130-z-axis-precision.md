---
id: KB-130
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://blog.honzamrazek.cz/2019/08/testing-the-precision-of-elegoo-mars/
---

# Z-axis precision measurements (Elegoo Mars)

Comprehensive 5-part measurement series by Honza Mrazek.

## Mechanical specs

| Parameter | Value | Notes |
|-----------|-------|-------|
| Lead screw type | T8 trapezoidal | 8mm lead, 2mm pitch, 4-start |
| Motor | NEMA17 | 1.8°/step, 200 steps/rev |
| Microstepping | 16× | |
| Step resolution | 0.625 µm | 400 steps/mm |
| Screw precision | ±15 µm | |
| Backlash | 30 µm | |

## Mechanical play

| Component | Play (mm) | Notes |
|-----------|-----------|-------|
| Lead screw housing | ~2.0 | Spring washer misapplication (design flaw) |
| Linear rail | ~0.2 | Under hand force |

## Position lag under peel load

| Resin | Immediate lag (µm) | After 30s settling (µm) |
|-------|-------------------|------------------------|
| Siraya Tech Fast | 260 | 80-100 |
| Siraya Tech Sculpt | 340 | 80-100 |

Motor must travel up to 5 mm before actual movement begins (worst case with housing play).

## Dimensional accuracy

| Target (mm) | Measured (mm) | Error | Notes |
|------------|--------------|-------|-------|
| 3.0 (cube) | 2.9 | -0.1 | First-layer compression |
| 15.0 (staircase) | 14.7 | -0.3 (2%) | Cumulative over 30 half-mm steps |

After backlash fix (spring washer replaced with M8 washers):
- 3mm cube: 3.0 ± 0.1 mm
- Position tracks within 10 µm above 2mm build height

## Settling behavior

- Resin settling time after Z move: ~4 seconds
- First 5 layers have unreliable height (plate contact compression)
- Recommended bottom layers: 6-10 at 5-10× exposure
