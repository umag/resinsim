//! Step definitions for `spec/uat/viz-screenshot-flag.md`.
//!
//! TIER A (this module, step 4 of viz-uat-cucumber-harness): UAT-7a/7b/7c
//! — renderer-free, `--screenshot` path validation exits 5 via
//! `std::process::exit` at main.rs BEFORE `App::new()`. No window, no
//! wgpu, no LogPlugin; these run on any machine, display or not.
//!
//! UAT-7d ("empty path → exit 5") is declared debt, NOT stepped, despite
//! being read as tier-A-shaped by the plan. Empirically probed at
//! viz-uat-cucumber-harness step 1 via a direct subprocess invocation
//! (bypassing shell-quoting ambiguity) AND an isolated minimal clap 4.6.1
//! repro: passing an empty string to `--screenshot` (either
//! `--screenshot ""` or the unambiguous `--screenshot=` form) is rejected
//! by clap's OWN parser — exit 2, "a value is required for '--screenshot
//! <PATH.png>' but none was supplied" — before `main()`'s own
//! `validate_screenshot_path` / `PathError::Empty` (screenshot.rs:124)
//! ever runs. The scenario as literally written (exit 5 + "is empty") is
//! therefore NOT reachable via CLI subprocess. Per the hard rule that
//! every spec/harness mismatch is resolved on the HARNESS side, never by
//! editing the spec or weakening the assertion, UAT-7d stays an actual
//! runtime skip, covered instead by the pre-existing in-process unit test
//! `validate_rejects_empty_path` (screenshot.rs) — the same pattern the
//! spec's own "Test coverage notes" section already uses for exit 7/8.
//!
//! TIER B (UAT-1/3/4/9) is added by step 5, in this same module.
//!
//! Scenario-scoped paths come from `VizWorld::tempdir`, never a
//! hardcoded `/tmp`, even though the spec's own prose names literal
//! `/tmp/...` paths — using the real literal path would still be
//! CORRECT for what UAT-7a/7b/7c assert (IsDirectory / ParentMissing /
//! BadExtension are all path-shape checks, not filesystem-location
//! checks), but a scenario-scoped tempdir means parallel runs (and
//! repeat runs leaving stale state) cannot collide or leave litter.

use cucumber::{given, then, when};

use crate::VizWorld;
use crate::uat_viz_steps::viz_cli::invoke_viz;

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
// Shared Then/And assertions (UAT-7a/7b/7c today; UAT-1/3/4/9 reuse the
// same exit-code step in step 5).
// ---------------------------------------------------------------------

#[then(regex = r#"^the process exits with code (\d+)$"#)]
fn then_process_exits_with_code(world: &mut VizWorld, code: u8) {
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
// (UAT-7c) and `And stderr contains "..."` (UAT-7a/7b) lines — an `And`
// following a `Then` resolves to StepType::Then (same mechanism as
// above), so cucumber matches both against this single `#[then]`.
#[then(regex = r#"^stderr contains "([^"]*)"$"#)]
fn then_stderr_contains(world: &mut VizWorld, needle: String) {
    let outcome = world
        .last
        .as_ref()
        .expect("scenario invariant: a When step must populate world.last before Then/And");
    assert!(
        outcome.stderr_contains(&needle),
        "expected stderr to contain {needle:?}; got:\n{}",
        outcome.stderr,
    );
}
