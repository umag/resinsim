//! Step definitions for `spec/uat/viz-arrow-keys-step-layer-with-saturation.md`.
//!
//! One scenario, UAT-5: ArrowUp/ArrowDown step the layer cursor and
//! clamp at 0 and max (saturating arithmetic).
//!
//! IN-PROCESS. Builds a minimal Bevy App with synthetic CurrentLayer
//! state (no real `.ctb` fixture). Uses `ButtonInput::press` + `reset_all`
//! for keyboard simulation, matching the unit test pattern in lib.rs
//! (`arrow_up_advances_current_layer_with_saturation`).
//! See `docs/patterns/anti/bevy-button-input-clear-without-input-plugin.md`
//! for why `reset_all` (not `clear`).
//!
//! HUD ASSERTION ADAPTATION. The spec says "HUD line reports Layer N/N".
//! The HUD is a `log_layer_change` system that emits `info!` lines — not
//! observable in-process without a log capture harness. Instead, we assert
//! directly on `CurrentLayer.index`, which is the authoritative source the
//! HUD reads from. The unit tests in lib.rs use the same approach.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use cucumber::{given, then, when};

use resinsim_viz::{CurrentLayer, LayerZPrefix, handle_layer_keys};

use crate::VizWorld;

const SYNTHETIC_MAX: u32 = 9;

fn make_saturation_app(start_index: u32) -> App {
    let mut app = App::new();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<CurrentLayer>()
        .init_resource::<LayerZPrefix>()
        .init_resource::<Time>();

    app.world_mut().resource_mut::<CurrentLayer>().max = SYNTHETIC_MAX;
    app.world_mut().resource_mut::<CurrentLayer>().index = start_index;
    app.add_systems(Update, handle_layer_keys);
    app
}

#[given(
    regex = r#"^the resinsim-viz binary running with --load-ctb \+ matching --load-sim, cursor at the topmost layer$"#
)]
fn given_running_at_topmost(world: &mut VizWorld) {
    let app = make_saturation_app(SYNTHETIC_MAX);
    world.in_process_app = Some(crate::InProcessApp(app));
}

#[when(regex = r#"^the user presses ArrowUp once$"#)]
fn when_press_arrow_up_once(world: &mut VizWorld) {
    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.reset_all();
    keys.press(KeyCode::ArrowUp);
    app.update();
}

#[then(
    regex = r#"^the HUD line still reports "Layer N/N" \(saturated at max\)$"#
)]
fn then_saturated_at_max(world: &mut VizWorld) {
    let app = &world
        .in_process_app
        .as_ref()
        .expect("Given step must initialise in_process_app")
        .0;
    let current = app.world().resource::<CurrentLayer>();
    assert_eq!(
        current.index, SYNTHETIC_MAX,
        "ArrowUp at max should saturate: expected index={SYNTHETIC_MAX}, got {}",
        current.index
    );
}

#[when(regex = r#"^the user presses ArrowDown three times$"#)]
fn when_press_arrow_down_three(world: &mut VizWorld) {
    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    for _ in 0..3 {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::ArrowDown);
        app.update();
    }
}

#[then(
    regex = r#"^the HUD line reports "Layer \(N-3\)/N" with the corresponding cure_depth$"#
)]
fn then_three_below_max(world: &mut VizWorld) {
    let app = &world
        .in_process_app
        .as_ref()
        .expect("Given step must initialise in_process_app")
        .0;
    let current = app.world().resource::<CurrentLayer>();
    let expected = SYNTHETIC_MAX - 3;
    assert_eq!(
        current.index, expected,
        "After 3x ArrowDown from max: expected index={expected}, got {}",
        current.index
    );
}

#[when(regex = r#"^the user presses ArrowDown to step past 0$"#)]
fn when_press_arrow_down_past_zero(world: &mut VizWorld) {
    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    let current_index = app.world().resource::<CurrentLayer>().index;
    for _ in 0..(current_index + 2) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::ArrowDown);
        app.update();
    }
}

#[then(
    regex = r#"^the HUD line reports "Layer 1/N" \(saturated at 0\)$"#
)]
fn then_saturated_at_zero(world: &mut VizWorld) {
    let app = &world
        .in_process_app
        .as_ref()
        .expect("Given step must initialise in_process_app")
        .0;
    let current = app.world().resource::<CurrentLayer>();
    assert_eq!(
        current.index, 0,
        "ArrowDown past 0 should saturate: expected index=0, got {}",
        current.index
    );
}
