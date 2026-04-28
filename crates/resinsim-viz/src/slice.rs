//! CTB slice-stack → Bevy mesh conversion.
//!
//! Lives in `resinsim-viz` (presentation layer per ADR-0010): no
//! `bevy::*` types may flow into `resinsim-core`. `LayerInput`,
//! `LayerMask`, and `BoundingBox` flow the other way, from core into viz.
//!
//! # Algorithm (two passes)
//!
//! **Pass 1 — Dim validation.** All mask-bearing layers must share
//! `(width_cells, height_cells, voxel_size_mm)` with the first
//! mask-bearing layer. Mismatch → empty mesh + `warn!`. All-None →
//! empty mesh.
//!
//! **Pass 2 — Z prefix sum (f64) + emission with immediate-neighbour
//! face culling.** Per voxel, emit the six axis-aligned faces only
//! where the immediate neighbour is empty. None layers render as void:
//! they emit no voxels themselves AND expose surrounding ±Z faces. This
//! is the load-bearing `mask-synthesising-adapter` contract — viz never
//! synthesises a solid mask for a layer the parser left as None.
//!
//! Z prefix sums accumulate in f64 (cast to f32 only at vertex
//! emission). f32 alone drifts ~50 µm over 4500 layers at 20 µm.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use resinsim_core::io::sliced::LayerInput;
use resinsim_core::io::stl::BoundingBox;
use resinsim_core::values::LayerMask;

/// ECS marker for the entity holding the currently-loaded slice stack,
/// carrying the source CTB path so the Run pipeline can re-parse it.
///
/// Used by the loader systems to find prior loads (despawn-before-load),
/// to enforce mutual exclusion with `LoadedStlMesh` (one geometry source
/// at a time in v1), AND by `apply_run_request` (sim.rs) to obtain the
/// path for `SimulationRunner::run_from_layer_inputs` via
/// `ctb::parse_ctb`. Path provenance lives on the marker so the world
/// always knows which file Run will consume — see ADR-0011.
#[derive(Component)]
pub struct LoadedSliceStack {
    pub path: std::path::PathBuf,
}

/// Single-source the canonical-dims lookup for the validate / bbox
/// helpers. Returns the first mask-bearing layer's
/// `(width_cells, height_cells, voxel_size_mm)`, or `None` for an
/// all-None / empty stack.
fn first_mask_dims(layers: &[LayerInput]) -> Option<(u32, u32, f32)> {
    for layer in layers {
        if let Some(mask) = layer.mask.as_ref() {
            return Some((
                mask.width_cells(),
                mask.height_cells(),
                mask.voxel_size_mm(),
            ));
        }
    }
    None
}

/// Cumulative-Z prefix sum (mm) over the layer stack, returned as a
/// length-`n+1` vector. Element `i` is the bottom-Z of layer `i`;
/// element `n` is the top-Z of the topmost layer (== total stack
/// height). Empty input returns `vec![0.0]`.
///
/// **Why f64 internally.** f32 alone drifts ~50 µm over 4500 layers at
/// 20 µm because the mantissa step at 90 mm magnitude is ~5 µm,
/// amplified by cumulative summation. The single f64 fold pre-empts
/// this; values are narrowed to f32 only at the vector boundary because
/// downstream consumers (Bevy mesh vertices, PanOrbitCamera bbox) are
/// f32-typed. See `docs/patterns/voxel-mask-stack-to-bevy-mesh.md` —
/// the f64-prefix-sum invariant must survive any future tidying pass.
///
/// **Why public.** `slice_stack_bounding_box` and
/// `slice_stack_to_bevy_mesh` consume this internally; the heatmap
/// layer-cursor system in `main.rs` also reads it to position a cursor
/// entity at `z_prefix[current_layer]`. Single source of truth across
/// all three call sites.
pub fn cumulative_z_mm(layers: &[LayerInput]) -> Vec<f32> {
    let mut prefix: Vec<f32> = Vec::with_capacity(layers.len() + 1);
    prefix.push(0.0);
    let mut acc: f64 = 0.0;
    for layer in layers {
        acc += layer.layer_height_um as f64 / 1000.0;
        prefix.push(acc as f32);
    }
    prefix
}

