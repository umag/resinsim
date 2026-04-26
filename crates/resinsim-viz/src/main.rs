mod mesh;

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, TrackpadBehavior};
use clap::Parser;
use resinsim_core::io::stl;

use crate::mesh::{LoadedStlMesh, fit_panorbit_to_bbox, triangles_to_bevy_mesh};

#[derive(Parser, Debug, Resource)]
#[command(name = "resinsim-viz", about = "Resinsim physics-simulation visualizer")]
struct Args {
    /// Run one frame and exit (smoke-test mode)
    #[arg(long)]
    smoke_exit: bool,

    /// Load an STL file at startup. Drag-drop replaces the loaded mesh at runtime.
    #[arg(long, value_name = "PATH")]
    load_stl: Option<PathBuf>,
}

pub fn setup_scene(mut commands: Commands) {
    // Camera carries its own DirectionalLight as a child entity — a
    // "headlamp" rig. Both Camera3d and DirectionalLight point along
    // their entity's -Z axis, so a child light with Transform::default()
    // automatically tracks the camera direction as the user orbits.
    // Shadows are disabled: when the light direction is the view
    // direction, every front-facing facet shadows the next, producing
    // self-shadowing acne that masks the geometry we're trying to
    // inspect. Ambient (200 brightness from issue 01) provides fill for
    // back-facing surfaces.
    commands
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            PanOrbitCamera {
                trackpad_behavior: TrackpadBehavior::BlenderLike {
                    modifier_pan: None,
                    modifier_zoom: None,
                },
                trackpad_pinch_to_zoom_enabled: true,
                ..default()
            },
            AmbientLight {
                brightness: 200.0,
                ..default()
            },
        ))
        .with_children(|cam| {
            cam.spawn((
                DirectionalLight {
                    illuminance: 10_000.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::default(),
            ));
        });
}

/// Despawn any existing `LoadedStlMesh` entity, load the STL at `path`,
/// spawn the converted mesh, and frame the PanOrbitCamera against the
/// loaded geometry's bounding box.
///
/// On parse failure, logs `bevy::log::error!` and returns without
/// spawning — keeps the world valid so a subsequent drop can recover.
fn load_stl_into_world(
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

    let triangles = match stl::load_stl(path) {
        Ok(t) => t,
        Err(e) => {
            error!("STL load failed for {}: {e}", path.display());
            return;
        }
    };
    let bbox = stl::bounding_box(&triangles);

    let mesh_handle = meshes.add(triangles_to_bevy_mesh(&triangles));
    let material_handle = materials.add(StandardMaterial::from(Color::WHITE));
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        LoadedStlMesh,
    ));

    for mut cam in camera.iter_mut() {
        fit_panorbit_to_bbox(&mut cam, &bbox);
    }
}

/// Startup system: if `--load-stl <PATH>` was passed, load it.
fn setup_initial_load(
    args: Res<Args>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior: Query<Entity, With<LoadedStlMesh>>,
    mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    let Some(path) = args.load_stl.as_deref() else {
        return;
    };
    load_stl_into_world(
        path,
        &mut commands,
        &mut meshes,
        &mut materials,
        &prior,
        &mut camera,
    );
}

/// Update system: when one or more files are dropped on the window,
/// load the *last* `DroppedFile` of the tick. If multiple were dropped,
/// log an `info!` naming the chosen one — non-determinism is bounded
/// (last wins).
fn handle_dropped_files(
    mut events: MessageReader<FileDragAndDrop>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior: Query<Entity, With<LoadedStlMesh>>,
    mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    let dropped: Vec<PathBuf> = events
        .read()
        .filter_map(|e| match e {
            FileDragAndDrop::DroppedFile { path_buf, .. } => Some(path_buf.clone()),
            _ => None,
        })
        .collect();
    let Some(path) = dropped.last() else {
        return;
    };
    if dropped.len() > 1 {
        info!(
            "{} files dropped this tick; rendering the last: {}",
            dropped.len(),
            path.display()
        );
    }
    load_stl_into_world(
        path,
        &mut commands,
        &mut meshes,
        &mut materials,
        &prior,
        &mut camera,
    );
}

