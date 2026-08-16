//! Step definitions for `spec/uat/viz-arrow-key-step-no-mesh-reupload.md`.
//!
//! One scenario, UAT-6: stepping through layers does not re-upload the
//! slice mesh. Asserts the bake-once contract — ATTRIBUTE_COLOR is
//! byte-identical and Assets<Mesh> count is stable across a full
//! arrow-key traversal.
//!
//! IN-PROCESS. Builds a minimal Bevy App with synthetic layers (no real
//! `.ctb` fixture). Uses `ButtonInput::press` + `reset_all` for keyboard
//! simulation, matching the unit test pattern in lib.rs
//! (`slice_stack_mesh_attribute_color_unmutated_under_arrow_keys`).
//! See `docs/patterns/anti/bevy-button-input-clear-without-input-plugin.md`
//! for why `reset_all` (not `clear`).

use bevy::asset::Assets;
use bevy::input::ButtonInput;
use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;
use cucumber::{given, then, when};

use resinsim_core::io::sliced::LayerInput;
use resinsim_core::values::LayerMask;
use resinsim_viz::{
    CurrentLayer, LayerCursor, LayerZPrefix, LoadedSliceStack,
    cumulative_z_mm, handle_layer_keys, slice_stack_to_bevy_mesh,
    update_layer_cursor,
};

use crate::VizWorld;

const LAYER_COUNT: usize = 3;

fn solid_layer(layer_height_um: f32, w: u32, h: u32, voxel: f32) -> LayerInput {
    let mask = LayerMask::new_all_solid(w, h, voxel)
        .expect("LayerMask::new_all_solid accepts positive dims + voxel");
    LayerInput::new(
        0,
        (w * h) as f64 * (voxel as f64).powi(2),
        1.0,
        60.0,
        layer_height_um,
        0.0,
    )
    .expect("LayerInput::new accepts non-negative area + positive exposure")
    .with_mask(mask)
}

fn read_colors(meshes: &Assets<Mesh>, handle: &Handle<Mesh>) -> Vec<[f32; 4]> {
    let mesh = meshes.get(handle).expect("slice-stack mesh present");
    match mesh
        .attribute(Mesh::ATTRIBUTE_COLOR)
        .expect("ATTRIBUTE_COLOR must be baked on the slice-stack mesh")
    {
        VertexAttributeValues::Float32x4(v) => v.clone(),
        other => panic!("expected Float32x4 colors, got {other:?}"),
    }
}

#[given(
    regex = r#"^the resinsim-viz binary running with --load-ctb \+ matching --load-sim$"#
)]
fn given_running_with_ctb_and_sim(world: &mut VizWorld) {
    let layers: Vec<LayerInput> = (0..LAYER_COUNT)
        .map(|_| solid_layer(50.0, 2, 2, 0.05))
        .collect();
    let colors: Vec<[f32; 4]> = vec![
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
    ];
    let mesh = slice_stack_to_bevy_mesh(&layers, Some(&colors));
    let z_prefix = cumulative_z_mm(&layers);

    let mut app = App::new();
    app.add_plugins(bevy::asset::AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<CurrentLayer>()
        .init_resource::<LayerZPrefix>()
        .init_resource::<Time>();

    let slice_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    app.world_mut().spawn((
        Mesh3d(slice_handle.clone()),
        Transform::default(),
        LoadedSliceStack {
            path: std::path::PathBuf::from("/synthetic"),
        },
    ));
    app.world_mut()
        .spawn((Transform::from_xyz(0.0, 0.0, 0.0), LayerCursor));

    let max = (LAYER_COUNT as u32) - 1;
    app.world_mut().resource_mut::<CurrentLayer>().max = max;
    app.world_mut().resource_mut::<CurrentLayer>().index = 0;
    app.world_mut().resource_mut::<LayerZPrefix>().0 = z_prefix;

    let colors_before = read_colors(app.world().resource::<Assets<Mesh>>(), &slice_handle);
    let mesh_count_before = app.world().resource::<Assets<Mesh>>().iter().count();

    app.add_systems(Update, (handle_layer_keys, update_layer_cursor));

    world.in_process_app = Some(crate::InProcessApp(app));
    world.slice_handle = Some(slice_handle);
    world.colors_before = Some(colors_before);
    world.mesh_count_before = Some(mesh_count_before);
}

#[when(regex = r#"^the user presses ArrowUp/ArrowDown N times$"#)]
fn when_press_arrows_n_times(world: &mut VizWorld) {
    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    let max = app.world().resource::<CurrentLayer>().max;

    for _ in 0..(max + 2) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::ArrowUp);
        app.update();
    }
    assert_eq!(
        app.world().resource::<CurrentLayer>().index,
        max,
        "ArrowUp should saturate at max"
    );
    for _ in 0..(max + 2) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::ArrowDown);
        app.update();
    }
    assert_eq!(
        app.world().resource::<CurrentLayer>().index,
        0,
        "ArrowDown should saturate at 0"
    );
}

#[then(
    regex = r#"^the slice-stack Mesh asset's ATTRIBUTE_COLOR Vec is byte-identical before and after$"#
)]
fn then_colors_identical(world: &mut VizWorld) {
    let app = &world
        .in_process_app
        .as_ref()
        .expect("Given step must initialise in_process_app")
        .0;
    let handle = world.slice_handle.as_ref().expect("slice_handle set by Given");
    let colors_before = world.colors_before.as_ref().expect("colors_before set by Given");
    let colors_after = read_colors(app.world().resource::<Assets<Mesh>>(), handle);
    assert_eq!(
        colors_after, *colors_before,
        "ATTRIBUTE_COLOR Vec must be byte-identical after arrow-key traversal"
    );
}

#[then(regex = r#"^no entry in Assets<Mesh> is added or removed$"#)]
fn then_mesh_count_stable(world: &mut VizWorld) {
    let app = &world
        .in_process_app
        .as_ref()
        .expect("Given step must initialise in_process_app")
        .0;
    let mesh_count_before = world.mesh_count_before.expect("mesh_count_before set by Given");
    let mesh_count_after = app.world().resource::<Assets<Mesh>>().iter().count();
    assert_eq!(
        mesh_count_after, mesh_count_before,
        "no new Mesh assets should be added by cursor / keyboard systems"
    );
}

#[then(
    regex = r#"^the only Transform that changes between frames is the LayerCursor's translation\.z$"#
)]
fn then_only_cursor_transform_changes(world: &mut VizWorld) {
    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    let slice_transform = app
        .world_mut()
        .query::<(&Transform, &LoadedSliceStack)>()
        .iter(app.world())
        .next()
        .map(|(t, _)| *t)
        .expect("LoadedSliceStack entity present");
    assert_eq!(
        slice_transform,
        Transform::default(),
        "LoadedSliceStack Transform must not change during arrow-key traversal"
    );
}
