//! Step definitions for `spec/uat/viz-load-sim-missing-sidecar.md`.
//!
//! 3 scenarios: UAT-1 and UAT-3 are subprocess CLI checks via
//! `invoke_viz`. UAT-2 (drag-drop without sidecar) is an in-process
//! Bevy App test: injects a synthetic `FileDragAndDrop::DroppedFile`
//! event via `write_message()` and asserts `LoadedSimulation.last_attempt`
//! carries the "missing sidecar" error. Does NOT require egui pointer
//! events — the drag-drop handler reads from Bevy's message queue,
//! not from egui.
//!
//! UAT-1 FEATURE GATE. The sidecar check in
//! `load_and_install_sidecar_with_budget` (simulation_repo.rs:688) is
//! `#[cfg(feature = "field-sim")]`. Without the feature, a Tier-2
//! sim.json with a `fields_sidecar` pointer loads successfully (pointer
//! silently ignored, no error). UAT-1's step functions use a runtime
//! `cfg!(feature = "field-sim")` check with `fixture_skipped` — the
//! same env-gated trivial-pass pattern used for `RESINSIM_SLICED_FIXTURE`
//! scenarios (docs/patterns/env-gated-fixture-with-trivial-pass-step.md).
//! Without the feature, UAT-1 passes trivially; with it, real assertions
//! run.
//!
//! EXIT TRIGGER. Both UAT-1 and UAT-3 add `--smoke-exit` to the
//! invocation (not in the spec's literal command line). Without
//! `--smoke-exit` (or `--screenshot`), `resinsim-viz` runs as an
//! interactive GUI and the subprocess never terminates — same
//! adaptation as `viz_screenshot_flag.rs`'s tier-B scenarios. The
//! `should_propagate_exit_codes` predicate (main.rs:236) requires
//! either flag for `fatal_exit` to fire on load failure.
//!
//! UAT-3 also passes `--v2` to bypass the bad-pairing check
//! (main.rs:964: `--load-sim` without `--load-ctb` exits 4). That
//! check is orthogonal to sidecar validation — it fires on ANY
//! `--load-sim` without `--load-ctb`, regardless of tier. The spec's
//! intent is to test the sidecar-free Tier-1 path, not pairing.
//! `--v2` suppresses the pairing exit so `--smoke-exit` reports the
//! actual load outcome (exit 0 for a clean Tier-1 load).
//!
//! UAT-1 FIXTURE. Reads the checked-in `lilith-torso.sim.json` (Tier-1
//! envelope), injects a synthetic `fields_sidecar` pointer via
//! `serde_json::Value` manipulation, and writes the result to the
//! scenario tempdir as `model.sim.json` WITHOUT creating
//! `model.fields.bin`. The stat() call in
//! `load_and_install_sidecar_with_budget` (simulation_repo.rs:739)
//! fails, producing the `"missing sidecar"` error substring the spec
//! asserts on.
//!
//! UAT-3 FIXTURE. Uses `lilith-torso.sim.json` directly — a Tier-1
//! envelope with no `fields_sidecar` pointer.
//!
//! REUSED STEPS. `then_process_does_not_panic` is defined here and
//! also matches the same prose in other viz specs' scenarios (same
//! shared-step pattern as `viz_screenshot_flag.rs`'s
//! `given_the_resinsim_viz_binary`).

use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use cucumber::{given, then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

// =====================================================================
// UAT-1: --load-sim with missing fields.bin reports missing sidecar
//
// Runtime-gated: cfg!(feature = "field-sim") checked at the first Given
// step. Without the feature, world.fixture_skipped is set and all steps
// pass trivially — same pattern as env-gated .ctb fixture scenarios.
// =====================================================================

#[given(
    regex = r#"^a paired `model\.sim\.json` \+ `model\.fields\.bin` was produced by a previous `resinsim sim --voxel-cure-mm` run$"#
)]
fn given_paired_sim_and_sidecar(world: &mut VizWorld) {
    if !cfg!(feature = "field-sim") {
        world.fixture_skipped = true;
        return;
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lilith-torso.sim.json");
    assert!(
        fixture.exists(),
        "UAT-1's base fixture is missing at {}",
        fixture.display(),
    );
    let contents = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture.display()));
    let mut envelope: serde_json::Value =
        serde_json::from_str(&contents).expect("fixture is valid JSON");
    envelope["fields_sidecar"] = serde_json::json!({
        "path": "model.fields.bin",
        "byte_size": 1024,
        "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "fields_present": ["cure"]
    });
    let original_dir = world.tempdir().join("original");
    std::fs::create_dir(&original_dir).unwrap_or_else(|e| {
        panic!("failed to mkdir {}: {e}", original_dir.display())
    });
    let sim_json = original_dir.join("model.sim.json");
    std::fs::write(
        &sim_json,
        serde_json::to_string_pretty(&envelope).expect("envelope serialises"),
    )
    .expect("write model.sim.json");
}

