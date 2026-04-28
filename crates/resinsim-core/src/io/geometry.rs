use crate::io::stl::{BoundingBox, Triangle};
use crate::values::{CrossSectionArea, LayerGeometry, LayerMask, DEFAULT_VOXEL_SIZE_MM};

/// Compute cross-section areas for all layers of a mesh.
/// Returns one CrossSectionArea per layer from z_min to z_max.
///
/// Thin wrapper over [`slice_layers`] that extracts only the area field.
/// Existing callers that don't need the per-layer occupancy mask continue to
/// call `slice_areas`; `SimulationRunner::run_stl` still uses it before
/// wrapping each area into a trivial `LayerMask` via
/// `SimulationRunner::run_from_areas`'s adapter path (Step 7).
pub fn slice_areas(
    triangles: &[Triangle],
    bbox: &BoundingBox,
    layer_height_um: f32,
) -> Vec<CrossSectionArea> {
    slice_layers(triangles, bbox, layer_height_um, DEFAULT_VOXEL_SIZE_MM)
        .into_iter()
        .map(|lg| lg.area)
        .collect()
}

/// Compute per-layer area + 2D occupancy mask for a mesh.
///
/// Returns one `LayerGeometry { area, mask }` per layer from z_min to z_max.
///
/// The area is computed via shoelace over triangle-plane intersection segments
/// (same algorithm as the previous `slice_areas`). The mask is computed via
/// scanline even-odd rasterisation of the same segments at `voxel_size_mm`
/// physical resolution.
///
/// # Scanline fill
///
/// For each cell row at world y = bbox.min.y + (row + 0.5) * voxel_size_mm:
///
/// 1. Collect all segments whose Y-range strictly straddles the scan line
///    (one endpoint strictly above, one strictly below). Segments tangent to
///    the scan line are skipped — this convention avoids double-counting
///    when a vertex lies exactly on the line.
/// 2. Linearly interpolate each segment's X at the scan line's Y.
/// 3. Sort X-intersections; fill cells between each consecutive (odd, even)
///    pair. Non-convex and multi-component polygons are handled naturally.
///
/// Returns an empty Vec on malformed input (non-finite layer height, empty
/// bbox, etc.) — matches the pre-existing `slice_areas` behaviour.
pub fn slice_layers(
    triangles: &[Triangle],
    bbox: &BoundingBox,
    layer_height_um: f32,
    voxel_size_mm: f32,
) -> Vec<LayerGeometry> {
    if !layer_height_um.is_finite() || layer_height_um <= 0.0 {
        return vec![];
    }
    if !voxel_size_mm.is_finite() || voxel_size_mm <= 0.0 {
        return vec![];
    }
    let z_min = bbox.min[2];
    let z_max = bbox.max[2];
    if !z_min.is_finite() || !z_max.is_finite() || z_max <= z_min {
        return vec![];
    }
    let x_span = bbox.max[0] - bbox.min[0];
    let y_span = bbox.max[1] - bbox.min[1];
    if !x_span.is_finite() || !y_span.is_finite() || x_span <= 0.0 || y_span <= 0.0 {
        return vec![];
    }

    let layer_height_mm = layer_height_um / 1000.0;
    let n_layers_f = ((z_max - z_min) / layer_height_mm).ceil();
    if !n_layers_f.is_finite() || n_layers_f <= 0.0 {
        return vec![];
    }
    let n_layers = n_layers_f as usize;

    let width_cells = ((x_span / voxel_size_mm).ceil() as u32).max(1);
    let height_cells = ((y_span / voxel_size_mm).ceil() as u32).max(1);

    let mut geometries = Vec::with_capacity(n_layers);

    for layer in 0..n_layers {
        let z = z_min + (layer as f32 + 0.5) * layer_height_mm;
        let segments = collect_segments_at_z(triangles, z);
        let area = shoelace_area(&segments);
        let mask = rasterise_mask(
            &segments,
            bbox.min[0],
            bbox.min[1],
            width_cells,
            height_cells,
            voxel_size_mm,
        );
        geometries.push(LayerGeometry::new(area, mask));
    }

    geometries
}

