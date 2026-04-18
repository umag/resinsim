use crate::io::stl::{BoundingBox, Triangle};
use crate::values::CrossSectionArea;

/// Compute cross-section areas for all layers of a mesh.
/// Returns one CrossSectionArea per layer from z_min to z_max.
///
/// Uses a simplified Sutherland-Hodgman approach: for each triangle, compute
/// the intersection polygon with the Z-plane, then sum signed areas.
/// For Tier 1 we count triangle-plane intersections and compute cross-section
/// area via the shoelace formula on the intersection contour projected to XY.
///
/// For solid convex shapes this is exact. For complex meshes with
/// internal cavities, this gives the outer-boundary area (sufficient
/// for peel force estimation).
pub fn slice_areas(
    triangles: &[Triangle],
    bbox: &BoundingBox,
    layer_height_um: f32,
) -> Vec<CrossSectionArea> {
    if !layer_height_um.is_finite() || layer_height_um <= 0.0 {
        return vec![];
    }
    let z_min = bbox.min[2];
    let z_max = bbox.max[2];
    if !z_min.is_finite() || !z_max.is_finite() || z_max <= z_min {
        return vec![];
    }
    let layer_height_mm = layer_height_um / 1000.0;
    let n_layers_f = ((z_max - z_min) / layer_height_mm).ceil();
    if !n_layers_f.is_finite() || n_layers_f <= 0.0 {
        return vec![];
    }
    let n_layers = n_layers_f as usize;

    let mut areas = Vec::with_capacity(n_layers);

    for layer in 0..n_layers {
        // Slice at mid-layer Z
        let z = z_min + (layer as f32 + 0.5) * layer_height_mm;
        let area = cross_section_at_z(triangles, z);
        areas.push(area);
    }

    areas
}

/// Compute cross-section area at a specific Z height.
/// Uses contour integration: collect all edge-plane intersection segments,
/// then compute enclosed area via the shoelace formula.
fn cross_section_at_z(triangles: &[Triangle], z: f32) -> CrossSectionArea {
    let mut segments: Vec<([f32; 2], [f32; 2])> = Vec::new();

    for tri in triangles {
        if let Some(seg) = triangle_z_intersection(tri, z) {
            segments.push(seg);
        }
    }

    if segments.is_empty() {
        return CrossSectionArea::new(0.0).expect("zero is valid");
    }

    // For a closed mesh, the segments form closed contours.
    // The area can be computed by summing the signed contributions
    // of each segment using the shoelace formula:
    // A = 0.5 × |Σ (x₁·y₂ - x₂·y₁)| for each segment
    let area: f64 = segments
        .iter()
        .map(|(p0, p1)| (p0[0] as f64 * p1[1] as f64) - (p1[0] as f64 * p0[1] as f64))
        .sum::<f64>()
        .abs()
        * 0.5;

    CrossSectionArea::new(if area.is_finite() { area } else { 0.0 })
        .expect("guarded finite — NaN mesh vertices produce zero area")
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
            Triangle { v0: v[0], v1: v[2], v2: v[1] },
            Triangle { v0: v[0], v1: v[3], v2: v[2] },
            // Top (z=1)
            Triangle { v0: v[4], v1: v[5], v2: v[6] },
            Triangle { v0: v[4], v1: v[6], v2: v[7] },
            // Front (y=0)
            Triangle { v0: v[0], v1: v[1], v2: v[5] },
            Triangle { v0: v[0], v1: v[5], v2: v[4] },
            // Back (y=1)
            Triangle { v0: v[3], v1: v[7], v2: v[6] },
            Triangle { v0: v[3], v1: v[6], v2: v[2] },
            // Left (x=0)
            Triangle { v0: v[0], v1: v[4], v2: v[7] },
            Triangle { v0: v[0], v1: v[7], v2: v[3] },
            // Right (x=1)
            Triangle { v0: v[1], v1: v[2], v2: v[6] },
            Triangle { v0: v[1], v1: v[6], v2: v[5] },
        ]
    }

    #[test]
    fn cube_cross_section_at_midheight() {
        let tris = unit_cube();
        let area = cross_section_at_z(&tris, 0.5);
        // 1mm × 1mm cube → cross section = 1.0 mm²
        assert!((area.value() - 1.0).abs() < 0.01, "expected ~1.0, got {}", area.value());
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
}
