---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
kind: pattern
---

# Pattern: Bevy bake-once mesh + cursor entity for per-layer affordances

## Context

Bevy 0.18's `Assets<Mesh>::get_mut(handle)` marks the asset Dirty;
the render world re-uploads the entire vertex buffer next extract.
For slice-stack meshes (~5000 layers × ~100k vertices each), that
re-upload is multi-millisecond and visibly stalls the frame on every
interaction.

The naive "update the mesh on layer change" approach (mutate
ATTRIBUTE_COLOR or visibility per-tick) fails this performance
budget.

## The pattern

Bake all per-vertex state (colours, layer indices, scalar overlays)
into the Mesh asset **at load time**. Never call `meshes.get_mut()`
on the asset after spawn. Per-layer affordances live on **separate
entities** whose `Transform` updates without touching the Mesh
asset.

```rust
// At load time: bake colours into the slice-stack mesh ONCE.
let mesh_handle = meshes.add(slice_stack_to_bevy_mesh(&layers, Some(&colors)));

// Cursor: a separate entity with its own Mesh + Material.
commands.spawn((
    Mesh3d(cursor_mesh),
    Transform::from_xyz(cx, cy, z_prefix[index] + LAYER_CURSOR_EPSILON_MM),
    LayerCursor,
));

// Update system mutates Transform, NOT the mesh.
fn update_layer_cursor(
    current: Res<CurrentLayer>,
    z_prefix: Res<LayerZPrefix>,
    mut cursor_q: Query<&mut Transform, With<LayerCursor>>,
) {
    if !current.is_changed() { return; }
    let z = z_prefix.0[current.index as usize] + LAYER_CURSOR_EPSILON_MM;
    for mut t in cursor_q.iter_mut() { t.translation.z = z; }
}
```

## Test contract

The bake-once invariant is structurally enforced (no `meshes.get_mut()`
calls in the codebase) but adversarial review caught the gap that
"structural enforcement" has no test guard. The contract is asserted
by capturing the slice-stack mesh's ATTRIBUTE_COLOR Vec before and
after a full ArrowUp/ArrowDown traversal and asserting byte-equality
plus `Assets<Mesh>::iter().count()` unchanged. See
`crates/resinsim-viz/src/main.rs::tests::slice_stack_mesh_attribute_color_unmutated_under_arrow_keys`.

## When to deviate

If a future affordance needs to mutate per-vertex state mid-session
(e.g., user-driven slice colouring), prefer a **second baked
attribute + a uniform** (custom Material with `clip_z: f32` or
`selected_layer: u32`) over per-tick `meshes.get_mut`. Issue
11-z-clip-cursor is the canonical extension.
