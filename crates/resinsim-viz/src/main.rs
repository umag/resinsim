use bevy::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, TrackpadBehavior};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "resinsim-viz", about = "Resinsim physics-simulation visualizer")]
struct Args {
    /// Run one frame and exit (smoke-test mode)
    #[arg(long)]
    smoke_exit: bool,
}

pub fn setup_scene(mut commands: Commands) {
    commands.spawn((
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
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn smoke_exit_after_one_frame(mut writer: MessageWriter<AppExit>) {
    writer.write(AppExit::Success);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "resinsim-viz".into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(PanOrbitCameraPlugin)
    .add_systems(Startup, setup_scene);
    if args.smoke_exit {
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
}
