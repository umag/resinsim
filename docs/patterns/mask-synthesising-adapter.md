---
issue: suction-detector-raft-false-positive
date: 2026-04-21
---

# Pattern: Mask-synthesising adapter for narrow-input entry points

## Context

When a value object grows a new field, existing entry points that construct
the object without the field have two options:

1. **Require the new field.** Every caller updates to provide it, even when
   the caller doesn't care about the new field's semantics.
2. **Make it optional.** Everywhere downstream now handles `None`, and the
   "required-after-v2" contract erodes.

Both options leak the field's existence into every caller. Neither is
appropriate when the new field's *production* semantics are load-bearing
(can't be optional long-term) but test-fixture callers legitimately don't
care about it.

## Pattern

Introduce a thin **adapter entry point** that accepts the narrower input
(no new field) and synthesises a semantically-inert value for the new
field, then delegates to the canonical richer entry point. The adapter is
not a shim — it implements a meaningful transformation from a reduced
view (test fixture) to the full domain model.

In resinsim (suction-detector-raft-false-positive, 2026-04-21):

```rust
// Canonical: takes LayerInput including mask.
pub fn run_from_layer_inputs(layers: &[LayerInput], ...) { ... }

// Adapter: takes areas only; synthesises a fully-solid 1×1 LayerMask per
// area (semantically-inert for CavityDetector — zero void cells → zero
// events). Existing test fixtures using area arrays work unchanged.
pub fn run_from_areas(areas: &[CrossSectionArea], ...) -> ... {
    let masks: Vec<LayerMask> = (0..areas.len())
        .map(|_| LayerMask::new_all_solid(1, 1, voxel))
        .collect();
    run_inner(areas, &masks, ...)
}
```

The adapter's synthesised value must be **semantically inert** for all
consumers: here, a fully-solid mask produces zero `CavityEvent`s, which
is the correct behaviour for tests that don't exercise cavity detection.

## Anti-application: when NOT to use

- Do not use as a deprecation shim. If the adapter's synthesised value
  is a plausible real input (e.g. "empty list" or "zero default") that
  could hide a production bug, require the explicit input instead.
- Do not use if the synthesised value could change observable behaviour
  in downstream services (e.g. non-solid synthetic mask would produce
  phantom events).

## See also

- `resinsim-core/src/app/simulation_runner.rs::run_from_areas` — canonical
  example.
- `docs/patterns/phase-boundaries-for-ddd-refactors.md` — Phase B atomic
  commits sometimes use this pattern to avoid cascading scope.
