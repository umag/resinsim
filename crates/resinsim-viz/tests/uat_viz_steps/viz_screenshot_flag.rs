//! Step definitions for `spec/uat/viz-screenshot-flag.md`.
//!
//! TIER A (this module, step 4 of viz-uat-cucumber-harness): UAT-7a/7b/7c/7d
//! — renderer-free, `--screenshot` path validation exits 5 via
//! `std::process::exit` at main.rs BEFORE `App::new()`. No window, no
//! wgpu, no LogPlugin; these run on any machine, display or not.
//!
//! UAT-7d ("empty path → exit 5") was previously declared debt because
//! clap 4.6.1 rejected empty `--screenshot` values (exit 2) before
//! `main()`'s `validate_screenshot_path` ran. Fixed by adding a custom
//! `value_parser` on the `--screenshot` arg that accepts any string
//! including empty, letting our `PathError::Empty` validation handle it
//! (exit 5 + "is empty" as the spec requires).
//!
//! TIER B (this module, step 5): UAT-1/3/4/9 — renderer-dependent. Each
//! opens a real window for a few seconds; `cargo uat-viz`'s `main()` runs
//! [`assert_renderer_available`] once, before cucumber, and PANICS
//! (never skips) if the environment cannot render — see that function's
//! doc comment. Confirmed by step 1's probe: this machine has a working
//! discrete GPU (AMD Radeon Pro 5500M via Metal), so tier B lands.
//!
//! UAT-3 IS THE ONE SCENARIO WHERE THE SPEC'S OWN ARGUMENTS ARE NOT USED
//! VERBATIM. The spec says `--load-sim foo.sim.json --screenshot ...`
//! expecting exit 4 (`EXIT_BAD_SIM_PAIRING`), but `foo.sim.json` does not
//! exist, and step 1's probe found that a nonexistent `--load-sim` path
//! queues `EXIT_SIM_LOAD_FAILED=2` FIRST (main.rs, `setup_initial_load`)
//! and wins the race over `EXIT_BAD_SIM_PAIRING=4` when both are queued
//! in the same Startup tick — even though the bad-pairing stderr line
//! DOES still get logged. Per the plan's pre-approved resolution: this
//! step substitutes the checked-in valid fixture
//! `crates/resinsim-viz/tests/fixtures/lilith-torso.sim.json` for
//! `foo.sim.json`, so the sim loads successfully and ONLY the pairing
//! failure fires — confirmed empirically at step 1 (exit 4, stderr
//! contains the pairing message, no PNG written). This is faithful to
//! the scenario's intent ("any .sim.json", not a specific one) — it is
//! NOT the forbidden move of weakening the assertion to accept either
//! exit code, and it is NOT an edit to the spec text.
//!
//! Scenario-scoped OUTPUT paths (where `resinsim-viz` writes/would write
//! a PNG) come from `VizWorld::tempdir`, never a hardcoded `/tmp`, even
//! though the spec's own prose names literal `/tmp/...` paths — using
//! the real literal path would still be CORRECT for what every scenario
//! in this file asserts (all are path-shape / exit-code / stderr-substring
//! checks, never filesystem-location checks), but a scenario-scoped
//! tempdir means parallel runs (and repeat runs leaving stale state)
//! cannot collide or leave litter. UAT-9's INPUT path `/nonexistent.ctb`
//! is kept literal (not tempdir-redirected) — it must not exist, an
//! absolute top-level path serves that purpose as well as any tempdir
//! path would, and it matches the spec's own literal text exactly.

