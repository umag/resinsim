---
issue: resin-recipe-model
date: 2026-04-21
---

# Anti-pattern: Overlay-on-muddy-aggregate as refactor shortcut

## Context

When a domain aggregate mixes concerns with different change cadences (e.g.
`PrinterProfile` mixing hardware mechanics with print recipe), a tempting "light touch"
fix is to keep the muddy aggregate intact and add a per-consumer override layer:

```rust
// BAD — the shortcut
struct PrinterProfile {
    // ... hardware fields ...
    normal_exposure_sec: f32,  // recipe-shaped, but stays on printer
}

struct ResinProfile {
    // ... chemistry ...
    exposure_override_sec: Option<f32>,  // NEW: overlay
}

// Caller picks:
let exposure = resin.exposure_override_sec.unwrap_or(printer.normal_exposure_sec);
```

## Why it's an anti-pattern

1. **Preserves the original bug.** If a caller forgets to consult the overlay, they get
   the printer's baked value — exactly the silent-wrong-value behaviour the refactor is
   trying to fix.
2. **Inverts the natural mental model.** Real-world slicers (Voxeldance Tango, ChituBox,
   Lychee) present these fields as resin settings. The overlay pattern keeps them as
   printer settings "with per-resin overrides", which reads backwards.
3. **Multiplies optionality.** Every consumer now has to decide whether to apply the
   overlay. Every consumer is a new bug site.
4. **Defers the real fix.** The muddy aggregate remains; the next refactor will have to
   undo the overlay AND do the correct split.

## Correct alternative

Split the aggregate at the cadence boundary. Move the recipe-shaped fields to the
aggregate that naturally owns them (the resin, in the ADR-0005 case), and add a
pairing-validation domain service to enforce compatibility at the boundary.

See `docs/adr/0005-three-axis-printer-resin-recipe.md` for the worked example.

## When is an overlay legitimate?

Overlays are fine for **genuinely per-instance overrides** that layer on top of a
well-partitioned default — e.g. `LayerOverrides` in `FailurePredictor::predict_layer`
(per-layer exposure from a CTB file, overriding the recipe default). The distinction:

- **Legit overlay:** a genuinely different value for a specific occasion, defaulting to
  an already-well-placed baseline.
- **Anti-pattern overlay:** a workaround for a misplaced baseline.

`LayerOverrides` qualifies for (a) because the baseline (`resin.recipe`) is already in the
right place; the overlay represents a legitimate per-layer variation from that baseline.
The rejected alternative in ADR-0005 §3 qualifies as (b) because the baseline
(`printer.normal_exposure_sec`) is in the wrong aggregate.

## See also

- `docs/adr/0005-three-axis-printer-resin-recipe.md` "Rejected alternatives §3" — the
  in-plan rejection that motivated this KB entry
- `docs/patterns/entity-validate-on-mutation.md` — the aggregate integrity pattern that
  overlays undermine
