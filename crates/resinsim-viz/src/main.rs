mod mesh;
mod slice;

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, TrackpadBehavior};
use clap::Parser;
use resinsim_core::io::{ctb, stl};

use crate::mesh::{LoadedStlMesh, fit_panorbit_to_bbox, triangles_to_bevy_mesh};
use crate::slice::{LoadedSliceStack, slice_stack_bounding_box, slice_stack_to_bevy_mesh};

#[derive(Parser, Debug, Resource)]
#[command(name = "resinsim-viz", about = "Resinsim physics-simulation visualizer")]
struct Args {
    /// Run one frame and exit (smoke-test mode)
    #[arg(long)]
    smoke_exit: bool,

    /// Load an STL file at startup. Drag-drop replaces the loaded mesh at runtime.
    #[arg(long, value_name = "PATH", conflicts_with = "load_ctb")]
    load_stl: Option<PathBuf>,

    /// Load a CTB sliced file at startup. Drag-drop replaces the loaded
    /// geometry at runtime. Mutually exclusive with --load-stl: only one
    /// geometry source is visible at a time in v1.
    #[arg(long, value_name = "PATH")]
    load_ctb: Option<PathBuf>,
}

/// Routing decision for a dropped file path. Pure-fn dispatch keeps
/// `handle_dropped_files` testable without exercising the MessageReader
/// plumbing.
///
/// Lower-cases the file extension before matching so mixed-case
/// extensions (`.CTB`, `.Stl`, `.STL`) route correctly. The core
/// `sliced::detect_format` helper is case-sensitive by design (it
/// matches the on-disk extension verbatim); this routing wrapper sits
/// in front of it for the viz drag-drop ergonomics on macOS, where
/// extensions often arrive in mixed case.
#[derive(Debug, PartialEq, Eq)]
pub enum DropAction {
    Stl,
    Ctb,
    Skip,
}

pub fn route_drop(path: &Path) -> DropAction {
    let lower: Option<String> = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    match lower.as_deref() {
        Some("ctb") => DropAction::Ctb,
        Some("stl") => DropAction::Stl,
        _ => DropAction::Skip,
    }
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

/// Despawn any existing `LoadedStlMesh` AND `LoadedSliceStack` entities,
/// load the STL at `path`, spawn the converted mesh, and frame the
/// PanOrbitCamera against the loaded geometry's bounding box.
///
/// Both prior queries are despawned to enforce mutual exclusion: only
/// one geometry source (STL OR slice stack) is visible at a time in v1.
/// On parse failure, logs `bevy::log::error!` and returns without
/// spawning — keeps the world valid so a subsequent drop can recover.
/// Despawn happens before parse so the world observably reflects the
/// user's last intent even when load fails.
fn load_stl_into_world(
    path: &Path,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior_stl: &Query<Entity, With<LoadedStlMesh>>,
    prior_slice: &Query<Entity, With<LoadedSliceStack>>,
    camera: &mut Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    for entity in prior_stl.iter() {
        commands.entity(entity).despawn();
    }
    for entity in prior_slice.iter() {
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

/// Despawn any existing `LoadedStlMesh` AND `LoadedSliceStack`
/// entities, parse the CTB at `path`, build a voxel-mask-stack mesh
/// from the per-layer `LayerMask`s, spawn the new entity with a
/// `LoadedSliceStack` marker, and frame the PanOrbitCamera against the
/// stack's bounding box.
///
/// Mutual exclusion + fail-soft posture mirror `load_stl_into_world`.
fn load_ctb_into_world(
    path: &Path,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    prior_stl: &Query<Entity, With<LoadedStlMesh>>,
    prior_slice: &Query<Entity, With<LoadedSliceStack>>,
    camera: &mut Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    for entity in prior_stl.iter() {
        commands.entity(entity).despawn();
    }
    for entity in prior_slice.iter() {
        commands.entity(entity).despawn();
    }

    let (_info, layers) = match ctb::parse_ctb(path) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!("CTB load failed for {}: {e}", path.display());
            return;
        }
    };
    let bbox = slice_stack_bounding_box(&layers);

    let mesh_handle = meshes.add(slice_stack_to_bevy_mesh(&layers));
    let material_handle = materials.add(StandardMaterial::from(Color::WHITE));
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        LoadedSliceStack,
    ));

    for mut cam in camera.iter_mut() {
        fit_panorbit_to_bbox(&mut cam, &bbox);
    }
}

