---
issue: 02-stl-mesh-rendering
date: 2026-04-26
---

# Pattern: STL → Bevy Mesh as a flat-shaded, vertex-tripled conversion

## Context

`resinsim-core::io::stl::load_stl` returns `Vec<Triangle>` where each
`Triangle` has three independent `[f32; 3]` vertices in millimetres. To
render that geometry in `resinsim-viz` (Bevy 0.18) we need a
`bevy::mesh::Mesh` carrying positions, normals, and indices in the
formats the GPU expects. STL files do not carry topology (no shared
vertex indices) and STL face normals are notoriously unreliable —
half the binary STLs in the wild have all-zero normals or normals that
disagree with the cross-product winding.

Beyond geometry, the camera needs to frame the mesh on load. The
viz crate uses `bevy_panorbit_camera 0.34`, whose plugin recomputes
the entity Transform every frame from its own `focus`/`target_focus`/
`radius`/`target_radius`/`yaw`/`pitch` fields — writing Transform
directly is a no-op.

ADR-0010 forbids any `bevy::*` dependency in `resinsim-core`, so the
conversion has to live in viz.

## Pattern

### 1. Vertex tripling, per-face cross-product normal

```rust
pub fn triangles_to_bevy_mesh(triangles: &[Triangle]) -> Mesh {
    let mut positions = Vec::with_capacity(3 * triangles.len());
    let mut normals   = Vec::with_capacity(3 * triangles.len());
    let mut indices   = Vec::with_capacity(3 * triangles.len());

    for (i, tri) in triangles.iter().enumerate() {
        let v0 = Vec3::from(tri.v0);
        let v1 = Vec3::from(tri.v1);
        let v2 = Vec3::from(tri.v2);
        let normal: [f32; 3] = (v1 - v0).cross(v2 - v0).normalize_or_zero().into();

        positions.extend_from_slice(&[tri.v0, tri.v1, tri.v2]);
        normals.extend_from_slice(&[normal, normal, normal]);
        indices.extend_from_slice(&[3*i as u32, 3*i as u32 + 1, 3*i as u32 + 2]);
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}
```

Three points per triangle, one face normal replicated to all three —
the canonical *flat shading* layout. No vertex sharing; adjacent
triangles get two distinct copies of the shared edge's endpoints, which
is exactly what flat shading needs to keep face normals from being
averaged into a smooth shading approximation.

The cross-product `(v1 − v0) × (v2 − v0)` ignores whatever was in the
STL's per-face normal slot. `normalize_or_zero()` instead of
`normalize()` is the unwrap-policy-compliant form (no panic on
degenerate triangles).

### 2. Drive the PanOrbitCamera, not the Transform

```rust
pub fn fit_panorbit_to_bbox(cam: &mut PanOrbitCamera, bbox: &BoundingBox) {
    let min = Vec3::from(bbox.min);
    let max = Vec3::from(bbox.max);
    let diagonal = (max - min).length();
    if diagonal < 1e-6 {
        cam.focus = Vec3::ZERO;
        cam.target_focus = Vec3::ZERO;
        cam.radius = Some(10.0);
        cam.target_radius = 10.0;
        return;
    }
    let centre = (min + max) * 0.5;
    let distance = 1.5 * diagonal;
    cam.focus = centre;
    cam.target_focus = centre;
    cam.radius = Some(distance);
    cam.target_radius = distance;
}
```

`focus` controls where the camera looks; `radius` is the orbit
distance. Setting both `focus`/`target_focus` and
`radius`/`target_radius` skips the smoothing animation — the camera
snaps to the framing on the next frame.

The mesh entity uses `Transform::default()` — we don't translate the
geometry to the world origin. The camera focus is set to the bbox
*centre*, so an off-origin mesh is framed correctly without a
translation hack that would just trade one footgun (centred camera vs
off-centre mesh) for another (z-fighting from large translations on
far-from-origin geometry).

### 3. Spawn-once / despawn-on-reload via a marker component

```rust
#[derive(Component)]
pub struct LoadedStlMesh;

pub fn load_stl_into_world(
    path: &Path,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior: &Query<Entity, With<LoadedStlMesh>>,
    camera: &mut Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    for entity in prior.iter() {
        commands.entity(entity).despawn();
    }
    // ... load, spawn (Mesh3d, MeshMaterial3d, Transform, LoadedStlMesh) ...
    for mut cam in camera.iter_mut() {
        fit_panorbit_to_bbox(&mut cam, &bbox);
    }
}
```

