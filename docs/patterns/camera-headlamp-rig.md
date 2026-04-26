---
issue: 02-stl-mesh-rendering
date: 2026-04-26
---

# Pattern: camera headlamp rig via `with_children` + `Transform::default()`

## Context

Inspection-mode 3D viewers (Fusion 360, Meshmixer, PrusaSlicer,
MeshLab) light the model from the view direction so the visible side
is always lit, regardless of how the user has orbited the camera.
This avoids the "flat dark side" problem of a fixed directional
light when the user rotates around to the back.

Bevy's `Camera3d` and `DirectionalLight` both define their forward
direction as the entity's local `-Z` axis. If a `DirectionalLight`
is parented to a `Camera3d` with `Transform::default()` (identity),
Bevy's `TransformPropagate` system computes the light's world
Transform as the camera's world Transform every frame — so the light
direction always matches the view direction.

## Implementation

```rust
pub fn setup_scene(mut commands: Commands) {
    commands
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            PanOrbitCamera::default(),
            AmbientLight { brightness: 200.0, ..default() },  // fill for back faces
        ))
        .with_children(|cam| {
            cam.spawn((
                DirectionalLight {
                    illuminance: 10_000.0,
                    shadows_enabled: false,  // see "Shadows off" below
                    ..default()
                },
                Transform::default(),
            ));
        });
}
```

## Shadows off

A view-aligned directional light with shadows enabled produces
self-shadowing acne: every front-facing facet shadows the next, and
the GPU's shadow-map bias can't cleanly separate them. Disable
shadows on the headlamp; rely on diffuse + ambient for shape
readability.

If you want shadows for screenshots or final renders, add a
**second** directional light at a fixed offset (e.g. up-and-to-the-
right, key light) with shadows enabled, and keep the headlamp as a
fill. Don't try to enable shadows on the headlamp itself.

## Why this works (mechanism)

- `with_children(|cam| cam.spawn(...))` adds the spawned light as a
  child of the camera entity, inserting `ChildOf(cam_entity)` on the
  light and adding it to the camera's `Children` component.
- Bevy's `TransformPropagate` system runs in the `PostUpdate`
  schedule and walks the hierarchy from roots, computing each
  entity's `GlobalTransform` as `parent_global * local`. With
  `local = Transform::default()` (identity),
  `child_global == parent_global`.
- `bevy_panorbit_camera`'s update system writes the camera's
  Transform from its panorbit fields. The next `TransformPropagate`
  tick re-computes the light's `GlobalTransform` accordingly.
- `DirectionalLight` shaders use the light's `GlobalTransform` to
  derive its world-space direction (`-Z` of the rotation matrix).

## Locked in by tests

```rust
#[test]
fn directional_light_is_child_of_camera_for_headlamp() {
    let mut app = run_startup();
    let world = app.world_mut();
    let cam = world.query_filtered::<Entity, With<Camera3d>>()
        .iter(world).next().expect("Camera3d must exist");
    let mut q = world.query::<(&DirectionalLight, &ChildOf)>();
    assert!(
        q.iter(world).any(|(_, c)| c.parent() == cam),
        "DirectionalLight must be a child of Camera3d"
    );
}
```

The static parent-relationship check is sufficient as a regression
guard — TransformPropagate is a Bevy-built-in invariant, so we don't
need to dynamically verify the propagated world transform.

## See also

- `crates/resinsim-viz/src/main.rs::setup_scene` — first instance.
- `docs/patterns/anti/transform-on-plugin-driven-camera.md` —
  related anti-pattern (panorbit Transform writes).
- `docs/adr/0010-resinsim-viz-presentation-layer.md` — viz layering
  rule (no bevy in core).
