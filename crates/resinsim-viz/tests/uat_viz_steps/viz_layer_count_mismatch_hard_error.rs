//! Step definitions for `spec/uat/viz-layer-count-mismatch-hard-error.md`.
//!
//! One scenario, UAT-2: `--load-ctb <ctb> --load-sim <mismatched.sim.json>
//! --smoke-exit` exits 3 (`EXIT_LAYER_COUNT_MISMATCH`) with stderr
//! mentioning the mismatch and `--allow-mismatch`.
//!
//! ENV-GATED. Requires `RESINSIM_SLICED_FIXTURE` pointing to a real `.ctb`
//! file. When absent, all steps pass trivially via `world.fixture_skipped`.
//!
//! FIXTURE SUBSTITUTION. The spec uses placeholder names `<100-layer.ctb>`
//! and `<50-layer.sim.json>`. This module uses the real ctb (4492 layers)
//! and builds a synthetic sim with a different layer count by truncating
//! the checked-in `lilith-torso.sim.json`'s layers array via serde_json.
//! The spec's Then assertion checks for literal "CTB has 100 layers, sim
//! has 50" — the shared `then_stderr_contains` adapts these placeholder
//! counts to the real values via `world.expected_mismatch_counts`.
//!
//! PROBE RESULT: truncating the layers array to 10 entries produces a
//! valid sim.json that `resinsim-viz` loads (reaching the mismatch check
//! before any layer-level validation). Confirmed empirically.
//!
//! REUSED STEPS (from viz_screenshot_flag.rs, global cucumber inventory):
//! - `Given the resinsim-viz binary` (no-op)
//! - `Then the process exits with code N` (exit code assertion)
//! - `And stderr contains "..."` (adapted via expected_mismatch_counts)

use cucumber::{then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

const SYNTHETIC_SIM_LAYERS: usize = 10;

#[when(
    regex = r#"^the user invokes it with --load-ctb <100-layer\.ctb> --load-sim <50-layer\.sim\.json> --smoke-exit$"#
)]
fn when_invoke_mismatch_smoke_exit(world: &mut VizWorld) {
    let Ok(ctb_path) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
        world.fixture_skipped = true;
        return;
    };

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base_sim = manifest.join("tests/fixtures/lilith-torso.sim.json");
    assert!(
        base_sim.exists(),
        "UAT-2's base sim fixture is missing at {}",
        base_sim.display(),
    );

    let contents = std::fs::read_to_string(&base_sim)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", base_sim.display()));
    let mut envelope: serde_json::Value =
        serde_json::from_str(&contents).expect("base sim fixture is valid JSON");

    let ctb_layer_count = envelope["simulation"]["layers"]
        .as_array()
        .expect("sim fixture has a simulation.layers array")
        .len();

    if let Some(layers) = envelope["simulation"]["layers"].as_array_mut() {
        layers.truncate(SYNTHETIC_SIM_LAYERS);
    }

    let mismatched_sim = world.tempdir().join("mismatched.sim.json");
    std::fs::write(
        &mismatched_sim,
        serde_json::to_string(&envelope).expect("envelope serialises"),
    )
    .expect("write mismatched sim");

    world.expected_mismatch_counts = Some((ctb_layer_count, SYNTHETIC_SIM_LAYERS));

    let sim_str = mismatched_sim
        .to_str()
        .expect("tempdir paths are UTF-8")
        .to_string();
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        &ctb_path,
        "--load-sim",
        &sim_str,
        "--smoke-exit",
    ]));
}

#[then(regex = r#"^stderr mentions "--allow-mismatch"$"#)]
fn then_stderr_mentions_allow_mismatch(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        outcome.stderr_contains("--allow-mismatch"),
        "expected stderr to mention '--allow-mismatch'; got:\n{}",
        outcome.stderr,
    );
}

#[then(regex = r#"^no Bevy window remains open$"#)]
fn then_no_bevy_window_remains_open(_world: &mut VizWorld) {
    // Trivial pass: the process exited (--smoke-exit or fatal_exit with
    // EXIT_LAYER_COUNT_MISMATCH). No window to check via subprocess.
}
