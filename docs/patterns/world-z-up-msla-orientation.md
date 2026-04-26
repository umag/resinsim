# Pattern: world Z-up + MSLA plate-on-top orientation in resinsim-viz

**Source:** issue 10-build-plate-and-volume-cube · **ADR:** 0011, 0012

The Bevy world for `resinsim-viz` is Z-up; the build plate sits at the
TOP of the printer envelope; the model hangs UPSIDE-DOWN below it,
glued to the plate's underside. This mirrors a real MSLA printer's
physical state at end-of-print and supersedes any FDM-style
"plate at the bottom, model on top" mental model.

## Why this shape

A resin slicer is not an FDM slicer. In MSLA the print attaches to
the plate's underside (the side facing the LCD/vat). When the print
is finished and the plate has retracted to the top of the envelope,
the model is hanging upside-down beneath the plate. That's the only
view of the model that is a true representation of the print as the
machine produces it; any other orientation is a re-interpretation by
the slicer UI.

Visualising failure modes (peel, suction, near-edge collisions, layer
delamination) requires the user to think in print orientation. v1
locks the print orientation in by default.

## Load-bearing API knobs

### `PanOrbitCamera` axis (Z-up)

`bevy_panorbit_camera 0.34` exposes an `axis: [Vec3; 3]` field on
`PanOrbitCamera` (`bevy_panorbit_camera-0.34/src/lib.rs:284`). Setting
`axis: [Vec3::X, Vec3::Z, Vec3::Y]` makes Z the world up axis.
`Transform::up()` is NOT the lever — `PanOrbitCamera` recomputes the
camera `Transform` every frame from `focus + yaw + pitch + radius`
(`lib.rs:584`), so any `looking_at(_, Vec3::Z)` is a no-op.

The constant lives in `resinsim-viz/src/main.rs`:

```rust
const AXIS_Z_UP: [Vec3; 3] = [Vec3::X, Vec3::Z, Vec3::Y];
```

It matches the upstream crate's `AXIS_Z_UP` test constant
(`bevy_panorbit_camera-0.34/src/util.rs:73`), which is the
authoritative reference for the axis order.

### Default 3/4 view (yaw + pitch)

The default initial view is `yaw = 45°`, `pitch = -120°` (= 30° below
horizon — vat-level looking up at the model hanging from the plate).
Set directly:

```rust
pub fn three_quarter_yaw_pitch() -> (f32, f32) {
    let yaw = 45f32.to_radians();
    let pitch = -120f32.to_radians();
    (yaw, pitch)
}
```

**Why direct angles, not `util::calculate_from_translation_and_focus`.**
The crate's inverse helper is private (`mod util;`, not `pub mod util;`),
AND it isn't a true inverse of `update_orbit_transform` for `AXIS_Z_UP`
in `bevy_panorbit_camera 0.34`. Verifiable by tracing the upstream
`util.rs:138 above_z_as_up_axis` test: input translation `(0, 0, 5)`
round-trips to `(yaw=0, pitch=π/2, radius=5)`, but the forward path
applied to that produces camera `(0, R, 0)` ≠ input. So even with the
helper exported, mirroring it would land us at the wrong camera
position. Direct angles avoid the mismatch.

**Pitch regime in `axis = AXIS_Z_UP`.** `update_orbit_transform`
applies `Quat::from_axis_angle(axis[0], -pitch)` to the back-of-camera
direction `(0, 0, R)`:

| `pitch` | rotation about X | back-of-camera direction | camera relative to focus |
|---------|------------------|--------------------------|--------------------------|
| 0°       | 0°    | `(0, 0, R)`         | directly overhead         |
| −45°     | +45°  | `(0, −0.71R, 0.71R)` | 45° above horizon         |
| −90°     | +90°  | `(0, −R, 0)`         | at horizon                |
| −120° (default) | +120° | `(0, −0.87R, −0.5R)` | 30° below horizon (vat-level) |
| −180°    | +180° | `(0, 0, −R)`         | directly below             |

Yaw rotation about `axis[1] = Z` then composes on top.

The `swapped_axis` example in the upstream crate
(`bevy_panorbit_camera-0.34/examples/swapped_axis.rs`) sets
`pitch: -45°` for an above-horizon view, confirming the negative-pitch
convention.

### `preserve_view: bool` on `fit_panorbit_to_bbox`

`fit_panorbit_to_bbox` accepts a `preserve_view: bool`:

- `false` (Startup / first-load): re-locks yaw/pitch to the 3/4
  default along with focus + radius.
- `true` (drag-drop reload): only writes focus + radius; yaw/pitch
  are preserved so the user's current orbit angle survives.

