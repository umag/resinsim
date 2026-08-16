//! Step definitions for `spec/uat/viz-load-sim-without-ctb-bad-pairing.md`.
//!
//! One subprocess scenario, UAT-4: `--load-sim` without `--load-ctb`
//! with `--smoke-exit` exits 4 (`EXIT_BAD_SIM_PAIRING`). The same
//! exit-4 path is already exercised by UAT-3 in `viz_screenshot_flag.rs`
//! (through `--screenshot`); this module covers the `--smoke-exit`
//! surface.
//!
//! Shared steps reused from `viz_screenshot_flag.rs` (global cucumber
//! inventory):
//! - `Given the resinsim-viz binary` (no-op)
//! - `Then stderr contains "..."` (substring check)
//! - `And the process exits with code N` (exit code assertion)
//!
//! Fixture substitution: the spec says `<any.sim.json>` — a placeholder
//! for "any valid .sim.json file". This module uses the checked-in
//! `lilith-torso.sim.json` fixture, same fixture UAT-3 uses for the
//! identical reason (a nonexistent path would exit 2 before the pairing
//! check fires). Confirmed empirically: exit 4, stderr contains both
//! expected messages.

use cucumber::then;

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

#[cucumber::when(
    regex = r#"^the user invokes it with --load-sim <any\.sim\.json> --smoke-exit$"#
)]
fn when_invoke_load_sim_without_ctb_smoke_exit(world: &mut VizWorld) {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/lilith-torso.sim.json");
    assert!(
        fixture.exists(),
        "UAT-4's fixture is missing at {}",
        fixture.display(),
    );
    let fixture_str = fixture.to_str().expect("fixture path is UTF-8").to_string();
    world.last = Some(invoke_viz(&["--load-sim", &fixture_str, "--smoke-exit"]));
}

#[then(regex = r#"^stderr mentions that the heatmap requires slice-stack geometry$"#)]
fn then_stderr_mentions_heatmap_slice_stack(world: &mut VizWorld) {
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        outcome.stderr_contains("the heatmap requires slice-stack geometry"),
        "expected stderr to mention heatmap/slice-stack geometry; got:\n{}",
        outcome.stderr,
    );
}
