---
issue: viz-v2-dashboard
date: 2026-05-12
---

# Pattern: Bevy 0.18 system-param-limit workaround — return value over `&mut Resource`

## Context

Bevy 0.18 systems cap at 16 `SystemParam`s. A function that's already
called by a 14- or 15-param system (typical for compound flows like
"load CTB then bake mesh then bake heatmap then spawn cursor") can't
gain another `ResMut<T>` without pushing every test closure that
wraps it over the limit too. The test closures are written as
explicit Bevy closures `|mut commands: Commands, mut foo: ResMut<T>,
…|`, so each callsite needs the same expanded param list.

In `resinsim-viz`, `load_ctb_into_world` already takes 20 args
(commands + assets + four `prior_*` queries + camera + 5 ResMuts +
config refs + exit writer + booleans). Adding `&mut LoadedSliceMasks`
to populate the slice-E layer-mask resource pushed every test
closure that registers `load_ctb_into_world` as a one-shot system
past the 16-param Bevy ceiling.

## Pattern

Make the function **return** the data it would have written, and
have the caller stash it. The caller is already a Bevy system with
the needed `ResMut` in scope; the inner function stays callable
without inheriting that param.

```rust
// Don't: pushes every caller closure past 16 params.
fn load_ctb_into_world(/* 20 args */, loaded_masks: &mut LoadedSliceMasks) { … }

// Do: returns the data; caller stashes.
fn load_ctb_into_world(/* 20 args */) -> Option<Vec<LayerInput>> {
    // … parse and bake …
    Some(layers_for_caller)
}

// Caller (already a Bevy system with `mut loaded_masks: ResMut<LoadedSliceMasks>`):
if let Some(parsed) = load_ctb_into_world(/* … */) {
    loaded_masks.layers = parsed;
}
```

Test closures invoking the function are unchanged — they just ignore
the return value or destructure with `let _ =`.

## When to use

- Function is already near or at the 16-param Bevy system limit.
- The new resource-write is one-shot per call (not woven through
  multiple internal branches that each need access).
- Tests register the function via `register_system(|args| f(args))`
  closures and would have to grow.

## When not to use

- The function writes the resource from many internal branches
  conditionally — collecting the writes for return-on-success
  becomes its own complexity.
- The caller doesn't already have the resource in scope.
- You're far from the param limit and explicitness of `&mut`
  makes the signature easier to read.

## See also

- `crates/resinsim-viz/src/main.rs::load_ctb_into_world` — the
  applied example: returns `Option<Vec<LayerInput>>` consumed by
  both `setup_initial_load` and `handle_dropped_files` callers
  to populate `LoadedSliceMasks`.
- `docs/patterns/system-param-bundle-for-16-param-limit.md` —
  alternative pattern (bundle multiple params into a `SystemParam`
  derive) for cases where the caller-side stash isn't viable.
