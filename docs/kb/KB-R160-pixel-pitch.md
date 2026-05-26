---
id: KB-R160
issue: resinsim
kind: measured-data
date: 2026-04-16
source: manufacturer specs
---

# Pixel pitch and resolution by printer model

| Printer | Resolution | Pixel pitch (µm) | Build area (mm) | Intensity (mW/cm²) |
|---------|-----------|-------------------|-----------------|-------------------|
| Elegoo Mars 5 Ultra | 9K (8520×4800) | 18×18 | 153.4×86.4 | ~4 |
| Elegoo Saturn 2 | 8K (7680×4320) | 28.5×28.5 | 218.9×123.1 | ~4 |
| Elegoo Saturn 3 | 12K (11520×5120) | 19×24 | 218.9×122.9 | 4.07 |
| Phrozen Revo 16K | 16K | 14×19 | — | — |
| Anycubic M7 Pro | 14K | 16.8×24.8 | — | — |
| Anycubic Photon Mono | 4K (3840×2400) | ~51 | 130×80 | 4.63-4.72 |
| Anycubic Photon M3 Plus | 6K (5760×3600) | ~43 | 245×154 | 7.06 |

LED arrays: Saturn uses 54 LEDs, Saturn 2 uses 64 LEDs. All 405nm wavelength.

Pixel pitch determines:
- Minimum feature size (cannot resolve smaller than 1 pixel)
- XY dimensional accuracy
- Light bleed / crosstalk between pixels (Gaussian PSF width scales with pitch)