Mirrors DragonFruit's `preserveCurrentViewDirection` flag
(`CameraIntroController.tsx:103`).

### Mesh anchor `Transform` (180° X-rotation + lift)

```rust
pub fn ctb_anchor_transform(envelope: &PrinterEnvelope) -> Transform {
    Transform {
        translation: Vec3::new(0.0, envelope.depth_mm, envelope.max_z_mm),
        rotation: Quat::from_rotation_x(std::f32::consts::PI),
        scale: Vec3::ONE,
    }
}
```

After rotation about X (`y → -y`, `z → -z`) and translation
`(0, envelope.depth_mm, envelope.max_z_mm)`, native vertex
`(x, y, z)` lands at world `(x, envelope.depth_mm - y,
envelope.max_z_mm - z)`. Native layer 0 (slicer "bottom" = first
printed) glues to the plate's underside at world
`z = envelope.max_z_mm`; native layer N hangs at the lowest world Z.
Mesh data (vertex positions) is unchanged — issue 09 contract
preserved at the data layer; only the entity `Transform` applies
the flip + anchor.

**Y-axis flip side-effect.** The 180° rotation about X also negates
Y (front/back swap). For symmetric prints (e.g. a cube fixture) it's
invisible. Asymmetric prints will appear front/back mirrored — a
known limitation that matches the physical reality of an MSLA print
on the plate. Out of v1 scope to address.

## Build envelope priority chain

`PrinterEnvelope` (a viz resource) is sourced through:

1. `ActivePrinterProfile.0.as_ref().and_then(|p| p.build_envelope_mm())` — profile wins if present.
2. CTB header `SlicedFileInfo.bed_size_mm` for X/Y + sentinel
   `max_z = 200 mm` if no profile envelope.
3. Cold-start default `192 × 120 × 200 mm` if neither.

`scene::resolve_envelope_after_ctb_load` reconciles this on every
CTB load; a one-shot warn fires (`Local<bool>` warn-once) when the
profile XY disagrees with the CTB header by more than
`ENVELOPE_MISMATCH_TOLERANCE_MM` (0.5 mm).

## What this v1 does NOT include

The following are intentionally out of scope (focus is sim + data;
the printer-grounded scene is supporting framework). All are
candidates for follow-up issues:

- Build-volume wireframe cube (envelope outline as gizmo lines).
- RGB axis arrows at the origin corner.
- FRONT marker / plate logo / hazard stripes / theme switching.
- Rounded plate corners / FRONT tab geometry from DragonFruit.
- Automated screenshot regression tests.
- Anchoring STL meshes like CTBs (STL ships with identity Transform
  in v1; if a CTB-style anchor is wanted, add a `--stl-up=z` flag in
  a follow-up).
- Pitch limits on `PanOrbitCamera` (v1 explicitly allows free orbit
  including underside inspection).

## Future overlay points

When subsequent viz issues land, they plug into this v1 frame:

- **Issue 03 — per-layer heatmap overlay.** The slice mesh's anchor
  `Transform` already places layers in world Z; a heatmap shader can
  read the local-Z fragment and map to a colour gradient.
- **Issue 04 — egui control panels.** The orbit camera + plate are
  scene-side; egui panels render in screen space and don't compete
  with the 3/4 view.
- **Issue 05 — layer timeline chart.** Reads layer indices from the
  slice mesh; the world-Z position of each layer is recoverable via
  `envelope.max_z_mm - native_z` (see anchor formula above).
- **Issue 06 — failure marker entities.** Spawn at world coords
  computed from native voxel coords through `ctb_anchor_transform`.
  Marker billboards / sprites need to compensate for the X-axis
  rotation if they want to face up.
- **Issue 07 — domain view modes.** Switching between
  "print orientation" and "slicer orientation" is a Transform-only
  change at the entity level — the mesh data is identical.

## Anti-patterns to avoid

- **`looking_at(target, Vec3::Z)` for Z-up.** No-op against
  `PanOrbitCamera`'s per-frame recompute. Use `axis: [X, Z, Y]`.
- **Required `build_envelope_mm` on `PrinterProfile`.** Forces every
  profile to populate it; blocks the human's "skip athena_ii"
  direction. The optional shape (ADR-0012) is the load-bearing
  decision.
- **Reflection / negative scale on the mesh anchor.** Inverts winding
  order; lighting breaks. A pure 180° rotation keeps normals correct.
- **Pure translate (no rotation) for the mesh anchor.** Leaves layer
  ordering perceptually inverted (layer N at top instead of bottom)
  and contradicts the "model hangs upside-down from plate" reality.
