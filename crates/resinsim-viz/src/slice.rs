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

/// ECS marker for the entity holding the currently-loaded slice stack.
///
/// Mirror of `LoadedStlMesh` from `mesh.rs`. Used by the loader systems
/// to find prior loads so they can be despawned before the new mesh is
/// added, AND to enforce mutual exclusion with `LoadedStlMesh` (only
/// one geometry source visible at a time in v1).
#[derive(Component)]
pub struct LoadedSliceStack;

/// Single-source the canonical-dims lookup for the validate / bbox
/// helpers. Returns the first mask-bearing layer's
/// `(width_cells, height_cells, voxel_size_mm)`, or `None` for an
/// all-None / empty stack.
fn first_mask_dims(layers: &[LayerInput]) -> Option<(u32, u32, f32)> {
    for layer in layers {
        if let Some(mask) = layer.mask.as_ref() {
            return Some((mask.width_cells(), mask.height_cells(), mask.voxel_size_mm()));
        }
    }
    None
}

/// Accumulate the layer-stack Z extent in f64 then narrow to f32.
///
/// f32 alone drifts ~50 µm over 4500 layers at 20 µm because the
/// mantissa step at 90 mm magnitude is ~5 µm, amplified by cumulative
/// summation. The single f64 fold pre-empts this without changing the
/// public BoundingBox shape.
fn cumulative_z_mm_f32(layers: &[LayerInput]) -> f32 {
    let total_um: f64 = layers
        .iter()
        .map(|l| l.layer_height_um as f64)
        .sum();
    (total_um / 1000.0) as f32
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
    let max_z = cumulative_z_mm_f32(layers);
    BoundingBox {
        min: [0.0, 0.0, 0.0],
        max: [max_x, max_y, max_z],
    }
}

/// Convert a slice of `LayerInput` into a flat-shaded Bevy `Mesh` via
/// face-culling boundary-quad emission.
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
pub fn slice_stack_to_bevy_mesh(layers: &[LayerInput]) -> Mesh {
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

    // Pass 2 — Z prefix sum (f64) + emission with immediate-neighbour
    // face culling.
    let n = layers.len();
    let mut z_prefix: Vec<f64> = Vec::with_capacity(n + 1);
    z_prefix.push(0.0);
    let mut acc: f64 = 0.0;
    for layer in layers {
        acc += layer.layer_height_um as f64 / 1000.0;
        z_prefix.push(acc);
    }

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for i in 0..n {
        let Some(mask) = layers[i].mask.as_ref() else {
            continue;
        };
        let z0 = z_prefix[i] as f32;
        let z1 = z_prefix[i + 1] as f32;

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
                        [
                            [x0, y1, z0],
                            [x0, y0, z0],
                            [x0, y0, z1],
                            [x0, y1, z1],
                        ],
                    );
                }
                // +X face.
                if cx + 1 == w || !mask.is_solid(cx + 1, cy) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [1.0, 0.0, 0.0],
                        [
                            [x1, y0, z0],
                            [x1, y1, z0],
                            [x1, y1, z1],
                            [x1, y0, z1],
                        ],
                    );
                }
                // -Y face.
                if cy == 0 || !mask.is_solid(cx, cy - 1) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, -1.0, 0.0],
                        [
                            [x0, y0, z0],
                            [x1, y0, z0],
                            [x1, y0, z1],
                            [x0, y0, z1],
                        ],
                    );
                }
                // +Y face.
                if cy + 1 == h || !mask.is_solid(cx, cy + 1) {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 1.0, 0.0],
                        [
                            [x1, y1, z0],
                            [x0, y1, z0],
                            [x0, y1, z1],
                            [x1, y1, z1],
                        ],
                    );
                }
                // -Z face: i==0 OR previous layer's mask is None OR
                // previous layer's voxel at (cx, cy) is empty.
                let neg_z_exposed = i == 0
                    || z_face_void(layers, i - 1, cx, cy);
                if neg_z_exposed {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 0.0, -1.0],
                        [
                            [x0, y0, z0],
                            [x0, y1, z0],
                            [x1, y1, z0],
                            [x1, y0, z0],
                        ],
                    );
                }
                // +Z face.
                let pos_z_exposed = i + 1 == n
                    || z_face_void(layers, i + 1, cx, cy);
                if pos_z_exposed {
                    push_quad(
                        &mut positions,
                        &mut normals,
                        &mut indices,
                        [0.0, 0.0, 1.0],
                        [
                            [x0, y1, z1],
                            [x0, y0, z1],
                            [x1, y0, z1],
                            [x1, y1, z1],
                        ],
                    );
                }
            }
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
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
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
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
        LayerInput::new(0, (w * h) as f64 * (voxel as f64).powi(2), 1.0, 60.0, layer_height_um, 0.0)
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
        let mesh = slice_stack_to_bevy_mesh(&[]);
        assert!(positions_of(&mesh).is_empty());
        assert!(normals_of(&mesh).is_empty());
        assert!(indices_of(&mesh).is_empty());
    }

    #[test]
    fn single_solid_voxel_yields_12_triangles() {
        // 1×1 mask with cell (0,0) solid, one layer 100µm thick at 0.5mm voxel.
        let layers = vec![solid_mask_layer(100.0, 1, 1, 0.5)];
        let mesh = slice_stack_to_bevy_mesh(&layers);
        let positions = positions_of(&mesh);
        let normals = normals_of(&mesh);
        let indices = indices_of(&mesh);
        assert_eq!(positions.len(), 36, "6 faces × 2 triangles × 3 vertices");
        assert_eq!(normals.len(), 36);
        assert_eq!(indices.len(), 36);

        // Collect unique normal directions; expect exactly the canonical six axis-aligned units.
        let mut unique: Vec<[i32; 3]> = normals
            .iter()
            .map(|n| [n[0].round() as i32, n[1].round() as i32, n[2].round() as i32])
            .collect();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 6, "expected six unique face normals, got {unique:?}");
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
        let mesh = slice_stack_to_bevy_mesh(&layers);
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
        let mesh = slice_stack_to_bevy_mesh(&layers);
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
        let mesh = slice_stack_to_bevy_mesh(&layers);
        assert_eq!(positions_of(&mesh).len(), 0);
    }

    #[test]
    fn bounding_box_matches_grid_dims_and_cumulative_z() {
        // 4×3 mask, 0.5mm voxel, 100µm layers × 5 layers.
        let layers: Vec<LayerInput> = (0..5)
            .map(|_| empty_mask_layer(100.0, 4, 3, 0.5))
            .collect();
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
        let layers = vec![no_mask_layer(100.0), no_mask_layer(100.0), no_mask_layer(100.0)];
        let mesh = slice_stack_to_bevy_mesh(&layers);
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
}
