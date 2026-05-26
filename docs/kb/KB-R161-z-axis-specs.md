---
id: KB-R161
issue: resinsim
kind: measured-data
date: 2026-04-16
source: hardware teardowns, manufacturer specs
---

# Z-axis motor and screw specifications

## Standard consumer MSLA printer Z-axis

| Component | Specification |
|-----------|--------------|
| Lead screw | T8 trapezoidal, 8mm lead, 2mm pitch, 4-start |
| Motor | NEMA17, 1.8°/step, 200 steps/rev |
| Microstepping | 16× (standard), 32× (some newer models) |
| Steps/mm (16×) | 400 |
| Step resolution (16×) | 2.5 µm per full step, 0.625 µm per microstep |
| Typical Z speed | 60-180 mm/min |

## Screw type comparison

| Type | Backlash | Cost | Notes |
|------|----------|------|-------|
| T8 trapezoidal | 0.03-0.1 mm | Low | Standard, gravity-preloaded |
| Anti-backlash nut | <0.1 mm | Medium | Eliminates backlash for resin use |
| Ball screw | ~0.05 mm | High | Overkill for unidirectional Z |

Ball screws are considered unnecessary for resin printers because Z-axis motion is unidirectional (always up during print) and gravity preloads the nut against backlash.

## Z-axis stiffness

No published values. See KB-182 for measurement experiment.
Derived estimate from Mrazek deflection data: k ≈ 460 N/mm (Elegoo Mars class).
