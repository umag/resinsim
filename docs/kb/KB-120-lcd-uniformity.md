---
id: KB-120
issue: resinsim
kind: measured-data
date: 2026-04-16
source: https://blog.honzamrazek.cz/2022/12/about-the-successful-quest-for-perfect-msla-printer-uv-backlight/
---

# LCD uniformity measurements by printer model

> **Terminology note (2026-04-22, from phase1-verification-audit).** Two
> dimensional-error figures in this KB are easy to conflate:
>
> - **"12 µm per layer"** (Saturn 1) — per-layer dimensional error from
>   uniformity variation at a single exposure.
> - **"157 µm"** (uncompensated) — *cumulative* dimensional variation
>   across the build plate summed over many layers without LCD-mask
>   compensation. "56 µm" is the same cumulative figure with compensation
>   applied.
>
> When comparing a simulator's edge-vs-center spread to this KB, use the
> 12 µm per-layer figure, not the 157 µm cumulative one.
> `UniformityCalculator::cure_depth_spread` returns a per-exposure extreme,
> not a cumulative variation.

## DrLCD project (Honza Mrazek)

| Printer | Uniformity variation | Cone angle | Max dimensional error |
|---------|---------------------|------------|----------------------|
| Elegoo Saturn 1 | 34% | 4-7° | 12 µm per layer |
| Elegoo Saturn 2 | 22% | 2-4° | ~7 µm per layer |
| Peopoly Forge | — | 6-10° | up to 17 µm |
| Original Mars | — | 14-35° | 28 µm (130 µm with glass) |

Backlight compensation (LCD mask correction):
- Uncompensated: 157 µm dimensional variation across build plate
- Compensated: 56 µm (3× improvement)

Cone angle = angular spread of light from LCD pixels. Larger angle = more light bleed = worse XY accuracy at distance from LCD.

## UV intensity by printer model

Source: Liqcreate UV meter measurements (https://www.liqcreate.com/supportarticles/uv-intensity-lcd-405nm-3d-printer-chitu/)

| Printer | Avg intensity (mW/cm²) | Within-printer deviation |
|---------|----------------------|------------------------|
| Photon M3 Plus | 7.06 | 3.1% |
| Photon Mono X | 7.32 | — |
| Photon Mono (4 units) | 4.63-4.72 | 5.8-14.6% |
| Elegoo Saturn 3 | 4.07 | 9.4% |
| Elegoo Saturn | 3.72 | — |
| Elegoo Mars 2 | 3.69 | — |
| Elegoo Mars 3 | 3.32 | — |
| Creality Halot-One | 2.63 | — |
| Photon D2 (DLP) | 2.92 | — |

General ranges:
- LCD printers: 3-5 mW/cm² typical
- DLP printers: 20-30 mW/cm²

Unit-to-unit variation (Photon Mono, 4 units): 5.8-14.6% deviation between highest and lowest measurement points across a 9-point grid.
