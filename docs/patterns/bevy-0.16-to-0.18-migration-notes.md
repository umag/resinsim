---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# Pattern: Bevy 0.16 → 0.18 API drifts encountered during 01-viz-crate-scaffold

## Context

The Phase 2 source plan
(`projects/000-global/research/resinsim-physics-simulation-plan.md`)
specified Bevy 0.16. At impl time `bevy_panorbit_camera 0.34` (the
chosen orbit-camera crate) requires `bevy ^0.18`, forcing a bump.
Three API renames between 0.16 and 0.18 affected the scaffold; future
viz work will hit the same boundary.

## The drifts

### EventWriter / Events → MessageWriter / Messages (Bevy 0.17)

```rust
// 0.16:
fn smoke_exit(mut writer: EventWriter<AppExit>) {
    writer.send(AppExit::Success);
}

// 0.18:
fn smoke_exit(mut writer: MessageWriter<AppExit>) {
    writer.write(AppExit::Success);
}
```

Both the type (`EventWriter` → `MessageWriter`) and the method
(`.send()` → `.write()`) changed. Bevy 0.17 distinguished
"buffered messages" (the renamed thing) from "observer-triggered
events" (kept as `EventWriter`). For one-shot signals like AppExit,
use `MessageWriter`.

`MessageWriter` is in `bevy::prelude::*`, no extra import needed.

### AmbientLight: Resource → per-camera Component (Bevy 0.18)

```rust
// 0.16: global resource
commands.insert_resource(AmbientLight {
    brightness: 0.2,
    ..default()
});

// 0.18: component on the camera
commands.spawn((
    Camera3d::default(),
    Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    AmbientLight { brightness: 200.0, ..default() },
));
```

Two consequences:
- The `AmbientLight` value moves out of `commands.insert_resource(...)`
  and into the camera's spawn tuple.
- The `brightness` units changed from a 0..1 multiplier to lux × intensity
  — order-of-magnitude shift. Empirically 200.0 looks like the old 0.2.
- World queries change accordingly:
  `world.contains_resource::<AmbientLight>()` →
  `world.query::<(&Camera3d, &AmbientLight)>().iter(world).next().is_some()`.

### Camera3dBundle removed (Bevy 0.13+, gone in 0.18)

```rust
// 0.16 deprecated, 0.18 removed:
commands.spawn(Camera3dBundle { transform: ..., ..default() });

// 0.18:
commands.spawn((
    Camera3d::default(),
    Transform::from_xyz(...).looking_at(Vec3::ZERO, Vec3::Y),
));
```

`*Bundle` types are gone. Spawn tuples of components instead.

## Pattern

Before pinning a Bevy minor version in a new viz-side crate:

1. Check the **canonical docs.rs page** of every camera/UI dep
   (panorbit, egui, gizmos, picking, …) for the `bevy ^N` constraint
   it ships against. Web-search summaries are not authoritative.
2. Pick the lowest Bevy version every dep accepts. If they disagree,
   bump the laggard or pick a different dep.
3. Run `cargo build` early — the "two versions of bevy_app in the
   dependency graph" error is the diagnostic signal.

## See also

- `crates/resinsim-viz/Cargo.toml` — pinned `bevy = "0.18"` after this
  research
- ADR-0010 — Bevy version + collateral API drifts section
- Anti-pattern `anti/web-search-version-compat-without-canonical-verification.md`