use cucumber::{given, then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

/// Renderer preflight, run ONCE from `main()` before cucumber (see
/// `uat_viz_gherkin.rs::main`). Performs one real `--screenshot`
/// invocation and PANICS with an actionable message on exit 8
/// (`EXIT_SCREENSHOT_RENDER_TIMEOUT`, ADR-0013: "headless config /
/// software rasterizer"), a spawn failure, or a missing output file —
/// NEVER skips. A silent skip here would turn "the environment has no
/// renderer" into four confusing individual scenario failures with
/// diffs about missing PNGs; this turns it into one line diagnosing the
/// actual cause. Exit 8 is a DISCRIMINATOR, never an assertion target.
pub fn assert_renderer_available() {
    let dir = tempfile::tempdir().expect("create renderer-preflight tempdir");
    let path = dir.path().join("preflight.png");
    let path_str = path.to_str().expect("tempdir paths are UTF-8");
    let outcome = invoke_viz(&["--screenshot", path_str]);
    match outcome.exit_code {
        0 if path.exists() => {}
        8 => panic!(
            "cargo uat-viz needs a GUI session with a working renderer; the preflight \
             capture exited 8 (EXIT_SCREENSHOT_RENDER_TIMEOUT — ADR-0013: headless config \
             or software rasterizer). Run from a desktop session, or run `cargo uat-viz` \
             on a machine with a GPU. (This message is why tier B panics instead of \
             skipping — see docs/adr/0024-second-uat-harness-in-resinsim-viz.md.)"
        ),
        code => panic!(
            "renderer preflight failed unexpectedly: exit {code}, PNG exists: {}, stderr:\n{}",
            path.exists(),
            outcome.stderr,
        ),
    }
}

// ---------------------------------------------------------------------
// UAT-7a: directory path -> exit 5, "is a directory"
// ---------------------------------------------------------------------

#[given(regex = r#"^/tmp exists as a directory$"#)]
fn given_tmp_exists_as_directory(_world: &mut VizWorld) {
    // Trivially true on any Unix CI/dev machine this suite targets; the
    // scenario's actual directory-under-test is scenario-scoped (see
    // module doc), so this step only documents the spec's premise.
    assert!(
        std::path::Path::new("/tmp").is_dir(),
        "/tmp does not exist as a directory on this machine"
    );
}

/// UAT-7a's "When" stages the intended `--screenshot` path but does NOT
/// invoke yet — the spec's own next line ("is created as a directory
/// before launch") makes the launch conditional on that setup step
/// running first, even though textually "When...invokes" appears before
/// the "And...is created" line. Invocation is deferred to
/// [`and_created_as_directory_before_launch`] below, which runs the
/// mkdir THEN launches — "before launch" made literally true.
#[when(regex = r#"^the user invokes resinsim-viz --screenshot /tmp/uat7-dir\.png$"#)]
fn when_invoke_screenshot_dir_path_staged(world: &mut VizWorld) {
    let target = world.tempdir().join("uat7-dir.png");
    world.pending_screenshot_path = Some(target);
}

// This line is written `And ...` in the spec, but gherkin resolves an
// `And` step's TYPE to whatever preceded it in the same block — here
// the `When` above — so cucumber-rs matches it against `#[when]`-
// registered regexes, not a (nonexistent) `#[and]` macro. Verified
// against cucumber-codegen 0.22.1's `steps!(given, when, then)` (no
// `and`/`but` macros exist) and gherkin 0.15.0's parser, which builds
// an `And` step with `.ty(env.last_step())`.
#[when(regex = r#"^/tmp/uat7-dir\.png is created as a directory before launch$"#)]
fn and_created_as_directory_before_launch(world: &mut VizWorld) {
    let target = world
        .pending_screenshot_path
        .clone()
        .expect("scenario invariant: the staged When step must run first");
    std::fs::create_dir(&target).unwrap_or_else(|e| {
        panic!("failed to mkdir scenario-scoped directory {}: {e}", target.display())
    });
    let path_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&["--screenshot", &path_str]));
}

// ---------------------------------------------------------------------
// UAT-7b: missing parent -> exit 5, "parent dir"
// ---------------------------------------------------------------------

#[when(regex = r#"^the user invokes resinsim-viz --screenshot /no/such/dir/x\.png$"#)]
fn when_invoke_screenshot_missing_parent(world: &mut VizWorld) {
    // Any nonexistent parent triggers PathError::ParentMissing; the
    // literal "/no/such/dir" from the spec is preserved as a path
    // COMPONENT under the scenario tempdir so the assertion's intent
    // (parent genuinely absent) holds without writing outside the
    // sandbox.
    let target = world.tempdir().join("no-such-dir").join("x.png");
    let path_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&["--screenshot", &path_str]));
}

// ---------------------------------------------------------------------
// UAT-7c: wrong extension -> exit 5, "unsupported extension"
// ---------------------------------------------------------------------

#[when(regex = r#"^the user invokes resinsim-viz --screenshot /tmp/x\.txt$"#)]
fn when_invoke_screenshot_bad_extension(world: &mut VizWorld) {
    let target = world.tempdir().join("x.txt");
    let path_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&["--screenshot", &path_str]));
}

