---
issue: 02-stl-mesh-rendering
date: 2026-04-26
---

# Anti-pattern: Writing `Transform` directly to a plugin-managed camera

## Context

Several Bevy camera crates — `bevy_panorbit_camera`,
`bevy_third_person_camera`, `bevy_dolly`, `bevy_blendy_cameras`, and
others — implement their behaviour by storing the camera's logical
state (focus point, orbit radius, yaw, pitch, follow target, …) in
fields on a custom `Component`, and then running an Update system
that writes `Transform` from those fields every frame.

When you also write `Transform` from your own code (e.g. "frame the
camera against the loaded STL's bounding box"), your write is silently
overwritten by the plugin's next tick. The visible behaviour: the
framing seems to take effect for one frame and then revert, or never
takes effect at all if the plugin's system runs first.

## Don't do this

```rust
// 02-stl-mesh-rendering, plan v1 (rejected by adversarial review)
fn frame_camera(bbox: &BoundingBox, mut q: Query<&mut Transform, With<Camera3d>>) {
    let centre = bbox.centre();
    let dist = 1.5 * bbox.diagonal();
    for mut t in q.iter_mut() {
        *t = Transform::from_translation(centre + Vec3::ONE.normalize() * dist)
            .looking_at(centre, Vec3::Y);
    }
}
```

The next time `bevy_panorbit_camera::PanOrbitCameraPlugin`'s update
system runs, it will recompute Transform from the camera's `focus`
(still `Vec3::ZERO` from default) and `radius` (still `None`),
reverting your framing.

## Do this

Drive the plugin's fields, not Transform:

```rust
fn fit_panorbit_to_bbox(cam: &mut PanOrbitCamera, bbox: &BoundingBox) {
    let centre = bbox.centre();
    let distance = 1.5 * bbox.diagonal();
    cam.focus = centre;
    cam.target_focus = centre;
    cam.radius = Some(distance);
    cam.target_radius = distance;
}
```

Setting both `focus` and `target_focus` (and similarly `radius` /
`target_radius`) skips the smoothing animation — the camera snaps to
the framing on the next frame.

## How to recognise the trap

If a camera-related crate exposes a custom Component with fields
named `focus`, `target`, `radius`, `distance`, `pitch`, `yaw`,
`follow`, etc., that's a strong signal that an Update system in the
plugin reads those fields and writes Transform. Verify by grepping
the crate's source for `&mut Transform` in system signatures.

## See also

- `docs/patterns/stl-to-bevy-mesh-flat-shaded.md` — concrete use of
  this principle in resinsim-viz.
- `docs/adr/0010-resinsim-viz-presentation-layer.md` — why the viz
  crate uses `bevy_panorbit_camera 0.34`.
