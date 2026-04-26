---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# Pattern: bundle related queries into a `SystemParam` struct when approaching Bevy's 16-param limit

## Context

Bevy 0.18 implements `IntoScheduleConfigs` for system-fn signatures up to
16 `SystemParam` arguments
(`bevy_ecs-0.18.1/src/system/system_param.rs:2242 all_tuples!(0, 16)`).
A 17th param compiles into a "trait `IntoScheduleConfigs` is not
implemented" error that's hard to diagnose because rustc only points at
the schedule-builder call site, not the offending system fn.

Issue 10 hit this when adding 4 new params (`prior_plate` query +
`active_profile` Res + `envelope` ResMut + warn-flag Local) to
`setup_initial_load` and `handle_dropped_files`, both of which already
had 13–14 params from earlier issues.

## Pattern

Group semantically related params into a `#[derive(SystemParam)]` struct:

```rust
use bevy::ecs::system::SystemParam;

#[derive(SystemParam)]
pub struct PriorGeometry<'w, 's> {
    pub stl: Query<'w, 's, Entity, With<LoadedStlMesh>>,
    pub slice: Query<'w, 's, Entity, With<LoadedSliceStack>>,
    pub cursor: Query<'w, 's, Entity, With<LayerCursor>>,
    pub plate: Query<'w, 's, Entity, With<BuildPlate>>,
}

fn handle_dropped_files(
    // ... 8 other params
    prior: PriorGeometry,    // counts as ONE SystemParam
    // ... 6 more params
) { ... }
```

Each field of the struct contributes its own world access (Bevy's borrow
checker still sees them individually for parallelism analysis), but the
struct counts as a single `SystemParam` for the schedule-config tuple.

## When to apply

- A system has > ~12 params and is likely to grow.
- Several params logically belong together (e.g. all four "prior geometry
  to despawn before re-spawn" queries).
- You hit the 16-param limit and rustc surfaces it as the cryptic
  `IntoScheduleConfigs` error at the schedule call site.

## When NOT to apply

- A system has < 10 params. The bundle adds indirection without
  necessity.
- The grouped params are unrelated. Bundling unrelated params just to
  hit a number is harder to reason about than the 17-param fn would be.

## Verification

The bundle is exercised by the existing tests of any system that uses
it. There's no separate "the SystemParam derive works" test — the
`#[derive(SystemParam)]` is upstream's contract.