/// Compute cross-section area at a specific Z height via shoelace.
/// Kept for tests; production callers use `slice_layers`.
#[cfg(test)]
fn cross_section_at_z(triangles: &[Triangle], z: f32) -> CrossSectionArea {
    let segments = collect_segments_at_z(triangles, z);
    shoelace_area(&segments)
}

/// Collect all triangle-plane intersection segments at Z=z.
fn collect_segments_at_z(triangles: &[Triangle], z: f32) -> Vec<([f32; 2], [f32; 2])> {
    let mut segments = Vec::new();
    for tri in triangles {
        if let Some(seg) = triangle_z_intersection(tri, z) {
            segments.push(seg);
        }
    }
    segments
}

/// Shoelace area over a segment set. For a closed mesh, segments form closed
/// contours; the signed-sum magnitude is the cross-section area.
fn shoelace_area(segments: &[([f32; 2], [f32; 2])]) -> CrossSectionArea {
    if segments.is_empty() {
        return CrossSectionArea::new(0.0).expect("zero is valid");
    }
    let area: f64 = segments
        .iter()
        .map(|(p0, p1)| (p0[0] as f64 * p1[1] as f64) - (p1[0] as f64 * p0[1] as f64))
        .sum::<f64>()
        .abs()
        * 0.5;
    CrossSectionArea::new(if area.is_finite() { area } else { 0.0 })
        .expect("guarded finite — NaN mesh vertices produce zero area")
}

/// Scanline even-odd fill over the segments to produce a binary mask.
///
/// The origin of the mask is at (bbox_min_x, bbox_min_y); cell (cx, cy) covers
/// the world-space square [bbox_min_x + cx*voxel, bbox_min_x + (cx+1)*voxel) ×
/// [bbox_min_y + cy*voxel, bbox_min_y + (cy+1)*voxel). A cell is marked solid
/// iff its centre lies inside an odd number of segment crossings in the +X
/// direction from that centre.
fn rasterise_mask(
    segments: &[([f32; 2], [f32; 2])],
    bbox_min_x: f32,
    bbox_min_y: f32,
    width_cells: u32,
    height_cells: u32,
    voxel_size_mm: f32,
) -> LayerMask {
    let mut mask = LayerMask::new(width_cells, height_cells, voxel_size_mm)
        .expect("slice_layers validated non-zero width/height and positive finite voxel_size_mm");

    if segments.is_empty() {
        return mask;
    }

    for row in 0..height_cells {
        let world_y = bbox_min_y + (row as f32 + 0.5) * voxel_size_mm;

        // Collect X-intersections at this scan line.
        let mut xs: Vec<f32> = Vec::new();
        for (a, b) in segments {
            let y0 = a[1];
            let y1 = b[1];
            // Strict straddle: one strictly above, one strictly below. Skips
            // tangent segments to avoid double-counting at shared vertices.
            let straddles = (y0 < world_y && y1 > world_y) || (y0 > world_y && y1 < world_y);
            if !straddles {
                continue;
            }
            let t = (world_y - y0) / (y1 - y0);
            let x = a[0] + t * (b[0] - a[0]);
            xs.push(x);
        }
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Fill cells between odd-even pairs.
        let mut i = 0;
        while i + 1 < xs.len() {
            let x_start = xs[i];
            let x_end = xs[i + 1];
            i += 2;

            let cell_start_f = (x_start - bbox_min_x) / voxel_size_mm;
            let cell_end_f = (x_end - bbox_min_x) / voxel_size_mm;
            // Round inward: start rounds up, end rounds down — mirrors the
            // "cell centre inside polygon" criterion.
            let cell_start = cell_start_f.ceil().max(0.0) as u32;
            let cell_end = cell_end_f.floor().min((width_cells as f32) - 1.0).max(-1.0) as i64;

            if cell_end < 0 {
                continue;
            }
            let cell_end = cell_end as u32;
            for cx in cell_start..=cell_end.min(width_cells.saturating_sub(1)) {
                let _ = mask.set(cx, row);
            }
        }
    }

    mask
}

