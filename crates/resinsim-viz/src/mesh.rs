//! STL → Bevy mesh conversion and PanOrbitCamera auto-fit helpers.
//!
//! Lives in `resinsim-viz` (presentation layer per ADR-0010): no
//! `bevy::*` types may flow into `resinsim-core`. Triangle and BoundingBox
//! flow the other way, from core into viz.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use resinsim_core::io::stl::{BoundingBox, Triangle};

/// ECS marker for the entity holding the currently-loaded STL mesh.
///
/// Used by the loader system to find prior loads so they can be despawned
/// before the new mesh is added.
#[derive(Component)]
pub struct LoadedStlMesh;

/// Convert a slice of STL triangles into a flat-shaded Bevy `Mesh`.
///
/// Each input triangle becomes 3 unique vertex positions (no vertex
/// sharing across triangles), one face normal computed from the cross
/// product of two edges and replicated to all three of the triangle's
/// vertices, and three sequential indices `3i, 3i+1, 3i+2`.
/// Topology is `TriangleList`; render-asset usage is `default()` (kept
/// in main world *and* uploaded to the GPU, the common case).
///
/// STL winding is not guaranteed to be CCW, so face normals may point
/// inward for some inputs. Lighting will look wrong in that case; the
/// geometry still renders. A future issue can derive normals from the
/// `stl_io` face data when present, or auto-flip via a centroid heuristic.
pub fn triangles_to_bevy_mesh(triangles: &[Triangle]) -> Mesh {
    let n = triangles.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(3 * n);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(3 * n);
    let mut indices: Vec<u32> = Vec::with_capacity(3 * n);

    for (i, tri) in triangles.iter().enumerate() {
        let v0 = Vec3::from(tri.v0);
        let v1 = Vec3::from(tri.v1);
        let v2 = Vec3::from(tri.v2);
        let normal = (v1 - v0).cross(v2 - v0).normalize_or_zero();
        let n_arr: [f32; 3] = normal.into();

        positions.push(tri.v0);
        positions.push(tri.v1);
        positions.push(tri.v2);
        normals.push(n_arr);
        normals.push(n_arr);
        normals.push(n_arr);

        let base = (3 * i) as u32;
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// Drive a `PanOrbitCamera` to frame the given bounding box.
///
/// Sets `focus`/`target_focus` to the bbox centre and
/// `radius`/`target_radius` to `1.5 × diagonal`. PanOrbitCamera
/// recomputes the entity Transform every frame from these fields, so
/// writing Transform directly is a no-op — driving the fields is the
/// supported API (verified against `bevy_panorbit_camera 0.34` source:
/// `focus: Vec3`, `target_focus: Vec3`, `radius: Option<f32>`,
/// `target_radius: f32`).
///
/// Degenerate bbox falls back to focus = origin, distance = 10.0 to
/// keep the camera valid for empty / collapsed input. "Degenerate" means
/// any of: zero-volume (diagonal < 1e-6), non-finite diagonal (e.g.
/// `bounding_box(&[])` returns `min: [INF;3]`, `max: [NEG_INF;3]` →
/// diagonal = INF), or non-finite centre (INF + NEG_INF = NaN).
/// Without this guard an empty-but-syntactically-valid STL drops the
/// camera into NaN/INF state.
///
/// `preserve_view` controls whether the orbit angle (`yaw` / `pitch`) is
/// re-locked to the default 3/4 view (false = first-load behaviour) or
/// preserved (true = drag-drop reload behaviour). Mirrors DragonFruit's
/// `preserveCurrentViewDirection` flag
/// (`DragonFruit/src/components/scene/camera/CameraIntroController.tsx:103`).
/// When `preserve_view` is true this function only writes
/// `target_focus` + `target_radius`; yaw / pitch are untouched so the user's
/// current orbit angle survives the reload.
pub fn fit_panorbit_to_bbox(cam: &mut PanOrbitCamera, bbox: &BoundingBox, preserve_view: bool) {
    let min = Vec3::from(bbox.min);
    let max = Vec3::from(bbox.max);
    let diagonal = (max - min).length();
    let centre = (min + max) * 0.5;

    let degenerate = !diagonal.is_finite() || diagonal < 1e-6 || !centre.is_finite();

    if degenerate {
        cam.focus = Vec3::ZERO;
        cam.target_focus = Vec3::ZERO;
        cam.radius = Some(10.0);
        cam.target_radius = 10.0;
        return;
    }

    let distance = 1.5 * diagonal;
    cam.focus = centre;
    cam.target_focus = centre;
    cam.radius = Some(distance);
    cam.target_radius = distance;
    if !preserve_view {
        let (yaw_3q, pitch_3q) = crate::three_quarter_yaw_pitch();
        cam.yaw = Some(yaw_3q);
        cam.pitch = Some(pitch_3q);
        cam.target_yaw = yaw_3q;
        cam.target_pitch = pitch_3q;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use resinsim_core::io::stl;
    use std::path::PathBuf;

    fn cube_fixture_path() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/test_cube.stl"
        ))
    }

    fn positions_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("triangles_to_bevy_mesh always inserts ATTRIBUTE_POSITION")
        {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            other => panic!("expected Float32x3 positions, got {other:?}"),
        }
    }

    fn normals_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .expect("triangles_to_bevy_mesh always inserts ATTRIBUTE_NORMAL")
        {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            other => panic!("expected Float32x3 normals, got {other:?}"),
        }
    }

    fn indices_of(mesh: &Mesh) -> Vec<u32> {
        match mesh
            .indices()
            .expect("triangles_to_bevy_mesh always inserts indices")
        {
            Indices::U32(v) => v.clone(),
            Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
        }
    }

    #[test]
    fn empty_input_yields_empty_mesh() {
        let mesh = triangles_to_bevy_mesh(&[]);
        assert!(positions_of(&mesh).is_empty());
        assert!(normals_of(&mesh).is_empty());
        assert!(indices_of(&mesh).is_empty());
    }

    #[test]
    fn single_triangle_yields_3_positions_3_normals_3_indices() {
        let tri = Triangle {
            v0: [0.0, 0.0, 0.0],
            v1: [1.0, 0.0, 0.0],
            v2: [0.0, 1.0, 0.0],
        };
        let mesh = triangles_to_bevy_mesh(&[tri]);
        let positions = positions_of(&mesh);
        let normals = normals_of(&mesh);
        let indices = indices_of(&mesh);
        assert_eq!(positions.len(), 3);
        assert_eq!(normals.len(), 3);
        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(positions[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn face_normal_matches_cross_product() {
        // Triangle in the XY plane wound CCW from +Z view → normal = +Z.
        let tri = Triangle {
            v0: [0.0, 0.0, 0.0],
            v1: [1.0, 0.0, 0.0],
            v2: [0.0, 1.0, 0.0],
        };
        let mesh = triangles_to_bevy_mesh(&[tri]);
        let normals = normals_of(&mesh);
        for n in &normals {
            assert!((n[0] - 0.0).abs() < 1e-6, "nx = {} not 0", n[0]);
            assert!((n[1] - 0.0).abs() < 1e-6, "ny = {} not 0", n[1]);
            assert!((n[2] - 1.0).abs() < 1e-6, "nz = {} not 1", n[2]);
        }
    }

    #[test]
    fn cube_fixture_yields_36_positions() {
        let path = cube_fixture_path();
        let triangles = stl::load_stl(&path).expect("data/test_cube.stl is checked into the repo");
        assert_eq!(
            triangles.len(),
            12,
            "test_cube.stl should be a 12-triangle cube"
        );
        let mesh = triangles_to_bevy_mesh(&triangles);
        assert_eq!(positions_of(&mesh).len(), 36);
        assert_eq!(normals_of(&mesh).len(), 36);
        assert_eq!(indices_of(&mesh).len(), 36);
    }

    /// RAII guard that removes a tempdir on drop, so the malformed-STL
    /// test cleans up regardless of pass/fail.
    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn malformed_stl_returns_err_not_panic() {
        let dir =
            std::env::temp_dir().join(format!("resinsim-viz-malformed-stl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp_dir is writable on a developer workstation");
        let _guard = TempDirGuard(dir.clone());
        let path = dir.join("junk.stl");
        std::fs::write(&path, b"this is not an STL file at all")
            .expect("just-created tempdir is writable");

        let result = stl::load_stl(&path);
        assert!(
            result.is_err(),
            "garbage input must yield Err, got Ok with {:?} triangles",
            result.as_ref().map(Vec::len)
        );
    }

    #[test]
    fn degenerate_bbox_uses_fallback_panorbit_fields() {
        // Pre-poison the camera so a no-op fit would *fail* this test.
        let mut cam = PanOrbitCamera {
            focus: Vec3::splat(99.0),
            target_focus: Vec3::splat(99.0),
            radius: Some(1.0),
            target_radius: 1.0,
            ..default()
        };
        let bbox = BoundingBox {
            min: [5.0, 5.0, 5.0],
            max: [5.0, 5.0, 5.0],
        };
        fit_panorbit_to_bbox(&mut cam, &bbox, false);
        assert_eq!(cam.focus, Vec3::ZERO);
        assert_eq!(cam.target_focus, Vec3::ZERO);
        assert_eq!(cam.radius, Some(10.0));
        assert!((cam.target_radius - 10.0).abs() < 1e-6);
    }

    #[test]
    fn collinear_triangle_yields_zero_normal() {
        // Three collinear vertices give a zero cross product. We use
        // `normalize_or_zero()` which returns Vec3::ZERO instead of NaN
        // on degenerate input — this test locks that contract in.
        // Bevy renders the triangle dark (no light response) which is
        // the expected v1 behaviour.
        let tri = Triangle {
            v0: [0.0, 0.0, 0.0],
            v1: [1.0, 0.0, 0.0],
            v2: [2.0, 0.0, 0.0],
        };
        let mesh = triangles_to_bevy_mesh(&[tri]);
        let normals = normals_of(&mesh);
        for n in &normals {
            assert_eq!(*n, [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn empty_bbox_uses_fallback_panorbit_fields() {
        // `bounding_box(&[])` from resinsim_core::io::stl returns this
        // shape: an empty triangle list leaves min at INFINITY and max
        // at NEG_INFINITY (loop never updates them). Without the
        // is_finite guards in fit_panorbit_to_bbox, diagonal would be
        // INF (skipping the < 1e-6 check) and centre would be NaN
        // (INF + NEG_INF = NaN), corrupting the camera until a
        // non-empty STL is loaded.
        let mut cam = PanOrbitCamera {
            focus: Vec3::splat(99.0),
            target_focus: Vec3::splat(99.0),
            radius: Some(1.0),
            target_radius: 1.0,
            ..default()
        };
        let empty_bbox = resinsim_core::io::stl::bounding_box(&[]);
        fit_panorbit_to_bbox(&mut cam, &empty_bbox, false);
        assert_eq!(cam.focus, Vec3::ZERO);
        assert_eq!(cam.target_focus, Vec3::ZERO);
        assert_eq!(cam.radius, Some(10.0));
        assert!((cam.target_radius - 10.0).abs() < 1e-6);
    }

    #[test]
    fn fit_panorbit_to_bbox_first_load_writes_3q_yaw_pitch() {
        // preserve_view = false (Startup / first-load path): yaw and pitch
        // are re-locked to the default 3/4 view, regardless of any prior
        // camera angle. ADR-0011.
        let mut cam = PanOrbitCamera {
            yaw: Some(99.0), // intentionally bogus so the re-lock is observable
            pitch: Some(99.0),
            target_yaw: 99.0,
            target_pitch: 99.0,
            ..default()
        };
        let bbox = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        fit_panorbit_to_bbox(&mut cam, &bbox, false);
        let (expected_yaw, expected_pitch) = crate::three_quarter_yaw_pitch();
        let yaw = cam.yaw.expect("preserve_view=false branch must seed yaw");
        let pitch = cam
            .pitch
            .expect("preserve_view=false branch must seed pitch");
        assert!((yaw - expected_yaw).abs() < 1e-5);
        assert!((pitch - expected_pitch).abs() < 1e-5);
        assert!((cam.target_yaw - expected_yaw).abs() < 1e-5);
        assert!((cam.target_pitch - expected_pitch).abs() < 1e-5);
    }

    #[test]
    fn fit_panorbit_to_bbox_reload_preserves_yaw_pitch() {
        // preserve_view = true (drag-drop reload path): yaw / pitch /
        // target_yaw / target_pitch survive the call; only focus and
        // radius are updated. Mirrors DragonFruit's
        // preserveCurrentViewDirection contract for reloads.
        let user_yaw = 0.42;
        let user_pitch = 0.17;
        let mut cam = PanOrbitCamera {
            yaw: Some(user_yaw),
            pitch: Some(user_pitch),
            target_yaw: user_yaw,
            target_pitch: user_pitch,
            ..default()
        };
        let bbox = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        fit_panorbit_to_bbox(&mut cam, &bbox, true);
        // Focus moved (centre = (0.5, 0.5, 0.5)) — sanity-check.
        assert_eq!(cam.focus, Vec3::splat(0.5));
        // Yaw / pitch UNCHANGED — the reload-preserves-view contract.
        assert_eq!(cam.yaw, Some(user_yaw));
        assert_eq!(cam.pitch, Some(user_pitch));
        assert!((cam.target_yaw - user_yaw).abs() < 1e-9);
        assert!((cam.target_pitch - user_pitch).abs() < 1e-9);
    }

    #[test]
    fn unit_cube_panorbit_radius_scales_with_diagonal() {
        let mut cam = PanOrbitCamera::default();
        let bbox = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        fit_panorbit_to_bbox(&mut cam, &bbox, false);
        let expected_radius = 1.5 * 3.0_f32.sqrt();
        assert_eq!(cam.focus, Vec3::splat(0.5));
        assert_eq!(cam.target_focus, Vec3::splat(0.5));
        let r = cam.radius.expect("fit_panorbit_to_bbox always sets radius");
        assert!(
            (r - expected_radius).abs() < 1e-5,
            "radius {r} should equal 1.5 * sqrt(3) = {expected_radius}"
        );
        assert!((cam.target_radius - expected_radius).abs() < 1e-5);
    }
}
