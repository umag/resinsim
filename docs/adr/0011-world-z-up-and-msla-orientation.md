---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# ADR-0011: World coordinate system is Z-up; build plate sits on top with model hanging upside-down (MSLA orientation)

## Status
Accepted

## Context

Issue 09 landed CTB voxel-mask-stack rendering in `resinsim-viz`. The
slice mesh emits vertex positions in CTB-native coordinates: X / Y on
the LCD pixel grid, Z accumulating layer heights from `0` at native
layer 0 to `Σ layer_height_um / 1000` at the top.

Two coordinate-system decisions were left implicit by issue 09 and need
to be fixed before further viz work lands:

1. **World up axis.** Bevy's defaults treat Y as world-up
   (`Transform::looking_at(target, Vec3::Y)`). The CTB convention
   treats Z as the build axis. Without an explicit choice, the
   slice-stack geometry appears tipped on its side when rendered with
   Bevy defaults.
2. **Plate / model orientation.** A real MSLA printer holds the build
   plate at the TOP of the build envelope; the print HANGS
   upside-down beneath it, attached at the first-printed layer
   (slicer layer 0) which becomes the print's top in space when
   removed. Slicer-style "model on plate, plate flat below" is the
   wrong mental model for an MSLA simulator.

## Decision

### 1. World-space is Z-up

`PanOrbitCamera` is configured with
`axis: [Vec3::X, Vec3::Z, Vec3::Y]`, the same axis order used by the
upstream `bevy_panorbit_camera` crate's Z-up tests
(`bevy_panorbit_camera-0.34/src/util.rs:73 AXIS_Z_UP`). This makes
yaw rotate around Z and pitch elevate from the XY plane.

**Scope.** This decision concerns world-space only. Bevy's view-space
and clip-space conventions (Y-up viewport, right-handed
[NDC](https://gpuweb.github.io/gpuweb/#coordinate-systems)) are
unchanged.

### 2. MSLA orientation: build plate at TOP, model hangs below

The build plate is positioned at world Z = `envelope.max_z`, with the
plate's bottom face (vat-side surface) facing DOWN at exactly that
value. The plate's top face (back of plate) is at
`envelope.max_z + thickness`.

The CTB slice mesh is rendered with a `LoadedSliceStack` entity
`Transform`:

```
Transform {
    translation: Vec3::new(0.0, 0.0, envelope.max_z),
    rotation: Quat::from_rotation_x(PI),
    scale: Vec3::ONE,
}
```

After this Transform, a vertex at native `(x, y, z_native)` maps to
world `(x, -y, envelope.max_z - z_native)`:

- Native `(x, y, 0)` (slicer layer 0, bottom of slicer-orientation
  model = first printed) → world `(x, -y, envelope.max_z)` — glued to
  plate's underside.
- Native `(x, y, mesh_max_z)` (slicer layer N, top of slicer
  orientation = last printed) → world
  `(x, -y, envelope.max_z - mesh_max_z)` — hanging at the lowest
  world Z.

**Y-axis flip side-effect.** The 180° rotation about X also negates
the Y coordinate (front/back swap). This is acceptable for v1: it
matches the physical reality that a finished print on an MSLA plate
IS upside-down vs the slicer view, and most prints are
roughly Y-symmetric. Asymmetric prints will appear front/back
mirrored — a known limitation; fix in a future issue if the visual
becomes a real-world problem.

### 3. Mesh data is preserved at the data layer

The slice mesh asset (vertex positions, normals, indices) is unchanged
from the issue 09 contract: vertices remain at native CTB coordinates
`(0..w*voxel_size, 0..h*voxel_size, 0..Σ heights)`. The orientation
change lives entirely on the `LoadedSliceStack` entity's `Transform`.

## Consequences

- **Camera.** Default initial view: `yaw = 45°`, `pitch = -120°`
  (= 30° below horizon, vat-level looking up at the hanging model).
  Frames the combined model + plate span on the first load only.
  Subsequent reloads preserve the user's orbit angle. `allow_upside_down`
  is enabled so the orbit can cross horizon without a soft stop.
- **STL.** STL meshes render at native coords with identity
  Transform — no auto-rotation, no plate anchor. Anchoring an STL
  like a CTB is a separate follow-up.
- **Printer envelope.** The plate's Z position depends on
  `envelope.max_z`. The envelope is sourced from
  `ActivePrinterProfile.build_envelope_mm` (ADR-0012); CTB header
  bed_size_mm + sentinel max_z; or a cold-start default. Priority is
  documented in `docs/patterns/world-z-up-msla-orientation.md`.
- **Free orbit.** No `pitch_lower_limit` / `pitch_upper_limit` —
  matches DragonFruit + leaves underside inspection available.

## Alternatives considered

- **Y-up world (Bevy default).** Rejected: forces every CTB-aware
  consumer to translate between conventions; the build axis is Z
  everywhere else in the codebase (slicer files, `LayerInput`,
  bounding boxes).
- **Plate at z=0, model above (DragonFruit-style).** Rejected: this
  is the FDM convention, not MSLA. A resin sim that displays models
  the wrong way up will mislead users about peel direction, suction
  forces, and lift kinematics.
- **Pure translate Transform (no flip).** Rejected: leaves model
  layer N at the top (touching plate), inverting layer ordering
  perception. The 180° rotation flips both layer ordering AND
  vertical orientation in one step, matching the print's physical
  state when ejected.
- **Reflection / negative scale.** Rejected: inverts winding order;
  lighting breaks. A pure rotation keeps normals correct.
