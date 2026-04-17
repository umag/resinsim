use std::path::Path;

/// A triangle in 3D space with vertices in mm.
#[derive(Debug, Clone)]
pub struct Triangle {
    pub v0: [f32; 3],
    pub v1: [f32; 3],
    pub v2: [f32; 3],
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BoundingBox {
    pub fn height(&self) -> f32 {
        self.max[2] - self.min[2]
    }
}

/// Load triangles from a binary or ASCII STL file.
pub fn load_stl(path: &Path) -> Result<Vec<Triangle>, String> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| format!("failed to open STL: {e}"))?;

    let mesh = stl_io::read_stl(&mut file).map_err(|e| format!("failed to parse STL: {e}"))?;

    let triangles = mesh
        .faces
        .iter()
        .map(|face| {
            let v0 = mesh.vertices[face.vertices[0]];
            let v1 = mesh.vertices[face.vertices[1]];
            let v2 = mesh.vertices[face.vertices[2]];
            Triangle {
                v0: [v0[0], v0[1], v0[2]],
                v1: [v1[0], v1[1], v1[2]],
                v2: [v2[0], v2[1], v2[2]],
            }
        })
        .collect();

    Ok(triangles)
}

/// Compute bounding box of a triangle mesh.
pub fn bounding_box(triangles: &[Triangle]) -> BoundingBox {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for tri in triangles {
        for v in [&tri.v0, &tri.v1, &tri.v2] {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
    }

    BoundingBox { min, max }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_single_triangle() {
        let tris = vec![Triangle {
            v0: [0.0, 0.0, 0.0],
            v1: [10.0, 0.0, 0.0],
            v2: [5.0, 10.0, 5.0],
        }];
        let bb = bounding_box(&tris);
        assert_eq!(bb.min, [0.0, 0.0, 0.0]);
        assert_eq!(bb.max, [10.0, 10.0, 5.0]);
        assert!((bb.height() - 5.0).abs() < 1e-6);
    }
}
