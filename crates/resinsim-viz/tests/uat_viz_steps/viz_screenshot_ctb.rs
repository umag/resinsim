//! Step definitions for `spec/uat/viz-screenshot-flag.md` UAT-2, UAT-5,
//! and UAT-8 — the three scenarios that depend on a real `.ctb` fixture
//! via `RESINSIM_SLICED_FIXTURE`.
//!
//! These are a SECOND step-def module for the same spec. The primary
//! module (`viz_screenshot_flag.rs`) covers UAT-1/3/4/7a/7b/7c/9 (no
//! fixture needed). This module covers the remaining fixture-dependent
//! scenarios. Unlike core's `STEP_DEF_MODULE_RENAMES` pattern
//! (docs/patterns/two-step-def-modules-for-one-spec.md), the viz
//! harness does not use renames — the register's spec name is inferred
//! from the module name, so this module's name deliberately does NOT
//! match the spec stem. The use-binding in `uat_viz_gherkin.rs` is
//! sufficient for cucumber's global inventory.
//!
//! ENV-GATED. When `RESINSIM_SLICED_FIXTURE` is absent, all steps pass
//! trivially via `world.fixture_skipped`.
//!
//! `RESINSIM_SIM_FIXTURE` is optional for UAT-2: if set, it provides the
//! matching sim; otherwise the checked-in `lilith-torso.sim.json` is used.
//!
//! UAT-8 FIXTURE. Builds a truncated sim (same pattern as
//! `viz_layer_count_mismatch_hard_error.rs`) to create a layer-count
//! mismatch for the `--allow-mismatch` scenario.
//!
//! REUSED STEPS (from viz_screenshot_flag.rs, global cucumber inventory):
//! - `Then the process exits with code N` (exit code assertion)
//! - `And /tmp/uatN.png exists` (file existence check)
//! - `And stderr contains "..."` (substring check)

use cucumber::{given, then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

const SYNTHETIC_SIM_LAYERS: usize = 10;

// =====================================================================
// Shared Given steps for env-gated fixtures
// =====================================================================

#[given(regex = r#"^a fixture \.ctb file at \$RESINSIM_SLICED_FIXTURE$"#)]
fn given_ctb_fixture_at_env_var(world: &mut VizWorld) {
    let Ok(ctb_path) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
        world.fixture_skipped = true;
        return;
    };
    assert!(
        std::path::Path::new(&ctb_path).exists(),
        "RESINSIM_SLICED_FIXTURE={ctb_path} does not exist"
    );
}

#[given(regex = r#"^a matching sim JSON at \$RESINSIM_SIM_FIXTURE$"#)]
fn given_matching_sim_at_env_var(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    if let Ok(sim_path) = std::env::var("RESINSIM_SIM_FIXTURE") {
        assert!(
            std::path::Path::new(&sim_path).exists(),
            "RESINSIM_SIM_FIXTURE={sim_path} does not exist"
        );
    }
    // If RESINSIM_SIM_FIXTURE is unset, the When step falls back to
    // the checked-in lilith-torso.sim.json.
}

#[given(regex = r#"^a fixture \.ctb with N layers$"#)]
fn given_ctb_with_n_layers(world: &mut VizWorld) {
    let Ok(ctb_path) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
        world.fixture_skipped = true;
        return;
    };
    assert!(
        std::path::Path::new(&ctb_path).exists(),
        "RESINSIM_SLICED_FIXTURE={ctb_path} does not exist"
    );
}

#[given(regex = r#"^a sim JSON with M ≠ N layers$"#)]
fn given_sim_with_different_layers(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    // The truncated sim is built in the When step (needs tempdir).
}

// =====================================================================
// UAT-2: --screenshot captures the issue-03 visual surface
// =====================================================================

#[when(
    regex = r#"^the user invokes it with --load-ctb <ctb> --load-sim <sim> --screenshot /tmp/uat2\.png$"#
)]
fn when_invoke_ctb_sim_screenshot_uat2(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let ctb_path =
        std::env::var("RESINSIM_SLICED_FIXTURE").expect("fixture_skipped guards this path");
    let sim_path = if let Ok(p) = std::env::var("RESINSIM_SIM_FIXTURE") {
        p
    } else {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("tests/fixtures/lilith-torso.sim.json");
        assert!(
            fixture.exists(),
            "UAT-2's sim fixture is missing at {}",
            fixture.display(),
        );
        fixture.to_str().expect("fixture path is UTF-8").to_string()
    };
    let target = world.tempdir().join("uat2.png");
    let target_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        &ctb_path,
        "--load-sim",
        &sim_path,
        "--screenshot",
        &target_str,
    ]));
}

