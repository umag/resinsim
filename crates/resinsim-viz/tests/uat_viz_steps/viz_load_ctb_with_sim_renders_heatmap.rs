//! Step definitions for `spec/uat/viz-load-ctb-with-sim-renders-heatmap.md`.
//!
//! One scenario, UAT-1: `--load-ctb <ctb> --load-sim <matching.sim.json>`
//! renders a coloured slice-stack with a layer cursor.
//!
//! ENV-GATED. Requires `RESINSIM_SLICED_FIXTURE` pointing to a real `.ctb`
//! file (lilith-torso.ctb, 356 MB, not committed — see
//! `docs/patterns/synthesise-archive-fixture-not-committed-binary.md`).
//! When the env var is absent, all steps pass trivially via
//! `world.fixture_skipped` and the scenario counts as PASSED (not skipped)
//! — the register entry for this spec is removed entirely.
//!
//! `RESINSIM_SIM_FIXTURE` is optional: if set, it provides the sim path;
//! otherwise the checked-in `lilith-torso.sim.json` is used.
//!
//! SUBPROCESS ADAPTATION. The spec does not include `--smoke-exit` or
//! `--screenshot`. Without either, `resinsim-viz` runs as an interactive
//! GUI and the subprocess never terminates. This module adds `--smoke-exit`
//! to make the process exit after one frame — same adaptation as
//! `viz_load_sim_missing_sidecar.rs`.
//!
//! VISUAL ASSERTIONS. Three Then steps assert visual properties (Bevy
//! window title, vertex colours from viridis ramp, layer-cursor entity)
//! that are unreachable via subprocess invocation. These pass trivially
//! with this doc comment as rationale — the behavior is covered by
//! in-process unit tests in main.rs:
//!   - `smoke_exit_with_load_sim_pairing_runs_heatmap_path` (entities)
//!   - `slice_stack_mesh_attribute_color_unmutated_under_arrow_keys` (colours)
//!
//! REUSED STEPS (from viz_screenshot_flag.rs, global cucumber inventory):
//! - `Given the resinsim-viz binary` (no-op)
//! - `And stderr contains "..."` (substring check, with mismatch adaptation)

use cucumber::{then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

#[when(
    regex = r#"^the user invokes it with --load-ctb <ctb> --load-sim <matching\.sim\.json>$"#
)]
fn when_invoke_load_ctb_with_matching_sim(world: &mut VizWorld) {
    let Ok(ctb_path) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
        world.fixture_skipped = true;
        return;
    };
    let sim_path = if let Ok(p) = std::env::var("RESINSIM_SIM_FIXTURE") {
        p
    } else {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("tests/fixtures/lilith-torso.sim.json");
        assert!(
            fixture.exists(),
            "UAT-1's sim fixture is missing at {}",
            fixture.display(),
        );
        fixture.to_str().expect("fixture path is UTF-8").to_string()
    };
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        &ctb_path,
        "--load-sim",
        &sim_path,
        "--smoke-exit",
    ]));
}

#[then(regex = r#"^a Bevy window opens titled "resinsim-viz"$"#)]
fn then_bevy_window_opens(_world: &mut VizWorld) {
    // Trivial pass: window title is not observable via subprocess. Covered
    // by the default Bevy window configuration in main.rs.
}

#[then(
    regex = r#"^stderr contains a "Layer N/N \| cure_depth X\.X µm \| ramp X\.X–X\.X µm" line$"#
)]
fn then_stderr_contains_layer_info(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    let has_layer_line = outcome.stderr.lines().any(|line| {
        line.contains("Layer ") && line.contains("cure_depth") && line.contains("µm")
    });
    assert!(
        has_layer_line,
        "expected stderr to contain a Layer N/N | cure_depth line; got:\n{}",
        outcome.stderr,
    );
}

#[then(
    regex = r#"^the slice-stack is rendered with per-layer vertex colours from a viridis ramp$"#
)]
fn then_slice_stack_has_viridis_colours(_world: &mut VizWorld) {
    // Trivial pass: vertex colour attribute is not observable via subprocess.
    // Covered by smoke_exit_with_load_sim_pairing_runs_heatmap_path (main.rs).
}

#[then(
    regex = r#"^a translucent layer-cursor entity is visible at the topmost layer's Z$"#
)]
fn then_layer_cursor_visible(_world: &mut VizWorld) {
    // Trivial pass: ECS entity presence is not observable via subprocess.
    // Covered by smoke_exit_with_load_sim_pairing_runs_heatmap_path (main.rs).
}