#[given(
    regex = r#"^the user copies ONLY `model\.sim\.json` into a new directory `/tmp/move-test/` \(leaving the sidecar behind\)$"#
)]
fn given_copy_only_sim_json(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let original = world.tempdir().join("original/model.sim.json");
    assert!(
        original.exists(),
        "scenario invariant: the paired Given step must create original/model.sim.json first"
    );
    let move_test = world.tempdir().join("move-test");
    std::fs::create_dir(&move_test).unwrap_or_else(|e| {
        panic!("failed to mkdir {}: {e}", move_test.display())
    });
    std::fs::copy(&original, move_test.join("model.sim.json"))
        .expect("copy model.sim.json to move-test/");
}

#[when(
    regex = r#"^the user invokes `resinsim-viz --load-sim /tmp/move-test/model\.sim\.json`$"#
)]
fn when_invoke_load_sim_missing_sidecar(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let sim_path = world.tempdir().join("move-test/model.sim.json");
    let sim_str = sim_path
        .to_str()
        .expect("tempdir paths are UTF-8")
        .to_string();
    world.last = Some(invoke_viz(&["--load-sim", &sim_str, "--smoke-exit"]));
}

#[then(regex = r#"^`resinsim-viz` exits with non-zero code$"#)]
fn then_exits_with_non_zero(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then");
    assert_ne!(
        outcome.exit_code, 0,
        "expected non-zero exit code; got 0 (stderr: {})",
        outcome.stderr,
    );
}

#[then(regex = r#"^stderr mentions "missing sidecar"$"#)]
fn then_stderr_mentions_missing_sidecar(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        outcome.stderr_contains("missing sidecar"),
        "expected stderr to mention \"missing sidecar\"; got:\n{}",
        outcome.stderr,
    );
}

#[then(
    regex = r#"^stderr names the expected sidecar location next to the sim\.json$"#
)]
fn then_stderr_names_sidecar_location(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        outcome.stderr_contains("model.fields.bin"),
        "expected stderr to name the sidecar file \"model.fields.bin\"; got:\n{}",
        outcome.stderr,
    );
}

#[then(regex = r#"^the process does not panic$"#)]
fn then_process_does_not_panic(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        !outcome.stderr.contains("panicked at"),
        "process panicked; stderr:\n{}",
        outcome.stderr,
    );
    assert!(
        !outcome.stderr.contains("stack backtrace"),
        "process produced a stack backtrace (likely panic); stderr:\n{}",
        outcome.stderr,
    );
}

// =====================================================================
// UAT-2: drag-drop without sidecar produces typed error
//
// IN-PROCESS. Builds a minimal Bevy App with handle_dropped_files,
// injects FileDragAndDrop::DroppedFile via write_message(), and
// checks LoadedSimulation.last_attempt for the "missing sidecar"
// error.
//
// Runtime-gated: cfg!(feature = "field-sim") like UAT-1, because the
// sidecar check in load_and_install_sidecar_with_budget is cfg-gated.
// =====================================================================

#[given(regex = r#"^resinsim-viz is running with no sim loaded$"#)]
fn given_viz_running_no_sim(world: &mut VizWorld) {
    if !cfg!(feature = "field-sim") {
        world.fixture_skipped = true;
        return;
    }
    use clap::Parser;
    use resinsim_viz::{
        ActivePrinterProfile, CurrentLayer, CureDepthDomain, LayerZPrefix,
        LoadedSimulation, LoadedSliceMasks, PrinterEnvelope, handle_dropped_files,
    };

    let mut app = App::new();
    app.add_plugins(bevy::asset::AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_message::<FileDragAndDrop>()
        .add_message::<AppExit>()
        .insert_resource(resinsim_viz::Args::parse_from(["resinsim-viz"]))
        .init_resource::<LoadedSimulation>()
        .init_resource::<LoadedSliceMasks>()
        .init_resource::<CurrentLayer>()
        .init_resource::<LayerZPrefix>()
        .init_resource::<CureDepthDomain>()
        .init_resource::<ActivePrinterProfile>()
        .init_resource::<PrinterEnvelope>()
        .add_systems(Update, handle_dropped_files);

    world.in_process_app = Some(crate::InProcessApp(app));
}

#[when(
    regex = r#"^the user drags `model\.sim\.json` from a directory that does NOT contain `model\.fields\.bin` into the viewer window$"#
)]
fn when_drag_sim_json_without_sidecar(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lilith-torso.sim.json");
    let contents = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture.display()));
    let mut envelope: serde_json::Value =
        serde_json::from_str(&contents).expect("fixture is valid JSON");
    envelope["fields_sidecar"] = serde_json::json!({
        "path": "model.fields.bin",
        "byte_size": 1024,
        "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "fields_present": ["cure"]
    });
    let drop_dir = world.tempdir().join("drop-no-sidecar");
    std::fs::create_dir(&drop_dir).unwrap_or_else(|e| {
        panic!("failed to mkdir {}: {e}", drop_dir.display())
    });
    let sim_path = drop_dir.join("model.sim.json");
    std::fs::write(
        &sim_path,
        serde_json::to_string_pretty(&envelope).expect("envelope serialises"),
    )
    .expect("write model.sim.json");

    let app = &mut world
        .in_process_app
        .as_mut()
        .expect("Given step must initialise in_process_app")
        .0;
    app.world_mut()
        .write_message(FileDragAndDrop::DroppedFile {
            window: Entity::PLACEHOLDER,
            path_buf: sim_path,
        });
    app.update();
}