// ---------------------------------------------------------------------
// UAT-7d: empty path -> exit 5, "is empty"
// ---------------------------------------------------------------------

#[when(regex = r#"^the user invokes resinsim-viz --screenshot ""$"#)]
fn when_invoke_screenshot_empty_path(world: &mut VizWorld) {
    world.last = Some(invoke_viz(&["--screenshot", ""]));
}

// ---------------------------------------------------------------------
// Shared Then/And assertions (UAT-7a/7b/7c/7d; UAT-1/3/4/9 reuse the
// same exit-code step in step 5).
// ---------------------------------------------------------------------

#[then(regex = r#"^the process exits with code (\d+)$"#)]
fn then_process_exits_with_code(world: &mut VizWorld, code: u8) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then");
    assert_eq!(
        outcome.exit_code, i32::from(code),
        "expected exit code {code}; got {} (stderr: {})",
        outcome.exit_code, outcome.stderr,
    );
}

// One registration covers both the literal `Then stderr contains "..."`
// (UAT-7c) and `And stderr contains "..."` (UAT-7a/7b/UAT-1/4) lines —
// an `And` following a `Then` resolves to StepType::Then (same mechanism
// as above), so cucumber matches both against this single `#[then]`.
/// When `expected_mismatch_counts` is set (by a mismatch scenario's When
/// step), this replaces the spec's placeholder layer counts in the needle
/// with real fixture counts before checking. The replacement targets are
/// the EXACT placeholder text from `viz-layer-count-mismatch-hard-error`'s
/// spec ("CTB has 100 layers", "sim has 50"). If future mismatch specs use
/// different placeholder counts, this list must be extended — a documented
/// coupling point, not a generic pattern matcher.
#[then(regex = r#"^stderr contains "([^"]*)"$"#)]
fn then_stderr_contains(world: &mut VizWorld, needle: String) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    let effective_needle =
        if let Some((ctb_layers, sim_layers)) = world.expected_mismatch_counts {
            needle
                .replace(
                    "CTB has 100 layers",
                    &format!("CTB has {ctb_layers} layers"),
                )
                .replace("sim has 50", &format!("sim has {sim_layers}"))
        } else {
            needle
        };
    assert!(
        outcome.stderr_contains(&effective_needle),
        "expected stderr to contain {effective_needle:?}; got:\n{}",
        outcome.stderr,
    );
}

// ---------------------------------------------------------------------
// TIER B (step 5): UAT-1/3/4/9
// ---------------------------------------------------------------------

// "Given the resinsim-viz binary" is IDENTICAL, verbatim prose in FIVE
// viz specs (viz-allow-mismatch-soft-fallback, viz-load-ctb-with-sim-
// renders-heatmap, viz-layer-count-mismatch-hard-error, viz-load-sim-
// without-ctb-bad-pairing, and this one). Cucumber's step inventory is
// global per binary, so this single registration ALSO satisfies that
// opening Given in the other four specs' still-unstepped scenarios —
// same shared-step-for-identical-prose pattern core already uses
// (implementation-conventions.md's note on ctb_layer_height_authority.rs's
// generalised exit-code step). This is benign: each of those scenarios'
// NEXT step remains genuinely undefined, so the scenario still counts
// as skipped under the skipped-steps-equals-skipped-scenarios metric
// identity, and the register's expected counts for those specs are
// unaffected. Confirmed empirically at step 5 (register matched exactly
// on first run with this step present).
#[given(regex = r#"^the resinsim-viz binary$"#)]
fn given_the_resinsim_viz_binary(_world: &mut VizWorld) {
    // No-op: documents the scenario's premise. The binary's existence is
    // guaranteed by cargo before this test binary runs at all
    // (env!("CARGO_BIN_EXE_resinsim-viz") — see viz_cli.rs's doc comment).
}

#[given(regex = r#"^the file /nonexistent\.ctb does NOT exist$"#)]
fn given_nonexistent_ctb_does_not_exist(_world: &mut VizWorld) {
    assert!(
        !std::path::Path::new("/nonexistent.ctb").exists(),
        "/nonexistent.ctb unexpectedly exists on this machine — UAT-9's premise is violated"
    );
}

