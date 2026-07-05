---
id: KB-128
issue: resinsim
kind: source
date: 2026-07-06
source: https://github.com/sn4k3/UVtools/wiki/Anti-aliasing
---

# Source: UVtools wiki — Anti-aliasing

**UVtools wiki, "Anti-aliasing" (open-source mSLA post-processing tool).**

## What it is

The source of the grayscale AA gray-ladders and the empirical usable-brightness
thresholds, plus the "half pixel" dose-sharing description.

## Key data

- Gray ladders: 2×AA = {255,127}; 4× = {255,191,127,63}; 8× = {255,223,191,159,
  127,95,63,31}; 16× down to 15.
- At ~3 s exposure: imperfections begin at **190/255**; below **~170** pixels
  *stop bonding* to neighbours; below **~160** a complete void — **only the upper
  ~half of the brightness range is usable.**
- Verbatim: *"Adjacent faded pixels will attach to solid neighbors and detach…
  this will create a 'half pixel.'"*
- "Heal anti-aliasing" preset thresholds sub-usable grays to pure black.

## Used by

KB-122 (usable-range thresholds, gray ladders, half-pixel mechanism).

## Link

https://github.com/sn4k3/UVtools/wiki/Anti-aliasing