// =====================================================================
// UAT-5: --screenshot produces a non-trivial PNG
// =====================================================================

#[when(
    regex = r#"^the user invokes it with --load-ctb <ctb> --screenshot /tmp/uat5\.png$"#
)]
fn when_invoke_ctb_screenshot_uat5(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let ctb_path =
        std::env::var("RESINSIM_SLICED_FIXTURE").expect("fixture_skipped guards this path");
    let target = world.tempdir().join("uat5.png");
    let target_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        &ctb_path,
        "--screenshot",
        &target_str,
    ]));
}

// =====================================================================
// UAT-8: --screenshot with --allow-mismatch tolerates layer-count mismatch
// =====================================================================

#[when(
    regex = r#"^the user invokes it with --load-ctb <ctb> --load-sim <sim> --allow-mismatch --screenshot /tmp/uat8\.png$"#
)]
fn when_invoke_ctb_sim_allow_mismatch_screenshot_uat8(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let ctb_path =
        std::env::var("RESINSIM_SLICED_FIXTURE").expect("fixture_skipped guards this path");

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base_sim = manifest.join("tests/fixtures/lilith-torso.sim.json");
    let contents = std::fs::read_to_string(&base_sim)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", base_sim.display()));
    let mut envelope: serde_json::Value =
        serde_json::from_str(&contents).expect("base sim fixture is valid JSON");
    if let Some(layers) = envelope["simulation"]["layers"].as_array_mut() {
        layers.truncate(SYNTHETIC_SIM_LAYERS);
    }
    let mismatched_sim = world.tempdir().join("uat8-mismatch.sim.json");
    std::fs::write(
        &mismatched_sim,
        serde_json::to_string(&envelope).expect("envelope serialises"),
    )
    .expect("write mismatched sim");

    let sim_str = mismatched_sim
        .to_str()
        .expect("tempdir paths are UTF-8")
        .to_string();
    let target = world.tempdir().join("uat8.png");
    let target_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        &ctb_path,
        "--load-sim",
        &sim_str,
        "--allow-mismatch",
        "--screenshot",
        &target_str,
    ]));
}

// =====================================================================
// Shared Then steps for screenshot-ctb scenarios
// =====================================================================

#[then(regex = r#"^the file size of /tmp/(uat\d+\.png) is > (\d+) bytes$"#)]
fn then_file_size_exceeds(world: &mut VizWorld, filename: String, min_bytes: u64) {
    if world.fixture_skipped {
        return;
    }
    let target = world.tempdir().join(&filename);
    let metadata = std::fs::metadata(&target).unwrap_or_else(|e| {
        panic!(
            "expected {} to exist for size check; error: {e}",
            target.display()
        )
    });
    assert!(
        metadata.len() > min_bytes,
        "expected {} to be > {min_bytes} bytes; got {} bytes",
        target.display(),
        metadata.len(),
    );
}

#[then(regex = r#"^`file /tmp/(uat\d+\.png)` reports a valid PNG image$"#)]
fn then_file_command_reports_png(world: &mut VizWorld, filename: String) {
    if world.fixture_skipped {
        return;
    }
    let target = world.tempdir().join(&filename);
    let output = std::process::Command::new("file")
        .arg(target.as_os_str())
        .output()
        .expect("'file' command must be available on macOS and Linux");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PNG image"),
        "expected `file` to report PNG image for {}; got: {stdout}",
        target.display(),
    );
}

#[then(
    regex = r#"^the agent reading the PNG observes a coloured slice-stack with a layer cursor$"#
)]
fn then_agent_observes_coloured_slice_stack(_world: &mut VizWorld) {
    // Trivial pass: visual observation is not automatable via subprocess.
    // The PNG is captured; a human or multimodal AI agent can inspect it.
}
