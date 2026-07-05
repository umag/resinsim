---
id: KB-129
issue: resinsim
kind: source
date: 2026-07-06
source: https://www.3ders.org/articles/20160815-autodesk-offers-grayscale-trick-for-dlp-3d-printing-at-sub-pixel-resolution.html
---

# Source: Autodesk Ember — grayscale sub-pixel DLP (3ders)

**"Autodesk offers grayscale trick for DLP 3D printing at sub-pixel resolution,"
3ders 2016 (reporting Autodesk's Richard Greene / Steve Kranz). Patented as
US 10,354,445 B2, "Sub-pixel grayscale three-dimensional printing."**

## What it is

The canonical demonstration of grayscale voxel-dose calibration on the open-source
Ember DLP printer.

## Key data

- **32 gray steps** down a pillar edge → each cured layer ~**1.5 µm thinner** than
  the one below → a **3.6° draft angle within one 50 µm pixel** (Ember pixel =
  50 µm, 405 nm).
- Below a brightness threshold **nothing cures**; above it, hemispherical bumps
  form attached to the previous layer, growing with luminosity (direct
  visualisation of the Ec dose threshold).
- Reduces layer lines/artifacts (XY), not arbitrary sub-pixel *features*.

## Used by

KB-122 (grayscale sub-pixel Z-resolution numbers).

## Link

https://www.3ders.org/articles/20160815-autodesk-offers-grayscale-trick-for-dlp-3d-printing-at-sub-pixel-resolution.html
(secondary: Hackaday 2016; patent US 10,354,445 B2)
