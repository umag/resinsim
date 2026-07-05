---
id: KB-127
issue: resinsim
kind: source
date: 2026-07-06
source: https://www.alexwhittemore.com/efficacy-of-antialiasing-on-msla-prints/
---

# Source: Whittemore — Efficacy of Antialiasing on MSLA Prints

**Alex Whittemore, "Efficacy of Antialiasing on MSLA Prints" (blog, empirical
Elegoo Mars test).**

## What it is

The clearest practitioner statement of the grayscale dose-sharing mechanism, with
an empirical AA test.

## Key data

- Verbatim: a grey pixel *"does result in a lower volume of cured material, which
  **grows off of any adjacent cured surface**"* — sub-pixel rendering, not
  independent partial voxels.
- Verbatim: AA works *"the same way computer screens make text appear smoother."*
- Empirical: 0×/4×/8× AA gave mechanical **XY** smoothing but **zero change to Z
  layer lines** (grayscale XY blending cannot touch Z discretisation).

## Used by

KB-122 (dose-sharing quote, XY-only smoothing).

## Link

https://www.alexwhittemore.com/efficacy-of-antialiasing-on-msla-prints/
