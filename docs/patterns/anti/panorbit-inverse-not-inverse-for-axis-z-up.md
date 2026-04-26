---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# Anti-pattern: bevy_panorbit_camera 0.34's `util::calculate_from_translation_and_focus` is not a true inverse of `update_orbit_transform` for non-default `axis` orders

## Tempting

When configuring a Z-up world via `PanOrbitCamera::axis = AXIS_Z_UP`
(`[Vec3::X, Vec3::Z, Vec3::Y]`), it is tempting to compute the initial
yaw / pitch by passing a desired camera position vector through the
crate's inverse helper:

```rust
let comp = Mat3::from_cols(axis[0], axis[1], axis[2]) * (translation - focus);
let yaw = comp.x.atan2(comp.z);
let pitch = (comp.y / radius).asin();
```

This is a verbatim mirror of `bevy_panorbit_camera-0.34/src/util.rs:5
calculate_from_translation_and_focus`.

## Why it's wrong

The helper is private (`mod util;`, not `pub mod util;`) AND, even when
mirrored locally, it does NOT round-trip through `update_orbit_transform`
for non-default axis orders. Trace `util.rs:138 above_z_as_up_axis`
test:

- input `translation = (0, 0, 5)`, `axis = AXIS_Z_UP` → returns `(yaw=0,
  pitch=π/2, radius=5)`.
- forward path `update_orbit_transform(yaw=0, pitch=π/2, radius=5)` with
  the same axis → `pitch_rot = Quat::from_axis_angle(X, -π/2)` applied
  to `(0, 0, 5)` gives `(0, 5, 0)`. Camera at `(0, 5, 0)` ≠ input
  `(0, 0, 5)`.

The forward and inverse use different conventions for the meaning of
pitch under a swapped axis. So even bypassing the privacy issue, the
mirror would land the camera at the wrong position.

## Pattern: use direct angles

For `axis = AXIS_Z_UP`, set yaw and pitch by their geometric meaning
directly:

```rust
pub fn three_quarter_yaw_pitch() -> (f32, f32) {
    let yaw = 45f32.to_radians();
    let pitch = -120f32.to_radians();  // 30° below horizon
    (yaw, pitch)
}
```

Pitch regime in this axis (verified empirically + by tracing
`update_orbit_transform`):

| `pitch`  | rotation about X | back-of-camera direction | camera position |
|----------|------------------|--------------------------|-----------------|
| 0°       | 0°               | `(0, 0, R)`              | overhead        |
| −45°     | +45°             | `(0, −0.71R, 0.71R)`     | 45° above horizon |
| −90°     | +90°             | `(0, −R, 0)`             | at horizon      |
| −120°    | +120°            | `(0, −0.87R, −0.5R)`     | 30° below horizon |
| −180°    | +180°            | `(0, 0, −R)`             | directly below  |

The crate's own `examples/swapped_axis.rs` sets `pitch: -45°` for an
above-horizon view, confirming the negative-pitch convention.

## Verification

A forward-application test in `resinsim-viz/src/main.rs` locks the
camera-offset coordinates the chosen yaw / pitch produce, so a future
upstream change that breaks this convention fails loudly:

```rust
#[test]
fn three_quarter_camera_lands_below_horizon_at_vat_level() {
    let (yaw, pitch) = three_quarter_yaw_pitch();
    let yaw_rot = Quat::from_axis_angle(AXIS_Z_UP[1], yaw);
    let pitch_rot = Quat::from_axis_angle(AXIS_Z_UP[0], -pitch);
    let camera_offset = (yaw_rot * pitch_rot) * Vec3::new(0.0, 0.0, 1.0);
    assert!((camera_offset.x - 0.612).abs() < 1e-2);
    assert!((camera_offset.y + 0.612).abs() < 1e-2);
    assert!((camera_offset.z + 0.500).abs() < 1e-2);  // -sin(30°)
}
```