/// Compute the bounding box of a slice stack in physical mm.
///
/// X/Y come from the first mask-bearing layer's grid dims
/// (`width_cells * voxel_size_mm`, `height_cells * voxel_size_mm`).
/// Z accumulates `Σ layer_height_um / 1000` in f64, narrowed to f32 at
/// the boundary. Origin is the bed corner `(0, 0, 0)`.
///
/// Empty / all-None input returns the same INF/NEG_INF sentinel shape
/// as `stl::bounding_box(&[])`. The existing `bbox-degeneracy-guard`
/// pattern in `fit_panorbit_to_bbox` handles that case without extra
/// branches here.
pub fn slice_stack_bounding_box(layers: &[LayerInput]) -> BoundingBox {
    let Some((w, h, voxel_size_mm)) = first_mask_dims(layers) else {
        return BoundingBox {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
    };
    let max_x = w as f32 * voxel_size_mm;
    let max_y = h as f32 * voxel_size_mm;
    // Last prefix-sum entry == total stack height. Empty layers vec is
    // covered above by `first_mask_dims` returning None; the path here
    // always has ≥ 1 layer so `last()` is Some.
    let max_z = *cumulative_z_mm(layers)
        .last()
        .expect("cumulative_z_mm always returns at least one element");
    BoundingBox {
        min: [0.0, 0.0, 0.0],
        max: [max_x, max_y, max_z],
    }
}

/// Convert a slice of `LayerInput` into a flat-shaded Bevy `Mesh` via
/// face-culling boundary-quad emission, optionally with per-layer
/// vertex colours baked into `Mesh::ATTRIBUTE_COLOR`.
///
/// Each emitted face becomes 2 triangles = 6 unique vertex positions
/// with one face normal replicated, and sequential indices. Topology
/// is `TriangleList`; render-asset usage is `default()` — same shape
/// as `mesh::triangles_to_bevy_mesh`.
///
/// Mask-None layers render as void: they emit no voxels themselves
/// AND expose the ±Z faces of any mask-bearing neighbour. Mismatched
/// dims across layers fail soft (empty mesh + `warn!`), detected in a
/// discrete first pass before any vertex emission.
///
/// # Per-layer colours (heatmap support)
///
/// `colors` is the heatmap input — exactly one RGBA per layer (in the
/// same order as `layers`). When `Some(c)` and `c.len() == layers.len()`,
/// every voxel face emitted from layer `i` carries colour `c[i]` on all
/// six of its vertices, and the resulting `Mesh` has
/// `Mesh::ATTRIBUTE_COLOR` (`VertexFormat::Float32x4`, attribute index 5
/// per `bevy_mesh-0.18.1::mesh.rs:316`) set.
///
/// Bevy 0.18's `StandardMaterial` pipeline detects `ATTRIBUTE_COLOR`
/// and sets the `VERTEX_COLORS` shader_def
/// (`bevy_pbr-0.18.1::render::mesh.rs:2415`); the PBR fragment shader
/// then **replaces** `base_color` with the vertex colour
/// (`pbr_fragment.wgsl:54-55`: `pbr_input.material.base_color = in.color;`).
/// Caller can therefore use `StandardMaterial::default()` at the spawn
/// site — no `base_color` tweak required.
///
/// Mismatched length (`colors.len() != layers.len()`) emits one `warn!`
/// and the resulting mesh has NO `ATTRIBUTE_COLOR` (white fallback).
/// `None` skips the colour buffer entirely (zero overhead).
///
/// **Bake-once contract.** The colour buffer is baked into the Mesh
/// asset at build time and MUST NOT be mutated afterwards by any
/// system. Layer-change is achieved via a separate cursor entity whose
/// `Transform.translation.z` updates each tick — no
/// `Assets<Mesh>::get_mut()` on this mesh handle. Honouring this
/// contract is what satisfies the issue's "Update on layer change
/// without re-uploading the mesh" constraint.
pub fn slice_stack_to_bevy_mesh(layers: &[LayerInput], colors: Option<&[[f32; 4]]>) -> Mesh {
    // Pass 1 — dim validation. Returns early on all-None or mismatch.
    let Some((w, h, voxel_size_mm)) = first_mask_dims(layers) else {
        return empty_mesh();
    };
    for (i, layer) in layers.iter().enumerate() {
        if let Some(mask) = layer.mask.as_ref()
            && (mask.width_cells() != w
                || mask.height_cells() != h
                || mask.voxel_size_mm() != voxel_size_mm)
        {
            warn!(
                "slice_stack: layer {i} mask dims ({}×{}@{}) differ \
                 from canonical ({w}×{h}@{voxel_size_mm}); emitting empty mesh",
                mask.width_cells(),
                mask.height_cells(),
                mask.voxel_size_mm()
            );
            return empty_mesh();
        }
    }

    // Validate colour-buffer length. On mismatch, drop the buffer and
    // warn so the caller's hard-error policy (in main.rs) can take
    // precedence — viz-side soft fallback is the second line of defence.
    let effective_colors: Option<&[[f32; 4]]> = match colors {
        Some(c) if c.len() == layers.len() => Some(c),
        Some(c) => {
            warn!(
                "slice_stack: per-layer colors length {} differs from \
                 layers length {}; emitting mesh without ATTRIBUTE_COLOR",
                c.len(),
                layers.len()
            );
            None
        }
        None => None,
    };
    let mut colors_buf: Option<Vec<[f32; 4]>> = effective_colors.map(|_| Vec::new());

    // Pass 2 — Z prefix sum (f64-internal, length n+1) + emission with
    // immediate-neighbour face culling.
    let n = layers.len();
    let z_prefix = cumulative_z_mm(layers);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for i in 0..n {
        let Some(mask) = layers[i].mask.as_ref() else {
            continue;
        };
        let z0 = z_prefix[i];
        let z1 = z_prefix[i + 1];
        // Per-layer colour: when colours are active, this is c[i] which
        // gets replicated 6× per face below.
        let layer_color = effective_colors.map(|c| c[i]);

        for cy in 0..h {
            for cx in 0..w {
                if !mask.is_solid(cx, cy) {
                    continue;
                }
                let x0 = cx as f32 * voxel_size_mm;
                let x1 = x0 + voxel_size_mm;
                let y0 = cy as f32 * voxel_size_mm;
                let y1 = y0 + voxel_size_mm;

                // -X face exposed iff cx == 0 OR neighbour cell is empty.
                if cx == 0 || !mask.is_solid(cx - 1, cy) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [-1.0, 0.0, 0.0],
                        [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
                // +X face.
                if cx + 1 == w || !mask.is_solid(cx + 1, cy) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [1.0, 0.0, 0.0],
                        [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
                // -Y face.
                if cy == 0 || !mask.is_solid(cx, cy - 1) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, -1.0, 0.0],
                        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
                // +Y face.
                if cy + 1 == h || !mask.is_solid(cx, cy + 1) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 1.0, 0.0],
                        [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
                // -Z face: i==0 OR previous layer's mask is None OR
                // previous layer's voxel at (cx, cy) is empty.
                let neg_z_exposed = i == 0 || z_face_void(layers, i - 1, cx, cy);
                if neg_z_exposed {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 0.0, -1.0],
                        [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
                // +Z face.
                let pos_z_exposed = i + 1 == n || z_face_void(layers, i + 1, cx, cy);
                if pos_z_exposed {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 0.0, 1.0],
                        [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
                    );
                    push_face_color(colors_buf.as_mut(), layer_color);
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices));
    if let Some(buf) = colors_buf {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, buf);
    }
    mesh
}

/// Push 6 copies of `color` into the colour buffer, in lockstep with a
/// preceding `push_quad` that emitted 6 vertices. No-op if either the
/// buffer or the colour is absent — the two are always populated
/// together in the heatmap path, but the helper guards both for
/// readability at the call site.
fn push_face_color(buf: Option<&mut Vec<[f32; 4]>>, color: Option<[f32; 4]>) {
    if let (Some(b), Some(c)) = (buf, color) {
        for _ in 0..6 {
            b.push(c);
        }
    }
}

/// `true` iff the neighbour at `layers[idx]` does NOT cover (cx, cy).
/// None mask → void (true). Some mask but cell not solid → void (true).
/// This is the load-bearing `mask-synthesising-adapter` contract on the
/// viz side: viz never synthesises a solid mask for a None layer.
fn z_face_void(layers: &[LayerInput], idx: usize, cx: u32, cy: u32) -> bool {
    layers[idx]
        .mask
        .as_ref()
        .is_none_or(|m| !m.is_solid(cx, cy))
}

fn empty_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
    .with_inserted_indices(Indices::U32(Vec::new()))
}

/// Append one axis-aligned quad as 2 triangles (6 vertices, 6 indices)
/// to the mesh buffers. `corners` is winding-ordered so the cross
/// product matches `normal`.
fn push_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    normal: [f32; 3],
    corners: [[f32; 3]; 4],
) {
    let base = positions.len() as u32;
    // Triangle 1: c0, c1, c2.
    positions.push(corners[0]);
    positions.push(corners[1]);
    positions.push(corners[2]);
    // Triangle 2: c0, c2, c3.
    positions.push(corners[0]);
    positions.push(corners[2]);
    positions.push(corners[3]);
    for _ in 0..6 {
        normals.push(normal);
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base + 3, base + 4, base + 5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn empty_mask_layer(layer_height_um: f32, w: u32, h: u32, voxel: f32) -> LayerInput {
        let mask = LayerMask::new(w, h, voxel)
            .expect("LayerMask::new accepts positive dims and positive voxel size");
        LayerInput::new(0, 0.0, 1.0, 60.0, layer_height_um, 0.0)
            .expect("LayerInput::new accepts non-negative area and positive exposure")
            .with_mask(mask)
    }

    fn solid_mask_layer(layer_height_um: f32, w: u32, h: u32, voxel: f32) -> LayerInput {
        let mask = LayerMask::new_all_solid(w, h, voxel)
            .expect("LayerMask::new_all_solid accepts positive dims and positive voxel size");
        LayerInput::new(
            0,
            (w * h) as f64 * (voxel as f64).powi(2),
            1.0,
            60.0,
            layer_height_um,
            0.0,
        )
        .expect("LayerInput::new accepts non-negative area and positive exposure")
        .with_mask(mask)
    }

    fn no_mask_layer(layer_height_um: f32) -> LayerInput {
        LayerInput::new(0, 0.0, 1.0, 60.0, layer_height_um, 0.0)
            .expect("LayerInput::new accepts non-negative area and positive exposure")
    }

    fn positions_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("slice_stack_to_bevy_mesh always inserts ATTRIBUTE_POSITION")
        {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            other => panic!("expected Float32x3 positions, got {other:?}"),
        }
    }

    fn normals_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .expect("slice_stack_to_bevy_mesh always inserts ATTRIBUTE_NORMAL")
        {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            other => panic!("expected Float32x3 normals, got {other:?}"),
        }
    }

    fn indices_of(mesh: &Mesh) -> Vec<u32> {
        match mesh
            .indices()
            .expect("slice_stack_to_bevy_mesh always inserts indices")
        {
            Indices::U32(v) => v.clone(),
            Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
        }
    }

    #[test]
    fn empty_input_yields_empty_mesh() {
        let mesh = slice_stack_to_bevy_mesh(&[], None);
        assert!(positions_of(&mesh).is_empty());
        assert!(normals_of(&mesh).is_empty());
        assert!(indices_of(&mesh).is_empty());
    }

    #[test]
    fn single_solid_voxel_yields_12_triangles() {
        // 1×1 mask with cell (0,0) solid, one layer 100µm thick at 0.5mm voxel.
        let layers = vec![solid_mask_layer(100.0, 1, 1, 0.5)];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        let positions = positions_of(&mesh);
        let normals = normals_of(&mesh);
        let indices = indices_of(&mesh);
        assert_eq!(positions.len(), 36, "6 faces × 2 triangles × 3 vertices");
        assert_eq!(normals.len(), 36);
        assert_eq!(indices.len(), 36);

        // Collect unique normal directions; expect exactly the canonical six axis-aligned units.
        let mut unique: Vec<[i32; 3]> = normals
            .iter()
            .map(|n| {
                [
                    n[0].round() as i32,
                    n[1].round() as i32,
                    n[2].round() as i32,
                ]
            })
            .collect();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            6,
            "expected six unique face normals, got {unique:?}"
        );
        for n in &unique {
            assert_eq!(
                n[0].abs() + n[1].abs() + n[2].abs(),
                1,
                "normals must be axis-aligned ±X/±Y/±Z"
            );
        }
    }

    #[test]
    fn two_layer_solid_2x2_culls_interior_faces() {
        // Two layers, each 2×2 all-solid. Interior +Z face of layer 0 and
        // -Z face of layer 1 are culled — should be 144 positions, NOT 288.
        let layers = vec![
            solid_mask_layer(100.0, 2, 2, 0.5),
            solid_mask_layer(100.0, 2, 2, 0.5),
        ];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        assert_eq!(positions_of(&mesh).len(), 144);
    }

    #[test]
    fn none_mask_layer_renders_as_void_between_shells() {
        // [Some(2×2 solid), None, Some(2×2 solid)]. With the immediate-
        // neighbour rule, layer 0's +Z face is exposed (layer 1 is None
        // → map_or(true, ...) true-branch) and layer 2's -Z face is
        // exposed. Two separate 2×2×1 closed shells = 192 positions.
        let layers = vec![
            solid_mask_layer(100.0, 2, 2, 0.5),
            no_mask_layer(100.0),
            solid_mask_layer(100.0, 2, 2, 0.5),
        ];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        assert_eq!(
            positions_of(&mesh).len(),
            192,
            "two separate shells: 16 voxel-faces × 6 verts × 2 layers"
        );
    }

    #[test]
    fn mismatched_dims_yields_empty_mesh_first_pass() {
        // Layer 0 = 2×2 solid, layer 1 = 3×3 solid. Pass 1 detects mismatch
        // and returns empty mesh; emission never runs.
        let layers = vec![
            solid_mask_layer(100.0, 2, 2, 0.5),
            solid_mask_layer(100.0, 3, 3, 0.5),
        ];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        assert_eq!(positions_of(&mesh).len(), 0);
    }

    #[test]
    fn bounding_box_matches_grid_dims_and_cumulative_z() {
        // 4×3 mask, 0.5mm voxel, 100µm layers × 5 layers.
        let layers: Vec<LayerInput> = (0..5).map(|_| empty_mask_layer(100.0, 4, 3, 0.5)).collect();
        let bbox = slice_stack_bounding_box(&layers);
        assert!(
            (bbox.min[0] - 0.0).abs() < 1e-6,
            "min[0] = {} not 0",
            bbox.min[0]
        );
        assert!(
            (bbox.min[1] - 0.0).abs() < 1e-6,
            "min[1] = {} not 0",
            bbox.min[1]
        );
        assert!(
            (bbox.min[2] - 0.0).abs() < 1e-6,
            "min[2] = {} not 0",
            bbox.min[2]
        );
        assert!(
            (bbox.max[0] - 2.0).abs() < 1e-6,
            "max[0] = {} not 2.0 (4 cells × 0.5mm)",
            bbox.max[0]
        );
        assert!(
            (bbox.max[1] - 1.5).abs() < 1e-6,
            "max[1] = {} not 1.5 (3 cells × 0.5mm)",
            bbox.max[1]
        );
        assert!(
            (bbox.max[2] - 0.5).abs() < 1e-6,
            "max[2] = {} not 0.5 (5 layers × 100µm)",
            bbox.max[2]
        );
    }

    #[test]
    fn bounding_box_empty_input_returns_inf_sentinel() {
        let bbox = slice_stack_bounding_box(&[]);
        assert_eq!(bbox.min, [f32::INFINITY; 3]);
        assert_eq!(bbox.max, [f32::NEG_INFINITY; 3]);
    }

    #[test]
    fn all_none_masks_yields_empty_mesh() {
        let layers = vec![
            no_mask_layer(100.0),
            no_mask_layer(100.0),
            no_mask_layer(100.0),
        ];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        assert_eq!(positions_of(&mesh).len(), 0);
    }

    #[test]
    fn all_none_bounding_box_returns_inf_sentinel() {
        // bbox follows the same all-None branch as the mesh helper.
        let layers = vec![no_mask_layer(100.0), no_mask_layer(100.0)];
        let bbox = slice_stack_bounding_box(&layers);
        assert_eq!(bbox.min, [f32::INFINITY; 3]);
        assert_eq!(bbox.max, [f32::NEG_INFINITY; 3]);
    }

    #[test]
    fn f64_z_prefix_sum_resists_drift_over_5000_layers() {
        // 5000 layers × 20µm = 100mm. With f32 cumulative summation
        // the top of the bbox would drift by ~50µm; f64 accumulator
        // keeps it within 1e-3 mm.
        let layers: Vec<LayerInput> = (0..5000)
            .map(|_| empty_mask_layer(20.0, 1, 1, 0.5))
            .collect();
        let bbox = slice_stack_bounding_box(&layers);
        assert!(
            (bbox.max[2] - 100.0).abs() < 1e-3,
            "f64 prefix sum: top-of-bbox should be 100.0 mm ± 1e-3, got {}",
            bbox.max[2]
        );
    }

    #[test]
    fn cumulative_z_mm_empty_input_returns_single_zero() {
        // Empty layer stack — the prefix sum is just the starting Z = 0.
        let prefix = cumulative_z_mm(&[]);
        assert_eq!(prefix, vec![0.0]);
    }

    #[test]
    fn cumulative_z_mm_three_layers_returns_four_entries() {
        // 3 × 100µm = 0, 0.1, 0.2, 0.3 mm — entry i is the bottom-Z of
        // layer i, entry n is the top-Z of the stack.
        let layers = vec![
            empty_mask_layer(100.0, 1, 1, 0.5),
            empty_mask_layer(100.0, 1, 1, 0.5),
            empty_mask_layer(100.0, 1, 1, 0.5),
        ];
        let prefix = cumulative_z_mm(&layers);
        assert_eq!(prefix.len(), 4);
        for (i, expected) in [0.0_f32, 0.1, 0.2, 0.3].iter().enumerate() {
            assert!(
                (prefix[i] - expected).abs() < 1e-6,
                "prefix[{i}] = {} not {expected}",
                prefix[i]
            );
        }
    }

    #[test]
    fn cumulative_z_mm_handles_varying_layer_heights() {
        // Mixed layer heights: 50µm, 20µm, 100µm — prefix is
        // [0.0, 0.05, 0.07, 0.17] mm.
        let layers = vec![
            empty_mask_layer(50.0, 1, 1, 0.5),
            empty_mask_layer(20.0, 1, 1, 0.5),
            empty_mask_layer(100.0, 1, 1, 0.5),
        ];
        let prefix = cumulative_z_mm(&layers);
        let expected = [0.0_f32, 0.05, 0.07, 0.17];
        for (i, e) in expected.iter().enumerate() {
            assert!(
                (prefix[i] - e).abs() < 1e-6,
                "prefix[{i}] = {} not {e}",
                prefix[i]
            );
        }
    }

    #[test]
    fn cumulative_z_mm_resists_drift_over_5000_layers() {
        // Same drift test as the bbox test but exercises the helper
        // directly. 5000 × 20µm — top entry within 1e-3 mm of 100.0.
        let layers: Vec<LayerInput> = (0..5000)
            .map(|_| empty_mask_layer(20.0, 1, 1, 0.5))
            .collect();
        let prefix = cumulative_z_mm(&layers);
        assert_eq!(prefix.len(), 5001);
        let top = *prefix.last().expect("non-empty");
        assert!(
            (top - 100.0).abs() < 1e-3,
            "f64 prefix sum: top should be 100.0 mm ± 1e-3, got {top}"
        );
    }

    fn colors_of(mesh: &Mesh) -> Option<Vec<[f32; 4]>> {
        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            .map(|attr| match attr {
                VertexAttributeValues::Float32x4(v) => v.clone(),
                other => panic!("expected Float32x4 colors, got {other:?}"),
            })
    }

    #[test]
    fn none_colors_omits_attribute_color() {
        // Default path: no per-layer colors → no ATTRIBUTE_COLOR on the
        // resulting mesh. Existing 11 tests in this file rely on this
        // shape for byte-equality with their pre-refactor expectations.
        let layers = vec![solid_mask_layer(100.0, 1, 1, 0.5)];
        let mesh = slice_stack_to_bevy_mesh(&layers, None);
        assert!(
            colors_of(&mesh).is_none(),
            "ATTRIBUTE_COLOR must be absent when colors=None"
        );
    }

    #[test]
    fn some_colors_emits_attribute_color_with_replicated_layer_color() {
        // Single 1×1 voxel, one layer, red colour [1,0,0,1] → 6 faces ×
        // 6 vertices = 36 vertices, all carrying [1,0,0,1].
        let layers = vec![solid_mask_layer(100.0, 1, 1, 0.5)];
        let red = [1.0_f32, 0.0, 0.0, 1.0];
        let mesh = slice_stack_to_bevy_mesh(&layers, Some(&[red]));
        let colors = colors_of(&mesh).expect("ATTRIBUTE_COLOR must be present");
        assert_eq!(colors.len(), 36, "1×1×1 voxel emits 36 vertices");
        for (i, c) in colors.iter().enumerate() {
            assert_eq!(*c, red, "vertex {i}: {c:?} expected {red:?}");
        }
    }

    #[test]
    fn two_layer_distinct_colors_partition_vertices_by_layer() {
        // Two layers, each 1×1 solid, distinct colours. Interior +Z/-Z
        // faces between them are culled; layer-0 emits 5 faces (≠ +Z),
        // layer-1 emits 5 faces (≠ -Z). Each emits 30 vertices.
        let layers = vec![
            solid_mask_layer(100.0, 1, 1, 0.5),
            solid_mask_layer(100.0, 1, 1, 0.5),
        ];
        let red = [1.0_f32, 0.0, 0.0, 1.0];
        let blue = [0.0_f32, 0.0, 1.0, 1.0];
        let mesh = slice_stack_to_bevy_mesh(&layers, Some(&[red, blue]));
        let colors = colors_of(&mesh).expect("ATTRIBUTE_COLOR must be present");
        // Total vertices: 2 layers × 5 faces × 6 vertices = 60.
        assert_eq!(colors.len(), 60);
        let red_count = colors.iter().filter(|c| **c == red).count();
        let blue_count = colors.iter().filter(|c| **c == blue).count();
        assert_eq!(red_count, 30, "30 red verts (layer 0, 5 faces × 6 verts)");
        assert_eq!(blue_count, 30, "30 blue verts (layer 1, 5 faces × 6 verts)");
    }

    #[test]
    fn mismatched_colors_length_omits_attribute_color() {
        // Layers.len() = 2, colors.len() = 1 → length mismatch. The
        // viz-side soft fallback drops the colour buffer (warn) so the
        // mesh has NO ATTRIBUTE_COLOR. main.rs's hard-error policy is
        // the first line of defence; this is the second.
        let layers = vec![
            solid_mask_layer(100.0, 1, 1, 0.5),
            solid_mask_layer(100.0, 1, 1, 0.5),
        ];
        let red = [1.0_f32, 0.0, 0.0, 1.0];
        let mesh = slice_stack_to_bevy_mesh(&layers, Some(&[red]));
        assert!(
            colors_of(&mesh).is_none(),
            "mismatched colors length must drop ATTRIBUTE_COLOR (warn fallback)"
        );
    }

    #[test]
    fn empty_colors_with_zero_layers_yields_no_attribute_color() {
        // Edge case: zero layers + zero colors is a "matching" length
        // pair, but the empty-input early return omits ATTRIBUTE_COLOR
        // unconditionally (no faces emitted, nothing to colour). Locks
        // in that the early-return path doesn't accidentally insert an
        // empty colour buffer.
        let mesh = slice_stack_to_bevy_mesh(&[], Some(&[]));
        assert!(colors_of(&mesh).is_none());
    }
}