/// UAT-1: `--screenshot writes a PNG of the default scene and exits 0`.
#[when(regex = r#"^the user invokes it with --screenshot /tmp/uat1\.png$"#)]
fn when_invoke_it_with_screenshot_uat1(world: &mut VizWorld) {
    let target = world.tempdir().join("uat1.png");
    let path_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&["--screenshot", &path_str]));
}

/// UAT-3: `--screenshot propagates exit-4 on bad sim pairing`. See this
/// module's doc comment for the `foo.sim.json` -> checked-in fixture
/// substitution and why it is faithful, not a weakening.
#[when(
    regex = r#"^the user invokes it with --load-sim foo\.sim\.json --screenshot /tmp/uat3\.png$"#
)]
fn when_invoke_it_with_load_sim_bad_pairing_uat3(world: &mut VizWorld) {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/lilith-torso.sim.json");
    assert!(
        fixture.exists(),
        "UAT-3's substitute fixture is missing at {}",
        fixture.display(),
    );
    let fixture_str = fixture.to_str().expect("fixture path is UTF-8").to_string();
    let target = world.tempdir().join("uat3.png");
    let target_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&[
        "--load-sim",
        &fixture_str,
        "--screenshot",
        &target_str,
    ]));
}

/// UAT-4: `--screenshot wins over --smoke-exit`.
#[when(regex = r#"^the user invokes it with --smoke-exit --screenshot /tmp/uat4\.png$"#)]
fn when_invoke_it_with_smoke_exit_and_screenshot_uat4(world: &mut VizWorld) {
    let target = world.tempdir().join("uat4.png");
    let path_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&["--smoke-exit", "--screenshot", &path_str]));
}

/// UAT-9: `--screenshot propagates exit-6 on CTB load failure`.
#[when(
    regex = r#"^the user invokes it with --load-ctb /nonexistent\.ctb --screenshot /tmp/uat9\.png$"#
)]
fn when_invoke_it_with_load_ctb_nonexistent_uat9(world: &mut VizWorld) {
    let target = world.tempdir().join("uat9.png");
    let target_str = target.to_str().expect("tempdir paths are UTF-8").to_string();
    world.last = Some(invoke_viz(&[
        "--load-ctb",
        "/nonexistent.ctb",
        "--screenshot",
        &target_str,
    ]));
}

/// Covers both `Then the file /tmp/uat1.png exists` (UAT-1) and
/// `And /tmp/uat4.png exists` (UAT-4) — the optional "the file " prefix
/// is the only textual difference between the two spec lines.
#[then(regex = r#"^(?:the file )?/tmp/(uat\d+\.png) exists$"#)]
fn then_screenshot_file_exists(world: &mut VizWorld, filename: String) {
    if world.fixture_skipped {
        return;
    }
    let target = world.tempdir().join(&filename);
    assert!(
        target.exists(),
        "expected {} to exist after invocation; it does not (stderr: {})",
        target.display(),
        world.last.as_ref().map_or("<no invocation>", |o| &o.stderr),
    );
}

/// Covers `And /tmp/uat3.png does NOT exist` (UAT-3) and
/// `And /tmp/uat9.png does NOT exist` (UAT-9).
#[then(regex = r#"^/tmp/(uat\d+\.png) does NOT exist$"#)]
fn then_screenshot_file_does_not_exist(world: &mut VizWorld, filename: String) {
    if world.fixture_skipped {
        return;
    }
    let target = world.tempdir().join(&filename);
    assert!(
        !target.exists(),
        "expected {} to NOT exist after invocation; it does (stderr: {})",
        target.display(),
        world.last.as_ref().map_or("<no invocation>", |o| &o.stderr),
    );
}

/// UAT-9's grep-contract assertion. Same practical check as
/// `then_stderr_contains` (substring match); a separate regex because
/// the spec phrases this one as "stderr matches ... followed by the
/// underlying CTB parser error" rather than a bare "stderr contains".
#[then(regex = r#"^stderr matches "([^"]*)" followed by the underlying CTB parser error$"#)]
fn then_stderr_matches_ctb_load_failed(world: &mut VizWorld, needle: String) {
    if world.fixture_skipped {
        return;
    }
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then");
    assert!(
        outcome.stderr_contains(&needle),
        "expected stderr to contain {needle:?} (CTB load failure grep contract); got:\n{}",
        outcome.stderr,
    );
}
