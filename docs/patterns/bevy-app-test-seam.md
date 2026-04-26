---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# Pattern: pub fn setup_* — testable seam for Bevy startup systems

## Context

Bevy systems registered as `app.add_systems(Startup, my_system)` are
hard to test directly: they take Bevy ECS system params (`Commands`,
`Res<…>`, `Query<…>`) and run inside the App's schedule, not as
free-callable functions.

A smoke test that runs the full binary verifies "the App constructs
without panicking" but cannot assert on the resulting World — what
entities were spawned, which components they carry, whether plugins
were registered. Future contributors can silently break a startup
system (drop a light, swap a plugin, skip a resource insert) and a
"does the binary not crash" test will keep passing.

## Pattern

Two coordinated pieces:

### 1. Extract the body of the startup system as a `pub fn`

```rust
pub fn setup_scene(mut commands: Commands) {
    commands.spawn((Camera3d::default(), /* ... */));
    commands.spawn((DirectionalLight { /* ... */ }, /* ... */));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup_scene)
        .run();
    Ok(())
}
```

The system body is unchanged — `Commands` parameter, normal spawn calls
— but it is now a callable, addressable function.

### 2. Unit-test it on a plugin-less App

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn run_startup() -> App {
        let mut app = App::new();
        app.add_systems(Startup, setup_scene);
        app.update();  // runs Startup once
        app
    }

    #[test]
    fn setup_scene_spawns_camera() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<&Camera3d>();
        assert_eq!(q.iter(world).count(), 1);
    }
}
```

`App::new()` with no plugins is sufficient: Startup systems run, no
rendering is attempted, no window is opened, no wgpu is initialised.
Tests pass on any CI provider, no display required, ~600ms overhead
per test.

## Why this is preferred to subprocess smoke tests

A subprocess test (`cargo run -p resinsim-viz -- --smoke-exit`) only
proves the binary builds and reaches AppExit. It cannot tell the
difference between "DirectionalLight was spawned" and "DirectionalLight
was silently removed by a refactor" — both pass the smoke test.

In-process tests with World queries make the regression visible
immediately: deleting `commands.spawn(DirectionalLight { ... })` flips
`setup_scene_spawns_directional_light` to RED.

## When to use

Every viz crate startup system that has more than one logical
deliverable (e.g. spawn this entity, register that plugin, insert
this resource). For purely-configurational systems
(`app.add_plugins(MyPlugin)` with no body), the subprocess smoke is
fine.

## Trade-off

The seam adds one level of indirection: `setup_scene` lives next to
`main` instead of being inlined. In exchange, the system gains a
testable surface and a stable name that test code addresses.

## Caveat: asset-touching tests need `AssetPlugin`

Tests that exercise systems touching `Assets<Mesh>`,
`Assets<StandardMaterial>`, etc., cannot be fully plugin-less. The
`init_asset::<T>()` helper requires an `AssetServer` resource, which
is only inserted by `bevy::asset::AssetPlugin`. Add the plugin
explicitly before calling `init_asset`:

```rust
let mut app = App::new();
app.add_plugins(bevy::asset::AssetPlugin::default())
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>();
```

This still avoids the windowing backend, the renderer, and any of the
heavyweight `DefaultPlugins` machinery — `AssetPlugin` alone is
lightweight and synchronous. First seen in
`crates/resinsim-viz/src/main.rs::tests::make_loader_app` (issue 02).

## See also

- `crates/resinsim-viz/src/main.rs` — first instance of this pattern
- ADR-0010 — `setup_scene` seam consequence
- Anti-pattern `anti/bevy-subprocess-smoke-test.md` — what NOT to do
