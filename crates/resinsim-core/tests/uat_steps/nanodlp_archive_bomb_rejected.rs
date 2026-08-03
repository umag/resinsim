//! Step definitions for `spec/uat/nanodlp-archive-bomb-rejected.md` UAT-1
//! (uat-unskip-a3-b, plan step 6). SECURITY-RELEVANT: guards the ADR-0021
//! fail-closed PNG dimension-bomb bound (`decode_layer_png`,
//! nanodlp.rs:176-181, checked from the IHDR header *before* any pixel
//! buffer is allocated).
//!
//! Committed `tests/fixtures/bomb-dimensions.nanodlp` (1.3 kB; slicer.json
//! `PWidth`/`PHeight` = 100000; plate `LayersCount` = 1; `1.png` IHDR width
//! and height bytes are `00 01 86 a0` = 100000 each) — no new fixture
//! needed.
//!
//! SYMBOL VERIFICATION. The binary's `sim --file` -> `io::sliced::
//! parse_sliced` -> `io::nanodlp::parse_nanodlp` -> `decode_layer_png`'s
//! IHDR bound and the `pub const MAX_PNG_PIXELS` it reads are all
//! default-features, `#[cfg]`-free.
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory: no
//! other module registers "the command fails with an error naming the
//! pixel-count limit" or "no large pixel buffer is allocated (the header
//! check trips first)"; the When (`` the user invokes `resinsim sim --file
//! <bomb.nanodlp>` ``) is textually distinct from every other `resinsim sim`
//! When in the tree (no `--resin`/`--printer`/`--out` placeholders, a
//! different literal filename placeholder).
//!
//! VACUOUS-ASSERTION RISK, mitigated. A test cannot observe an allocation
//! that did NOT happen; asserting only "the command errors" would pass even
//! if the guard were deleted and the process OOM-killed. The Then below
//! discriminates POSITIVELY (the header-branch message is present) AND
//! NEGATIVELY (none of the four post-allocation branch messages are
//! present), plus a clean-exit-code check and a generous wall-clock bound.
//!
//! FAULT-INJECTION PROOF (mandatory per plan; performed BEFORE writing the
//! assertions below, evidence recorded here rather than asserted-and-hoped).
//! `MAX_PNG_PIXELS` was temporarily raised to `20_000_000_000` in
//! `src/io/nanodlp.rs`, the binary rebuilt, and this fixture re-run:
//!
//! ```text
//! Error producing sim.json from .../bomb-dimensions.nanodlp: NanoDLP layer
//! 1.png: PNG pixel decode failed: IDAT or fDAT chunk does not have enough
//! data for image.
//! ```
//!
//! exit code 1, wall-clock 0.332s (the `vec![0u8; ...]` allocation is served
//! by fresh zero-mapped OS pages rather than an explicit memset, so even a
//! ~10 GB attempted allocation for this fixture's declared 100000×100000
//! dimensions does not manifest as a slow/hung/OOM-killed run — it fails
//! fast at PNG decode instead). This is a DIFFERENT failure mode from the
//! header-check message the Then below requires, and it trips assertion
//! (b)'s negative branch (`PNG pixel decode failed` is one of the four
//! post-allocation strings checked for absence) — proving the positive/
//! negative discrimination actually distinguishes the pre-allocation IHDR
//! check from everything downstream, not merely "the command errors". The
//! const was reverted and the binary rebuilt before this module landed; no
//! production change ships from this issue.
//!
//! The 30s wall-clock bound in the Then below is ADVISORY, relative to the
//! branch-message discrimination above — the message check is what proves
//! the guard fired; the timing check is a cheap secondary signal that a
//! multi-GB decode/allocation attempt did not silently start (per
//! plan-review finding 3).

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::fixture_path;
use super::world::UatWorld;

#[given(regex = r"^a \.nanodlp whose layer PNG declares a 100000×100000 image in its IHDR$")]
fn given_nanodlp_with_dimension_bomb_png(world: &mut UatWorld) {
    world.nanodlp_fixture_path = Some(fixture_path("bomb-dimensions.nanodlp"));
}