/// Startup system: if `--load-stl <PATH>` or `--load-ctb <PATH>` was
/// passed, load it. Clap's `conflicts_with` guarantees at most one is
/// `Some`, so the dispatch is total.
fn setup_initial_load(
    args: Res<Args>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior_stl: Query<Entity, With<LoadedStlMesh>>,
    prior_slice: Query<Entity, With<LoadedSliceStack>>,
    mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>,
) {
    match (args.load_stl.as_deref(), args.load_ctb.as_deref()) {
        (Some(path), None) => load_stl_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &mut camera,
        ),
        (None, Some(path)) => load_ctb_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &mut camera,
        ),
        (None, None) => {}
        // clap's `conflicts_with` makes this unreachable, but the
        // exhaustive match keeps the dispatch total and grep-able.
        (Some(_), Some(_)) => unreachable!(
            "clap conflicts_with should reject --load-stl + --load-ctb at parse time"
        ),
    }
}

/// Update system: when one or more files are dropped on the window,
/// load the *last* `DroppedFile` of the tick. If multiple were dropped,
/// log an `info!` naming the chosen one — non-determinism is bounded
/// (last wins). Routes by extension via `route_drop`: `.stl` → STL
/// loader, `.ctb` → CTB loader, anything else → warn + skip.
fn handle_dropped_files(
    mut events: MessageReader<FileDragAndDrop>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    prior_stl: Query<Entity, With<LoadedStlMesh>>,
    prior_slice: Query<Entity, With<LoadedSliceStack>>,
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
    match route_drop(path) {
        DropAction::Stl => load_stl_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &mut camera,
        ),
        DropAction::Ctb => load_ctb_into_world(
            path,
            &mut commands,
            &mut meshes,
            &mut materials,
            &prior_stl,
            &prior_slice,
            &mut camera,
        ),
        DropAction::Skip => {
            warn!(
                "unsupported drop {} — only .stl and .ctb are rendered",
                path.display()
            );
        }
    }
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

    fn count_loaded_slice(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut q = world.query::<&LoadedSliceStack>();
        q.iter(world).count()
    }

    #[test]
    fn args_resource_reads_load_stl() {
        let mut app = App::new();
        let args = Args {
            smoke_exit: false,
            load_stl: Some(PathBuf::from("foo.stl")),
            load_ctb: None,
        };
        app.insert_resource(args);
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert_eq!(stored.load_stl.as_deref(), Some(Path::new("foo.stl")));
        assert!(stored.load_ctb.is_none());
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
            load_ctb: None,
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert!(stored.smoke_exit);
        assert!(stored.load_stl.is_none());
        assert!(stored.load_ctb.is_none());
    }

    #[test]
    fn args_resource_reads_load_ctb() {
        // Mirror of `args_resource_reads_load_stl` for the new --load-ctb
        // surface. clap's `conflicts_with` enforces mutual exclusion at
        // parse time; this test confirms the resource round-trip when
        // only --load-ctb is set.
        let mut app = App::new();
        app.insert_resource(Args {
            smoke_exit: false,
            load_stl: None,
            load_ctb: Some(PathBuf::from("foo.ctb")),
        });
        let stored = app
            .world()
            .get_resource::<Args>()
            .expect("Args was just inserted as a resource");
        assert!(stored.load_stl.is_none());
        assert_eq!(stored.load_ctb.as_deref(), Some(Path::new("foo.ctb")));
        assert!(!stored.smoke_exit);
    }

    #[test]
    fn load_stl_into_world_spawns_loaded_marker_for_cube() {
        let mut app = make_loader_app();
        let path = cube_fixture_path();
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
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
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
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
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_stl_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
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
            load_ctb: None,
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        assert_eq!(count_loaded(&mut app), 1, "loader ran during Startup");
    }

    #[test]
    fn load_ctb_into_world_with_invalid_path_does_not_spawn() {
        // No CTB writer exists in-tree; we exercise the error path with a
        // path that doesn't resolve. Asserts no LoadedSliceStack and no
        // panic, mirroring the STL test of the same shape.
        let mut app = make_loader_app();
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &mut camera,
                );
            },
        );
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            0,
            "invalid CTB path leaves world empty"
        );
    }

    #[test]
    fn load_ctb_into_world_despawns_prior_loaded_stl() {
        // Mutual exclusion: loading a CTB despawns any LoadedStlMesh, even
        // when the CTB load itself fails. The despawn-before-parse order
        // makes the world observably reflect the user's last intent.
        let mut app = make_loader_app();
        // Synthetic LoadedStlMesh: marker alone is enough — the despawn
        // path doesn't read mesh content, only the marker query.
        app.world_mut().spawn(LoadedStlMesh);
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &mut camera,
                );
            },
        );
        assert_eq!(count_loaded(&mut app), 1, "synthetic LoadedStlMesh present");
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded(&mut app),
            0,
            "prior LoadedStlMesh despawned by the failed CTB load"
        );
    }

    #[test]
    fn load_ctb_into_world_despawns_prior_slice_on_reload() {
        // Same-kind axis: a pre-existing LoadedSliceStack must be
        // despawned before the new (failing) CTB load, so a successful
        // reload would never leave two stacks visible.
        let mut app = make_loader_app();
        app.world_mut().spawn(LoadedSliceStack);
        let bad_path = PathBuf::from("/definitely/does/not/exist/nope.ctb");
        let load_id = app.world_mut().register_system(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  prior_stl: Query<Entity, With<LoadedStlMesh>>,
                  prior_slice: Query<Entity, With<LoadedSliceStack>>,
                  mut camera: Query<&mut PanOrbitCamera, With<Camera3d>>| {
                load_ctb_into_world(
                    &bad_path,
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &prior_stl,
                    &prior_slice,
                    &mut camera,
                );
            },
        );
        assert_eq!(count_loaded_slice(&mut app), 1, "synthetic LoadedSliceStack present");
        app.world_mut()
            .run_system(load_id)
            .expect("system runs even when load fails");
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            0,
            "prior LoadedSliceStack despawned by the failed CTB load"
        );
    }

    #[test]
    fn route_drop_dispatches_by_extension_with_case_folding() {
        // Pure-fn test on `route_drop` — locks in the case-folding
        // contract. macOS drag-drop often emits mixed-case extensions;
        // the core `sliced::detect_format` is case-sensitive by design,
        // so `route_drop` lower-cases the extension before matching.
        let cases: &[(&str, DropAction)] = &[
            ("foo.ctb", DropAction::Ctb),
            ("foo.CTB", DropAction::Ctb),
            ("foo.Ctb", DropAction::Ctb),
            ("foo.stl", DropAction::Stl),
            ("foo.STL", DropAction::Stl),
            ("foo.Stl", DropAction::Stl),
            ("foo.unknown", DropAction::Skip),
            ("nodot", DropAction::Skip),
            ("/abs/path/cube.ctb", DropAction::Ctb),
            ("/abs/path/CUBE.STL", DropAction::Stl),
        ];
        for (input, expected) in cases {
            let actual = route_drop(Path::new(input));
            assert_eq!(
                actual, *expected,
                "route_drop({input:?}) = {actual:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn smoke_exit_with_load_ctb_flag_runs_setup_without_panic() {
        // Env-var-gated smoke test: if RESINSIM_SLICED_FIXTURE points to
        // a real .ctb file, the Startup loader path must run without
        // panic. Without the env var the test no-ops — same convention
        // as `data/test_cube_10mm.ctb.README.md`.
        let Ok(fixture) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
            return;
        };
        let mut app = make_loader_app();
        app.insert_resource(Args {
            smoke_exit: true,
            load_stl: None,
            load_ctb: Some(PathBuf::from(fixture)),
        });
        app.add_systems(Startup, setup_initial_load);
        app.add_systems(Update, smoke_exit_after_one_frame);
        app.update();
        assert_eq!(
            count_loaded_slice(&mut app),
            1,
            "loader ran during Startup and spawned a LoadedSliceStack"
        );
    }
}