fn smoke_exit_after_one_frame(mut writer: MessageWriter<AppExit>) {
    writer.write(AppExit::Success);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let smoke_exit = args.smoke_exit;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "resinsim-viz".into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(PanOrbitCameraPlugin)
    .insert_resource(args)
    .add_systems(Startup, (setup_scene, setup_initial_load).chain())
    .add_systems(Update, handle_dropped_files);
    if smoke_exit {
        app.add_systems(Update, smoke_exit_after_one_frame);
    }
    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_startup() -> App {
        let mut app = App::new();
        app.add_systems(Startup, setup_scene);
        app.update();
        app
    }

    #[test]
    fn setup_scene_spawns_camera_with_panorbit() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut cam_q = world.query::<&Camera3d>();
        assert_eq!(cam_q.iter(world).count(), 1, "expected exactly one Camera3d");
        let mut orbit_q = world.query::<&PanOrbitCamera>();
        assert_eq!(
            orbit_q.iter(world).count(),
            1,
            "Camera3d must carry PanOrbitCamera"
        );
    }

    #[test]
    fn setup_scene_spawns_directional_light() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut light_q = world.query::<&DirectionalLight>();
        assert!(
            light_q.iter(world).count() >= 1,
            "expected at least one DirectionalLight"
        );
    }

    #[test]
    fn setup_scene_attaches_ambient_light_to_camera() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<(&Camera3d, &AmbientLight)>();
        assert!(
            q.iter(world).next().is_some(),
            "Camera3d must carry AmbientLight (Bevy 0.18: component on camera, not resource)"
        );
    }

    #[test]
    fn directional_light_is_child_of_camera_for_headlamp() {
        // Headlamp rig: DirectionalLight must be parented to the
        // Camera3d entity so its -Z (light direction) inherits the
        // camera's -Z (view direction). With Transform::default() on
        // the child, light direction == view direction every frame.
        let mut app = run_startup();
        let world = app.world_mut();
        let mut camera_q = world.query_filtered::<Entity, With<Camera3d>>();
        let camera_entity = camera_q
            .iter(world)
            .next()
            .expect("Camera3d entity must exist");

        let mut light_q = world.query::<(&DirectionalLight, &ChildOf)>();
        let mut found_headlamp = false;
        for (_light, child_of) in light_q.iter(world) {
            if child_of.parent() == camera_entity {
                found_headlamp = true;
            }
        }
        assert!(
            found_headlamp,
            "DirectionalLight must be a child of the Camera3d entity (headlamp rig)"
        );
    }

    #[test]
    fn headlamp_directional_light_disables_shadows() {
        // Shadows on a view-aligned light produce self-shadowing acne
        // because every front-facing facet shadows the next. Keep
        // shadows off for the headlamp; rely on ambient + view-aligned
        // diffuse for the inspection view.
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<&DirectionalLight>();
        for light in q.iter(world) {
            assert!(
                !light.shadows_enabled,
                "headlamp DirectionalLight must have shadows_enabled = false"
            );
        }
    }

    #[test]
    fn panorbit_uses_blender_trackpad_behavior() {
        let mut app = run_startup();
        let world = app.world_mut();
        let mut q = world.query::<&PanOrbitCamera>();
        let cam = q
            .iter(world)
            .next()
            .expect("PanOrbitCamera must be present");
        assert!(
            matches!(cam.trackpad_behavior, TrackpadBehavior::BlenderLike { .. }),
            "Mac trackpad config must be BlenderLike, was {:?}",
            cam.trackpad_behavior
        );
        assert!(
            cam.trackpad_pinch_to_zoom_enabled,
            "pinch-to-zoom must be enabled for Mac trackpad"
        );
    }

    fn cube_fixture_path() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/test_cube.stl"
        ))
    }

    /// Assemble an App with just enough plumbing for the loader tests:
    /// `AssetPlugin` (required by `init_asset`), the `Mesh` and
    /// `StandardMaterial` asset stores, and a camera entity carrying a
    /// default `PanOrbitCamera`. No window backend, no rendering.
    fn make_loader_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();
        app.world_mut()
            .spawn((Camera3d::default(), PanOrbitCamera::default()));
        app
    }

    fn count_loaded(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&LoadedStlMesh>();
        q.iter(world).count()
    }

    #[test]
    fn args_resource_reads_load_stl() {
        let mut app = App::new();
        let args = Args {
            smoke_exit: false,
            load_stl: Some(PathBuf::from("foo.stl")),
        };
        app.insert_resource(args);
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert_eq!(stored.load_stl.as_deref(), Some(Path::new("foo.stl")));
        assert!(!stored.smoke_exit);
    }

    #[test]
    fn args_resource_reads_smoke_exit_without_load_stl() {
        // Symmetric case: --smoke-exit alone (no --load-stl) is the
        // existing CI smoke flag; the loader path is the *new* surface.
        // Locks in that the absent-arg field is None, not Some(default).
        let mut app = App::new();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert!(stored.smoke_exit);
        assert!(stored.load_stl.is_none());
    }

    #[test]
    fn load_stl_into_world_spawns_loaded_marker_for_cube() {
        let mut app = make_loader_app();
        let path = cube_fixture_path();
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior: Query<Entity, With<LoadedStlMesh>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior,
                    &mut camera,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("registered system runs");
        app.update(); // flush deferred Commands

        assert_eq!(count_loaded(&mut app), 1, "exactly one LoadedStlMesh after load");

        // Camera radius was updated from default None to Some(1.5 * diagonal).
        let world = app.world_mut();
        let mut q = world.query::<&PanOrbitCamera>();
        let cam = q.iter(world).next().expect("camera entity present");
        assert!(
            cam.radius.is_some(),
            "fit_panorbit_to_bbox should have set radius"
        );
    }

    #[test]
    fn load_stl_into_world_despawns_prior_marker() {
        let mut app = make_loader_app();
        let path = cube_fixture_path();
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior: Query<Entity, With<LoadedStlMesh>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior,
                    &mut camera,
                );
            },
        );

        app.world_mut().run_system(load_id).expect("first run ok");
        app.update();
        assert_eq!(count_loaded(&mut app), 1, "one entity after first load");

        app.world_mut().run_system(load_id).expect("second run ok");
        app.update();
        assert_eq!(
            count_loaded(&mut app),
            1,
            "still one entity after second load — prior was despawned"
        );
    }

    #[test]
    fn load_stl_into_world_with_invalid_path_does_not_spawn() {
        let mut app = make_loader_app();
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.stl");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior: Query<Entity, With<LoadedStlMesh>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior,
                    &mut camera,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(count_loaded(&mut app), 0, "invalid path leaves world empty");
    }

    #[test]
    fn smoke_exit_with_load_stl_flag_runs_setup_initial_load() {
        // Regression guard: --load-stl + --smoke-exit must run the
        // Startup loader path without panic. No window backend needed.
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: Some(cube_fixture_path()),
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        assert_eq!(count_loaded(&mut app), 1, "loader ran during Startup");
    }
}