The marker component is the *only* state needed to find prior loads.
Both system entry points — Startup (`setup_initial_load`) and Update
(`handle_dropped_files`) — share the same helper, so the spawn/despawn
invariant ("at most one `LoadedStlMesh` after the helper runs") holds
regardless of which path triggered the load.

### 4. Multi-drop → last wins

`MessageReader<FileDragAndDrop>` yields events of three variants:
`DroppedFile`, `HoveredFile`, `HoveredFileCanceled`. The reader filters
to `DroppedFile`, takes the *last* path of the tick, and logs an
`info!` if more than one was dropped:

```rust
let dropped: Vec<PathBuf> = events
    .read()
    .filter_map(|e| match e {
        FileDragAndDrop::DroppedFile { path_buf, .. } => Some(path_buf.clone()),
        _ => None,
    })
    .collect();
if dropped.len() > 1 { info!("..."); }
if let Some(path) = dropped.last() { /* load */ }
```

Bounds the visible non-determinism: 5 simultaneous drops produce one
visible mesh (the 5th) and one log line naming it.

## Alternatives considered

- **Use `stl_io`'s per-face `normal` field directly.** Rejected: the
  field is unreliable in practice (zero normals are widespread; some
  exporters disagree with their own winding). The cross product is
  always self-consistent with the geometry we render.
- **Smooth shading via vertex sharing + averaged normals.** Rejected
  for v1: STL is faceted by definition; smooth normals would require
  per-vertex deduplication via a hash + averaging pass, and the
  resulting shading would lie about facet boundaries that the
  simulation cares about. Future Phase 2 issues that overlay per-face
  data (heatmaps, failure markers) need flat shading anyway.
- **Translate the mesh to the world origin via
  `Transform::from_translation(-bbox_centre)` and look at
  `Vec3::ZERO`.** Rejected: introduces a divergence between mesh
  location and camera focus that's easy to forget when wiring future
  systems (gizmos, picking, overlays) — they'd see the translated
  position, not the native one. Also risks z-fighting for
  far-from-origin meshes (CAD-exported STLs sometimes have origins at
  e.g. plate centres tens of mm off).
- **Set `Transform` on the camera entity instead of the
  PanOrbitCamera fields.** Doesn't work — verified against
  `bevy_panorbit_camera-0.34.0/src/lib.rs`: the plugin's update system
  recomputes Transform every frame from its own fields. This was the
  HIGH adversarial finding that drove plan v2.

## Consequences

- **Memory cost.** A 1M-triangle STL becomes 3M positions + 3M normals
  + 3M indices = 60 MB of vertex data on the CPU side, before GPU
  upload. Acceptable for a dev tool; revisit if an importer ever needs
  to handle huge prints.
- **No vertex sharing means no `Aabb` shortcut.** Bevy auto-computes
  the entity's `Aabb` from `ATTRIBUTE_POSITION` for frustum culling;
  vertex tripling triples the work but the result is correct.
- **Lighting may look "inside out" for non-CCW STLs.** Since we
  recompute normals from cross products, an STL with reversed winding
  produces inward-facing normals. The geometry still renders (Bevy's
  default `StandardMaterial` has back-face culling on, so the
  *non-culled* side will be the one with wrong-sign normals — visible
  as flat-dark instead of lit). Fix is an auto-flip heuristic on
  centroid-vs-vertex direction, deferred to a follow-up issue.

## See also

- `docs/adr/0010-resinsim-viz-presentation-layer.md` — viz layering
  (no bevy in core), Bevy 0.18 / PanOrbitCamera 0.34 choices.
- `docs/patterns/bevy-app-test-seam.md` — `pub fn setup_*` pattern;
  applies to `setup_scene` and `setup_initial_load` here. Tests for
  the loader path use `make_loader_app()` which adds
  `bevy::asset::AssetPlugin::default()` (required by `init_asset`)
  but no window backend.
- `docs/patterns/bevy-0.16-to-0.18-migration-notes.md` — Event →
  Message rename. `MessageReader<FileDragAndDrop>` is the 0.18
  spelling; `FileDragAndDrop` is registered via `add_message` in
  `bevy_window-0.18.1/src/lib.rs`.
- `docs/patterns/mac-trackpad-panorbit-config.md` — `PanOrbitCamera`
  configuration carried forward from issue 01.
