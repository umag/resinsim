---
id: KB-170
issue: resinsim
kind: calibration-geometry
date: 2026-04-16
source: https://ameralabs.com/blog/town-calibration-part/
---

# AmeraLabs Town calibration test

Standard community calibration geometry for MSLA/DLP resin printers.

## Features tested

| Feature | Range | What it tests |
|---------|-------|--------------|
| Slots | 0.1-1.0 mm width | Negative space resolution |
| Protrusions | 0.1-1.0 mm width | Positive feature resolution |
| Pillars | 0.1-0.5 mm thick, 1-4 mm tall | Minimum support-free feature |
| Depth plates | 0.025-0.200 mm in 0.025 mm steps | Minimum curable layer thickness |
| Checkerboard | 1.0 mm squares | XY accuracy and pixel bleed |

## Simulation predictions

At correct exposure:
- Slots ≥ 0.3mm should be clear (no overcure bridging)
- Protrusions ≥ 0.2mm should survive (sufficient cure)
- Pillars ≥ 0.3mm × 2mm should stand (sufficient strength vs. peel)
- Depth plates at Cd ≥ plate thickness should resolve

At overexposure:
- Slots fill in (light bleed exceeds gap)
- Features fuse together
- Loss of fine detail

At underexposure:
- Pillars fail (insufficient cure depth → delamination)
- Thin protrusions don't cure
- Depth plates below Cd threshold are absent
