---
issue: 04-egui-control-panels
date: 2026-04-26
---

# Pattern: idempotent cache update gated on identity, not Resource freshness

## Context

Bevy's `Res<T>::is_changed()` fires on any `&mut` access to the
resource — including the system's own write. A naive cache-update
system written as:

```rust
fn refresh_loaded_profiles_system(
    mut state: ResMut<PickerState>,
    repos: Option<Res<ProfileRepos>>,
) {
    let Some(repos) = repos else { return };
    if state.is_changed() {
        state.loaded_resin = state
            .selected_resin
            .as_ref()
            .and_then(|n| repos.resin.load(n).ok());
    }
}
```

…ping-pongs forever: the system writes through `state`, that flips
`is_changed` true next frame, the system runs again, writes again.

## Pattern

Make the **cache body** idempotent — equal names mean no mutation —
and let `is_changed` serve only as an early-out hint, not a
correctness gate:

```rust
pub fn refresh_loaded_profiles(state: &mut PickerState, repos: &ProfileRepos) {
    let resin_needs = state.selected_resin.as_deref()
        != state.loaded_resin.as_ref().map(|r| r.name());
    if resin_needs {
        state.loaded_resin = match &state.selected_resin {
            Some(name) => repos.resin.load(name).ok(),
            None => None,
        };
    }
    // mirror for printer …
}
```

Now even if the system runs every frame, it only mutates when the
selected name differs from the cached name. No `is_changed`
ping-pong.

## How to verify

A unit test that runs the helper twice on the same input and
asserts the resulting names are equal pins the no-op-when-equal
contract:

```rust
let resin_before = state.loaded_resin.as_ref().map(|r| r.name().to_string());
refresh_loaded_profiles(&mut state, &shipped_repos());
let resin_after = state.loaded_resin.as_ref().map(|r| r.name().to_string());
assert_eq!(resin_before, resin_after);
```

## When to use

Any Bevy system that writes through `&mut Res<T>` to update a
"derived" field (cache, projection, lookup result) keyed on
another field of the same Resource. The general rule: derive
identity from the **input** field, gate the write on the
identity, not on freshness.

## First-party example

`crates/resinsim-viz/src/ui/state.rs::refresh_loaded_profiles`
(issue 04). Tests: `refresh_loaded_profiles_is_idempotent` +
`refresh_loaded_profiles_loads_on_selection_change`.

## See also

- ADR-0011 — egui control panels
- `docs/patterns/anti/per-frame-disk-io-in-egui-draw.md` — the
  motivating problem this cache pattern resolves