#[when(regex = r"^the user invokes `resinsim sim --file <bomb\.nanodlp>`$")]
fn when_user_invokes_resinsim_sim_bomb(world: &mut UatWorld) {
    let fixture = world
        .nanodlp_fixture_path
        .clone()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let data_dir = workspace_data_dir();
    let out_path =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("uat-bomb-out.sim.json");
    let start = std::time::Instant::now();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
            "--out",
            out_path.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_elapsed = Some(start.elapsed());
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

/// The header-branch (pre-allocation) message this fixture must trip —
/// interpolates the production `MAX_PNG_PIXELS` constant rather than a
/// hardcoded 64000000, so a deliberate bound change fails loudly at the
/// const and not as a mystery string mismatch.
fn header_branch_message() -> String {
    format!(
        "PNG layer is 100000×100000 = 10000000000 px, exceeds the {}-px limit \
         (dimension bomb?)",
        resinsim_core::io::nanodlp::MAX_PNG_PIXELS
    )
}

/// The four post-allocation branch message substrings (nanodlp.rs:185, 188,
/// 192, 199) — NONE of these may appear if the header check trips first.
const POST_ALLOCATION_MESSAGES: [&str; 4] = [
    "PNG pixel decode failed",
    "PNG frame has zero pixels",
    "PNG frame has zero-size pixels",
    "layer mask alloc failed",
];

#[then(regex = r"^the command fails with an error naming the pixel-count limit$")]
fn then_command_fails_naming_pixel_count_limit(world: &mut UatWorld) {
    let exit_code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();

    assert_ne!(exit_code, 0, "expected a non-zero exit code; stderr={stderr}");
    // main.rs:1926 `eprintln!("Error producing sim.json from {}: {e}", ...)`.
    assert!(
        stderr.contains("Error producing sim.json from "),
        "stderr must carry cmd_sim's own error wrapper, got: {stderr}"
    );
    assert!(
        stderr.contains(&header_branch_message()),
        "stderr must name the pixel-count limit via the header-check message \
         (MAX_PNG_PIXELS={}), got: {stderr}",
        resinsim_core::io::nanodlp::MAX_PNG_PIXELS
    );
    // nanodlp.rs:314 `.map_err(|e| format!("NanoDLP layer {png_name}: {e}"))`.
    assert!(
        stderr.contains("NanoDLP layer 1.png: "),
        "stderr must carry the per-layer wrapping context, got: {stderr}"
    );
}

#[then(regex = r"^no large pixel buffer is allocated \(the header check trips first\)$")]
fn then_no_large_pixel_buffer_allocated(world: &mut UatWorld) {
    let exit_code = world
        .cli_exit_code
        .expect("scenario invariant: When step populated cli_exit_code");
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let elapsed = world
        .cli_elapsed
        .expect("scenario invariant: When step populated cli_elapsed");

    // (a) POSITIVE: the emitted message is the header-branch message.
    assert!(
        stderr.contains(&header_branch_message()),
        "expected the pre-allocation header-check message, got: {stderr}"
    );

    // (b) NEGATIVE: none of the post-allocation branch messages appear.
    for needle in POST_ALLOCATION_MESSAGES {
        assert!(
            !stderr.contains(needle),
            "stderr must NOT contain the post-allocation branch message {needle:?} \
             (that would mean the header check did not trip first): {stderr}"
        );
    }

    // (c) Clean CLI error exit (cmd_sim's own `std::process::exit(1)`), not
    // a Rust panic (101), SIGABRT from an allocator abort (134), or killed
    // by signal (negative from `CliOutcome`'s `.code().unwrap_or(-1)`).
    assert_eq!(
        exit_code, 1,
        "expected cmd_sim's clean error exit code 1, got {exit_code} (101 = Rust panic, \
         134 = SIGABRT/allocator abort, negative = killed by signal)"
    );
    assert!(
        !stderr.contains("panicked at")
            && !stderr.contains("memory allocation of")
            && !stderr.contains("stack backtrace"),
        "a rejected dimension bomb must be a clean CLI error, not a Rust panic or an \
         allocator abort: {stderr}"
    );

    // (d) Advisory wall-clock bound relative to the message discrimination
    // above (see module doc) — a 10^10-pixel buffer that WAS allocated and
    // decoded would either abort or thrash, neither of which finishes
    // promptly.
    assert!(
        elapsed.as_secs() < 30,
        "expected the header check to reject the bomb promptly, took {elapsed:?}"
    );
}