/// Intersect a triangle with a Z-plane. Returns the 2D segment (in XY)
/// if the plane cuts through the triangle, or None if it doesn't.
fn triangle_z_intersection(tri: &Triangle, z: f32) -> Option<([f32; 2], [f32; 2])> {
    let verts = [tri.v0, tri.v1, tri.v2];
    let mut above = Vec::new();
    let mut below = Vec::new();
    let mut on_plane = Vec::new();

    for (i, v) in verts.iter().enumerate() {
        let dz = v[2] - z;
        if dz.abs() < 1e-6 {
            on_plane.push(i);
        } else if dz > 0.0 {
            above.push(i);
        } else {
            below.push(i);
        }
    }

    // Two vertices on plane — the edge is the intersection
    if on_plane.len() == 2 {
        let p0 = verts[on_plane[0]];
        let p1 = verts[on_plane[1]];
        return Some(([p0[0], p0[1]], [p1[0], p1[1]]));
    }

    // Need vertices on both sides for a proper intersection
    if above.is_empty() || below.is_empty() {
        return None;
    }

    // Find the two intersection points on the triangle edges
    let mut points = Vec::with_capacity(2);

    let edges = [(0, 1), (1, 2), (2, 0)];
    for (i, j) in edges {
        let zi = verts[i][2];
        let zj = verts[j][2];
        if (zi - z) * (zj - z) < 0.0 {
            // Edge crosses the plane
            let t = (z - zi) / (zj - zi);
            let x = verts[i][0] + t * (verts[j][0] - verts[i][0]);
            let y = verts[i][1] + t * (verts[j][1] - verts[i][1]);
            points.push([x, y]);
        }
    }

    if points.len() == 2 {
        Some((points[0], points[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::stl;

    /// Create a unit cube (1mm × 1mm × 1mm) as 12 triangles.
    fn unit_cube() -> Vec<Triangle> {
        // 6 faces × 2 triangles each
        let v = [
            [0.0f32, 0.0, 0.0], // 0: origin
            [1.0, 0.0, 0.0],    // 1
            [1.0, 1.0, 0.0],    // 2
            [0.0, 1.0, 0.0],    // 3
            [0.0, 0.0, 1.0],    // 4
            [1.0, 0.0, 1.0],    // 5
            [1.0, 1.0, 1.0],    // 6
            [0.0, 1.0, 1.0],    // 7
        ];

        vec![
            // Bottom (z=0)
            Triangle {
                v0: v[0],
                v1: v[2],
                v2: v[1],
            },
            Triangle {
                v0: v[0],
                v1: v[3],
                v2: v[2],
            },
            // Top (z=1)
            Triangle {
                v0: v[4],
                v1: v[5],
                v2: v[6],
            },
            Triangle {
                v0: v[4],
                v1: v[6],
                v2: v[7],
            },
            // Front (y=0)
            Triangle {
                v0: v[0],
                v1: v[1],
                v2: v[5],
            },
            Triangle {
                v0: v[0],
                v1: v[5],
                v2: v[4],
            },
            // Back (y=1)
            Triangle {
                v0: v[3],
                v1: v[7],
                v2: v[6],
            },
            Triangle {
                v0: v[3],
                v1: v[6],
                v2: v[2],
            },
            // Left (x=0)
            Triangle {
                v0: v[0],
                v1: v[4],
                v2: v[7],
            },
            Triangle {
                v0: v[0],
                v1: v[7],
                v2: v[3],
            },
            // Right (x=1)
            Triangle {
                v0: v[1],
                v1: v[2],
                v2: v[6],
            },
            Triangle {
                v0: v[1],
                v1: v[6],
                v2: v[5],
            },
        ]
    }

    #[test]
    fn cube_cross_section_at_midheight() {
        let tris = unit_cube();
        let area = cross_section_at_z(&tris, 0.5);
        // 1mm × 1mm cube → cross section = 1.0 mm²
        assert!(
            (area.value() - 1.0).abs() < 0.01,
            "expected ~1.0, got {}",
            area.value()
        );
    }

    #[test]
    fn cube_cross_section_constant_across_layers() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        let areas = slice_areas(&tris, &bbox, 100.0); // 100µm layers → 10 layers for 1mm

        assert_eq!(areas.len(), 10);
        for (i, a) in areas.iter().enumerate() {
            assert!(
                (a.value() - 1.0).abs() < 0.01,
                "layer {i}: expected ~1.0 mm², got {:.4}",
                a.value()
            );
        }
    }

    #[test]
    fn cube_cross_section_outside_mesh_is_zero() {
        let tris = unit_cube();
        let area = cross_section_at_z(&tris, 2.0);
        assert!((area.value()).abs() < 1e-6);
    }

    #[test]
    fn zero_layer_height_returns_empty() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        assert!(slice_areas(&tris, &bbox, 0.0).is_empty());
        assert!(slice_areas(&tris, &bbox, -10.0).is_empty());
        assert!(slice_areas(&tris, &bbox, f32::NAN).is_empty());
    }

    #[test]
    fn degenerate_bbox_returns_empty() {
        let tris = unit_cube();
        let degenerate = stl::BoundingBox {
            min: [0.0, 0.0, 1.0],
            max: [0.0, 0.0, 0.0],
        };
        assert!(slice_areas(&tris, &degenerate, 50.0).is_empty());
    }

    #[test]
    fn empty_mesh_no_layers() {
        let areas = slice_areas(
            &[],
            &stl::BoundingBox {
                min: [0.0, 0.0, 0.0],
                max: [0.0, 0.0, 0.0],
            },
            50.0,
        );
        assert!(areas.is_empty());
    }

    // --- slice_layers (Step 4 of suction-detector-raft-false-positive) ---

    #[test]
    fn slice_layers_returns_same_layer_count_as_slice_areas() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        let areas = slice_areas(&tris, &bbox, 100.0);
        let geoms = slice_layers(&tris, &bbox, 100.0, 0.1);
        assert_eq!(geoms.len(), areas.len());
    }

    #[test]
    fn slice_layers_area_matches_slice_areas() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        let areas = slice_areas(&tris, &bbox, 100.0);
        let geoms = slice_layers(&tris, &bbox, 100.0, 0.1);
        for (i, (lg, a)) in geoms.iter().zip(areas.iter()).enumerate() {
            assert!(
                (lg.area.value() - a.value()).abs() < 1e-6,
                "layer {i}: slice_layers area {} vs slice_areas area {}",
                lg.area.value(),
                a.value()
            );
        }
    }

    #[test]
    fn slice_layers_mask_is_solid_for_cube_interior() {
        // Unit cube sliced at 0.1mm voxels → 10×10 cells per layer.
        // All cells should be solid (mask.solid_area_mm2 ≈ 1.0 mm²).
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        let geoms = slice_layers(&tris, &bbox, 100.0, 0.1);
        for (i, lg) in geoms.iter().enumerate() {
            // Allow 10% rounding at 0.1mm voxels on a 1mm cube
            let mask_area = lg.mask.solid_area_mm2();
            assert!(
                (mask_area - 1.0).abs() < 0.15,
                "layer {i}: mask area {:.4} far from 1.0",
                mask_area
            );
        }
    }

    #[test]
    fn slice_layers_rejects_non_finite_voxel_size() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        assert!(slice_layers(&tris, &bbox, 100.0, 0.0).is_empty());
        assert!(slice_layers(&tris, &bbox, 100.0, -0.5).is_empty());
        assert!(slice_layers(&tris, &bbox, 100.0, f32::NAN).is_empty());
    }

    #[test]
    fn slice_layers_voxel_resolution_affects_mask_dimensions() {
        let tris = unit_cube();
        let bbox = stl::bounding_box(&tris);
        let coarse = slice_layers(&tris, &bbox, 500.0, 0.5);
        let fine = slice_layers(&tris, &bbox, 500.0, 0.1);
        assert_eq!(coarse.len(), fine.len());
        // Coarse → 2×2 cells; fine → 10×10
        assert!(coarse[0].mask.width_cells() < fine[0].mask.width_cells());
    }

    #[test]
    fn slice_layers_empty_mesh_empty_output() {
        let geoms = slice_layers(
            &[],
            &stl::BoundingBox {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
            },
            100.0,
            0.1,
        );
        // Non-empty bbox → layer count > 0, but each mask should be all-void.
        assert!(!geoms.is_empty());
        for lg in &geoms {
            assert_eq!(lg.mask.solid_cell_count(), 0);
            assert_eq!(lg.area.value(), 0.0);
        }
    }
}
