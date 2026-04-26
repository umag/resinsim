---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# Pattern: Mac trackpad config for bevy_panorbit_camera

## Context

`bevy_panorbit_camera 0.34` ships two trackpad behaviours behind
`PanOrbitCamera::trackpad_behavior`:

- `TrackpadBehavior::Default` — mouse-style: vertical scroll = zoom.
  Feels wrong on a Mac trackpad where users expect two-finger drag to
  rotate (matching Blender, FreeCAD, PrusaSlicer).
- `TrackpadBehavior::BlenderLike { modifier_pan, modifier_zoom }` —
  Blender-style: scroll/two-finger drag = orbit, `modifier_pan` key
  enables pan, `modifier_zoom` enables zoom.

Pinch-to-zoom (the macOS native gesture) is gated separately on
`trackpad_pinch_to_zoom_enabled: bool` and defaults to `false`.

## Pattern

For a Mac-first viz crate, configure both:

```rust
use bevy_panorbit_camera::{PanOrbitCamera, TrackpadBehavior};

commands.spawn((
    Camera3d::default(),
    Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    PanOrbitCamera {
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan: None,    // no modifier needed for two-finger pan
            modifier_zoom: None,   // no modifier needed for pinch zoom
        },
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },
));
```

Result on macOS:
- Two-finger drag → orbits camera
- Pinch (two-finger spread/squeeze) → zooms
- Modifier-free → no chord required

## Regression-guard test

Lock the configuration as a permanent test so a future contributor
who copy-pastes a Bevy default doesn't silently flip behaviour:

```rust
#[test]
fn panorbit_uses_blender_trackpad_behavior() {
    let mut app = App::new();
    app.add_systems(Startup, setup_scene);
    app.update();
    let world = app.world_mut();
    let mut q = world.query::<&PanOrbitCamera>();
    let cam = q.iter(world).next().expect("PanOrbitCamera");
    assert!(matches!(cam.trackpad_behavior, TrackpadBehavior::BlenderLike { .. }));
    assert!(cam.trackpad_pinch_to_zoom_enabled);
}
```

## Trade-off

Users on Linux/Windows with a mouse-only setup get Blender-style
controls instead of Default. For the current dev workstation
(macOS-only) this is the right call; if the audience widens, expose
behaviour as a config flag rather than flipping the default for
everyone.

## Bevy native gesture events (alternative, not used here)

Bevy 0.18 exposes `PinchGesture`, `RotationGesture`, `PanGesture`,
`DoubleTapGesture` events on macOS/iOS via `bevy::input::gestures`.
`bevy_panorbit_camera` already abstracts over them; consume the raw
events directly only if you need rotation or double-tap (panorbit
doesn't expose those).