// spec/uat/viz-load-sim-missing-sidecar.md UAT-2
#[then(
    regex = r#"^the in-app error toast / status mentions "missing sidecar"$"#
)]
fn then_in_app_error_mentions_missing_sidecar(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let app = &world
        .in_process_app
        .as_ref()
        .expect("scenario invariant: When step must run before Then")
        .0;
    let loaded_sim = app
        .world()
        .get_resource::<resinsim_viz::LoadedSimulation>()
        .expect("LoadedSimulation resource must be present");
    let err = loaded_sim
        .last_attempt
        .as_ref()
        .expect("last_attempt must be set after a drop attempt")
        .as_ref()
        .expect_err("last_attempt must be Err for a missing-sidecar drop");
    assert!(
        err.contains("missing sidecar"),
        "expected error to mention \"missing sidecar\"; got: {err}",
    );
}

// spec/uat/viz-load-sim-missing-sidecar.md UAT-2
#[then(
    regex = r#"^resinsim-viz remains running \(drop failure is not fatal\)$"#
)]
fn then_viz_remains_running(world: &mut VizWorld) {
    if world.fixture_skipped {
        return;
    }
    let app = &world
        .in_process_app
        .as_ref()
        .expect("scenario invariant: When step must run before Then")
        .0;
    let messages = app.world().resource::<Messages<AppExit>>();
    assert!(
        messages.is_empty(),
        "handle_dropped_files must not send AppExit on a drop failure",
    );
}

// =====================================================================
// UAT-3: Tier-1 envelope without fields_sidecar pointer loads cleanly
// =====================================================================

#[given(
    regex = r#"^a `tier1\.sim\.json` envelope WITHOUT a `fields_sidecar` pointer \(i\.e\. produced by `resinsim sim` without `--voxel-cure-mm`\)$"#
)]
fn given_tier1_envelope(world: &mut VizWorld) {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lilith-torso.sim.json");
    assert!(
        fixture.exists(),
        "UAT-3's Tier-1 fixture is missing at {}",
        fixture.display(),
    );
    let tier1_dir = world.tempdir().join("tier1");
    std::fs::create_dir(&tier1_dir).unwrap_or_else(|e| {
        panic!("failed to mkdir {}: {e}", tier1_dir.display())
    });
    std::fs::copy(&fixture, tier1_dir.join("tier1.sim.json"))
        .expect("copy lilith-torso.sim.json as tier1.sim.json");
}

#[when(
    regex = r#"^the user invokes `resinsim-viz --load-sim tier1\.sim\.json`$"#
)]
fn when_invoke_load_sim_tier1(world: &mut VizWorld) {
    let sim_path = world.tempdir().join("tier1/tier1.sim.json");
    let sim_str = sim_path
        .to_str()
        .expect("tempdir paths are UTF-8")
        .to_string();
    // --v2 bypasses the bad-pairing check (--load-sim without --load-ctb
    // → exit 4) which is orthogonal to sidecar validation. See module doc.
    world.last = Some(invoke_viz(&[
        "--load-sim", &sim_str, "--smoke-exit", "--v2",
    ]));
}

#[then(regex = r#"^the load succeeds$"#)]
fn then_load_succeeds(world: &mut VizWorld) {
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then");
    assert_eq!(
        outcome.exit_code, 0,
        "expected exit code 0 (load succeeds); got {} (stderr: {})",
        outcome.exit_code, outcome.stderr,
    );
}

#[then(regex = r#"^no error about a missing sidecar appears$"#)]
fn then_no_missing_sidecar_error(world: &mut VizWorld) {
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        !outcome.stderr_contains("missing sidecar"),
        "expected stderr NOT to mention \"missing sidecar\"; got:\n{}",
        outcome.stderr,
    );
}
