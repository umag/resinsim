//! `--screenshot PATH.png` capture-and-exit pipeline + Capture-screenshot
//! button (issue 12).
//!
//! Closes the AI visual-feedback gap (KB-3 anti-pattern: review matrix
//! tick-the-box failure on issue 03's missing emissive cursor). The agent
//! makes a visual change → runs resinsim-viz --screenshot /tmp/x.png →
//! Read tool reads the PNG (Claude Code is multimodal) → agent sees what
//! the user sees.
//!
//! Three phases (see [`capture_inner`]):
//!   - **Phase 1: Settle.** Wait for --load-* loads to settle (tracked
//!     via [`loads_settled`]); then 2 extra frames for PBR / transparency
//!     sort. Falls back to "capture anyway + warn" after MAX_WAIT_FRAMES.
//!   - **Phase 2: Render.** Spawned `Screenshot` entity; wait for Bevy
//!     to emit `Captured`. Times out after MAX_RENDER_FRAMES → exit 8.
//!   - **Phase 3: Persist.** `Captured` observed; wait for the file to
//!     land on disk. Times out after MAX_FILE_WAIT_FRAMES → exit 7.
//!
//! `captured_observed` is sticky — Bevy's `clear_screenshots` runs in
//! `First` (BEFORE Update) so the live `Captured` query goes empty one
//! frame after observation. Without the sticky latch, Phase 3 regresses
//! into Phase 2 and mis-fires render-timeout on a successful capture.
//!
//! `has_exited` is an idempotency guard so terminal decisions
//! (ExitSuccess / ExitWriteFailed / ExitTimeoutRenderHung) don't re-fire
//! on extra frames before Bevy honours AppExit.
//!
//! ADR-0010 boundary: this module imports only Bevy, std, and the data
//! resources from main.rs (Args, LoadedSimulation, LoadedSliceStack,
//! LoadedStlMesh). No `use resinsim_core` lines.

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Captured, Screenshot};

use crate::mesh::LoadedStlMesh;
use crate::slice::LoadedSliceStack;
use crate::{
    fatal_exit, Args, LoadedSimulation, EXIT_SCREENSHOT_RENDER_TIMEOUT,
    EXIT_SCREENSHOT_WRITE_FAILED,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Frames to wait after `loads_settled` returns true, so PBR shaders +
/// transparency sort settle before capture. 2 frames @ 60 Hz = ~33 ms.
pub const SETTLE_FRAMES_AFTER_READY: u32 = 2;

/// Phase 1 cap: max frames to wait for --load-* loads to settle before
/// "capturing anyway + warn". 600 @ 60 Hz = 10 s.
pub const MAX_WAIT_FRAMES: u32 = 600;

/// Phase 2 cap: max frames to wait for Bevy's `Captured` marker after
/// spawning the `Screenshot` entity. 600 @ 60 Hz = 10 s. Exceeding this
/// fires EXIT_SCREENSHOT_RENDER_TIMEOUT (code 8). Override on slow CI:
/// see README "Failure modes".
pub const MAX_RENDER_FRAMES: u32 = 600;

/// Phase 3 cap: max frames to wait for the PNG file to appear on disk
/// after `Captured` is observed. 60 @ 60 Hz = 1 s. Exceeding this fires
/// EXIT_SCREENSHOT_WRITE_FAILED (code 7).
pub const MAX_FILE_WAIT_FRAMES: u32 = 60;

// ---------------------------------------------------------------------------
// Marker components
// ---------------------------------------------------------------------------

/// Marker for screenshot entities spawned by the auto-capture (CLI)
/// path. Distinguishes them from button-click captures so the
/// system-shim's `auto_captured` query only triggers on the CLI path
/// (button-click captures share Bevy's pipeline but must NOT trigger
/// AppExit).
#[derive(Component)]
pub struct AutoCaptureMarker;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Records the most-recent button-click screenshot for the in-panel
/// "Captured: <basename>" toast. The toast is a verb-independent
/// signal — UI tests assert on `LastScreenshot.0.is_some()` to
/// avoid coupling to the toast-rendering string (round-4 code LOW
/// + code-r5 LOW closure).
#[derive(Resource, Default)]
pub struct LastScreenshot(pub Option<(PathBuf, std::time::Instant)>);

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Reasons --screenshot can fail validation before App::new(). Each
/// variant maps to a 3-line user-facing message via [`format_path_error`]
/// and exits with EXIT_SCREENSHOT_BAD_PATH (5). Kept narrow on purpose:
/// every variant is observable on Unix without test-harness gymnastics
/// (CwdUnavailable is the exception — covered by unit tests only).
#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    /// `--screenshot ""` (empty string).
    Empty,
    /// Path resolved to an existing directory.
    IsDirectory,
    /// Path's parent directory does not exist. --screenshot does NOT
    /// create parents (unlike --save-sim, which does — see step 5
    /// rationale).
    ParentMissing { parent: PathBuf },
    /// File extension is not `.png`, `.jpg`, or `.jpeg`.
    BadExtension { actual: String },
    /// Failed to determine the current working directory while
    /// canonicalising a relative path. Hard to trigger from a shell;
    /// unit-tested only.
    CwdUnavailable,
}

/// Validate the --screenshot path. On Ok returns the absolute, canonical
/// PathBuf the system will write to. On Err returns a [`PathError`] which
/// the caller renders via [`format_path_error`] to stderr and exits with
/// EXIT_SCREENSHOT_BAD_PATH.
pub fn validate_screenshot_path(input: &Path) -> Result<PathBuf, PathError> {
    if input.as_os_str().is_empty() {
        return Err(PathError::Empty);
    }

    // Extension check first — cheap, no syscalls, catches the common
    // typo (".pgn") before we touch the filesystem.
    match input.extension().and_then(|e| e.to_str()) {
        Some(ext) if matches!(ext, "png" | "jpg" | "jpeg") => {}
        Some(other) => {
            return Err(PathError::BadExtension {
                actual: other.to_string(),
            });
        }
        None => {
            return Err(PathError::BadExtension {
                actual: String::new(),
            });
        }
    }

    // Resolve to absolute. std::path::absolute exists on stable
    // (1.79+); if the path is already absolute it's a clone. CWD lookup
    // can fail in weird sandboxes — surface as CwdUnavailable.
    let abs = if input.is_absolute() {
        input.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(input),
            Err(_) => return Err(PathError::CwdUnavailable),
        }
    };

    // Existing directory at the path → can't write a PNG over it.
    if abs.is_dir() {
        return Err(PathError::IsDirectory);
    }

    // Parent dir must exist. We deliberately don't create it — the
    // user/agent passes a deliberate path; "silent mkdir -p" surprises
    // the caller. Contrast with --save-sim, which creates parents
    // because it's best-effort persistence.
    match abs.parent() {
        Some(parent) if parent.as_os_str().is_empty() => {
            // Bare filename like "shot.png"; CWD is the parent which
            // exists by construction (current_dir succeeded above).
        }
        Some(parent) if !parent.exists() => {
            return Err(PathError::ParentMissing {
                parent: parent.to_path_buf(),
            });
        }
        _ => {}
    }

    Ok(abs)
}

/// Render a [`PathError`] as a 3-line message — what failed, why, what
/// to do. Lines are separated by `\n  ` so a logger that prefixes lines
/// preserves the indentation. Per ux-r5 finding: every variant must
/// answer WHAT/WHY/WHAT-TO-DO.
pub fn format_path_error(input: &Path, err: &PathError) -> String {
    match err {
        PathError::Empty => format!(
            "--screenshot path is empty\n  \
             what: passed an empty string\n  \
             fix: pass a path like `--screenshot /tmp/shot.png`"
        ),
        PathError::IsDirectory => format!(
            "--screenshot path '{}' is a directory\n  \
             what: cannot write a PNG over an existing directory\n  \
             fix: include a filename, e.g. `{}/shot.png`",
            input.display(),
            input.display()
        ),
        PathError::ParentMissing { parent } => format!(
            "--screenshot parent dir '{}' does not exist\n  \
             what: --screenshot does NOT create parent directories \
             (unlike --save-sim)\n  \
             fix: `mkdir -p {}` first, then re-run",
            parent.display(),
            parent.display()
        ),
        PathError::BadExtension { actual } if actual.is_empty() => format!(
            "--screenshot path '{}' has no extension\n  \
             what: only .png/.jpg/.jpeg are supported\n  \
             fix: append .png to the filename, e.g. `{}.png`",
            input.display(),
            input.display()
        ),
        PathError::BadExtension { actual } => format!(
            "--screenshot path '{}' has unsupported extension '.{}'\n  \
             what: only .png/.jpg/.jpeg are supported\n  \
             fix: change the extension to .png, .jpg, or .jpeg",
            input.display(),
            actual
        ),
        PathError::CwdUnavailable => format!(
            "could not determine current working directory while \
             resolving --screenshot path '{}'\n  \
             what: a relative --screenshot path needs CWD; the lookup \
             failed (sandboxed environment?)\n  \
             fix: pass an absolute path, e.g. `--screenshot /tmp/shot.png`",
            input.display()
        ),
    }
}

// ---------------------------------------------------------------------------
// Three-phase capture decision (pure fn — testable seam)
// ---------------------------------------------------------------------------

/// What the capture system should do this frame. Returned by
/// [`capture_inner`] and dispatched by `capture_screenshot_and_exit`.
#[derive(Debug, PartialEq)]
pub enum CaptureDecision {
    /// Do nothing this frame; the system shim may emit a
    /// "scheduled" log on first invocation only.
    Skip,
    /// Spawn a `Screenshot` entity for the given path. Phase 1
    /// transition.
    SpawnScreenshot(PathBuf),
    /// Bevy emitted Captured AND the file landed on disk.
    /// Phase 3 → exit 0 via `AppExit::Success`.
    ExitSuccess,
    /// Phase 1 timeout: --load-* surfaces never settled within
    /// MAX_WAIT_FRAMES. Capture proceeds anyway (the warn message
    /// names the still-pending loads). The system shim spawns a
    /// screenshot on this decision and continues into Phase 2/3.
    ExitTimeoutLoadsPending {
        load_ctb: bool,
        load_stl: bool,
        load_sim: bool,
    },
    /// Phase 2 timeout: spawned a Screenshot but Bevy never produced
    /// Captured within MAX_RENDER_FRAMES. → exit 8.
    ExitTimeoutRenderHung,
    /// Phase 3 timeout: Captured fired but the file didn't land within
    /// MAX_FILE_WAIT_FRAMES. → exit 7.
    ExitWriteFailed(PathBuf),
}

/// Pure decision function for the --screenshot capture pipeline.
/// Owns no state — all per-frame counters and latches are passed by
/// `&mut` from the system shim's `Local<T>` params.
///
/// Phase ordering:
///   1. Idempotency guard: if a terminal decision already fired this
///      run, return Skip.
///   2. Sticky `captured_observed` latch: once Bevy ever shows the
///      Captured marker, latch it true so Phase 3 stays valid even
///      after `clear_screenshots` auto-despawns the entity in `First`.
///   3. Phase 3 (post-Captured): wait for the file to appear; bail
///      after MAX_FILE_WAIT_FRAMES with ExitWriteFailed.
///   4. Phase 2 (post-spawn, pre-Captured): wait for Bevy to produce
///      Captured; bail after MAX_RENDER_FRAMES with
///      ExitTimeoutRenderHung. Defensive: if the file landed before
///      we observed Captured (theoretical race window), accept it.
///   5. Phase 1 (pre-spawn): wait for loads to settle, then 2 settle
///      frames, then SpawnScreenshot. Phase 1 timeout (`frame_count >
///      MAX_WAIT_FRAMES`) returns ExitTimeoutLoadsPending which the
///      system shim turns into "spawn anyway + warn".
#[allow(clippy::too_many_arguments)]
pub fn capture_inner(
    args: &Args,
    ready: bool,
    bevy_captured_marker_present: bool,
    file_present_on_disk: bool,
    ctb_pending: bool,
    stl_pending: bool,
    sim_pending: bool,
    frame_count: &mut u32,
    frames_since_ready: &mut u32,
    frames_since_spawn: &mut u32,
    frames_since_captured: &mut u32,
    spawn_fired: &mut bool,
    previously_ready: &mut bool,
    captured_observed: &mut bool,
    has_exited: &mut bool,
) -> CaptureDecision {
    // (1) Idempotency guard — terminal decisions never re-fire even
    // if Bevy hasn't honoured AppExit yet (one extra frame may run).
    if *has_exited {
        return CaptureDecision::Skip;
    }

    let Some(path) = args.screenshot.as_ref() else {
        return CaptureDecision::Skip;
    };
    *frame_count += 1;

    // (2) Latch captured_observed: once Bevy ever emits Captured for
    // our auto-marker, keep the flag true. Bevy's clear_screenshots
    // runs in First (BEFORE Update) so the live query goes empty next
    // frame even though we did observe it. Without this latch Phase 3
    // regresses into Phase 2 and mis-fires render-timeout.
    if *spawn_fired && bevy_captured_marker_present {
        *captured_observed = true;
    }

    // (3) Phase 3: post-Captured (sticky once observed).
    if *spawn_fired && *captured_observed {
        *frames_since_captured += 1;
        if file_present_on_disk {
            *has_exited = true;
            return CaptureDecision::ExitSuccess;
        }
        if *frames_since_captured > MAX_FILE_WAIT_FRAMES {
            *has_exited = true;
            return CaptureDecision::ExitWriteFailed(path.clone());
        }
        return CaptureDecision::Skip;
    }

    // (4) Phase 2: spawned but Bevy hasn't completed readback yet.
    if *spawn_fired {
        *frames_since_spawn += 1;
        // Defensive: if the file landed before we observed Captured
        // (theoretical race window — save_to_disk's observer + the
        // Captured insertion happen on adjacent ticks), accept it.
        if file_present_on_disk {
            *has_exited = true;
            return CaptureDecision::ExitSuccess;
        }
        if *frames_since_spawn > MAX_RENDER_FRAMES {
            *has_exited = true;
            return CaptureDecision::ExitTimeoutRenderHung;
        }
        return CaptureDecision::Skip;
    }

    // Ready oscillation reset (drag-drop case): if loads were ready
    // and then unloaded, restart the settle counter so we wait for the
    // new loads to fully settle before capturing.
    if !ready && *previously_ready && *frames_since_ready > 0 {
        *frames_since_ready = 0;
    }
    *previously_ready = ready;

    // (5) Phase 1: settle + readiness.
    if ready {
        *frames_since_ready += 1;
    } else if *frame_count > MAX_WAIT_FRAMES {
        // Force-spawn after the Phase 1 cap: capture whatever is on
        // screen (default clear color + plate, probably) and warn.
        // The system shim spawns the auto-screenshot on this decision
        // and continues into Phase 2/3. We do NOT set has_exited —
        // capture is in flight, just on incomplete data.
        *spawn_fired = true;
        return CaptureDecision::ExitTimeoutLoadsPending {
            load_ctb: ctb_pending,
            load_stl: stl_pending,
            load_sim: sim_pending,
        };
    } else {
        return CaptureDecision::Skip;
    }

    if *frames_since_ready < SETTLE_FRAMES_AFTER_READY {
        return CaptureDecision::Skip;
    }

    *spawn_fired = true;
    CaptureDecision::SpawnScreenshot(path.clone())
}

// ---------------------------------------------------------------------------
// Readiness predicate
// ---------------------------------------------------------------------------

/// True when every requested --load-* surface has either successfully
/// produced its world resource OR (for sim only) attempted and failed
/// (the failure is a settled state — capture proceeds without the
/// heatmap). When no loads are requested, returns true immediately so
/// `--screenshot` alone captures whatever the default scene shows.
pub fn loads_settled(
    args: &Args,
    slice_present: bool,
    stl_present: bool,
    sim_loaded: bool,
    sim_attempted_and_failed: bool,
) -> bool {
    let ctb_settled = args.load_ctb.is_none() || slice_present;
    let stl_settled = args.load_stl.is_none() || stl_present;
    let sim_settled = args.load_sim.is_none() || sim_loaded || sim_attempted_and_failed;
    ctb_settled && stl_settled && sim_settled
}

// ---------------------------------------------------------------------------
// Spawn helpers + default path
// ---------------------------------------------------------------------------

/// Spawn a `Screenshot` entity for the button-click path. NO
/// AutoCaptureMarker — the system shim's auto_captured query
/// excludes button captures so AppExit isn't fired on click.
pub fn spawn_button_screenshot(commands: &mut Commands, path: &Path) {
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.to_path_buf()));
}

/// Spawn a `Screenshot` entity for the auto-capture (CLI) path.
/// Carries `AutoCaptureMarker` so the auto_captured query in the
/// system shim sees this capture (and only this capture) when
/// Bevy emits Captured.
pub fn spawn_auto_screenshot(commands: &mut Commands, path: &Path) {
    commands
        .spawn((Screenshot::primary_window(), AutoCaptureMarker))
        .observe(save_to_disk(path.to_path_buf()));
}

/// Generate a default path for the button-click capture, of the form
/// `<CWD>/resinsim-viz-<unix-secs>.png`. Falls back to `<TMPDIR>/...`
/// if CWD lookup fails (sandboxed env). Stable filename pattern is
/// the UAT-6 contract (issue 12).
pub fn default_screenshot_path() -> PathBuf {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("resinsim-viz-{secs}.png");
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(filename),
        Err(_) => std::env::temp_dir().join(filename),
    }
}

// ---------------------------------------------------------------------------
// System shim
// ---------------------------------------------------------------------------

/// Bundles the four world-state queries the capture system reads each
/// frame. Folding them into one `SystemParam` keeps the
/// `capture_screenshot_and_exit` signature legible (4 queries → 1
/// param). Per docs/patterns/system-param-bundle-for-16-param-limit.md.
#[derive(SystemParam)]
pub struct LoadStateParams<'w, 's> {
    pub slice: Query<'w, 's, (), With<LoadedSliceStack>>,
    pub stl: Query<'w, 's, (), With<LoadedStlMesh>>,
    pub sim: Res<'w, LoadedSimulation>,
    /// Captured + AutoCaptureMarker — distinguishes the auto-capture
    /// from a concurrent button-click capture (which has Captured
    /// but no AutoCaptureMarker, so AppExit doesn't fire on click).
    pub auto_captured: Query<'w, 's, (), (With<Captured>, With<AutoCaptureMarker>)>,
}

/// `--screenshot` system shim. Runs every Update tick when --screenshot
/// is set (registered in main.rs step 9). Routes capture_inner's
/// CaptureDecision to Bevy commands + log output + AppExit.
///
/// Phase ordering matches capture_inner (see that fn's docs).
/// The 12-arg signature is over clippy's default 7-cap; allow attribute
/// matches the precedent on capture_inner (and 5 other systems in
/// main.rs at lines 466/537/756/863/1130).
#[allow(clippy::too_many_arguments)]
pub fn capture_screenshot_and_exit(
    args: Res<Args>,
    loads: LoadStateParams,
    mut commands: Commands,
    mut writer: MessageWriter<AppExit>,
    mut frame_count: Local<u32>,
    mut frames_since_ready: Local<u32>,
    mut frames_since_spawn: Local<u32>,
    mut frames_since_captured: Local<u32>,
    mut spawn_fired: Local<bool>,
    mut previously_ready: Local<bool>,
    mut captured_observed: Local<bool>,
    mut has_exited: Local<bool>,
) {
    let was_spawned = *spawn_fired;

    // SAFETY: fs::metadata is gated on *spawn_fired so we make zero
    // syscalls during Phase 1 readiness wait (which can be 600
    // frames). Enforced by code structure, not a test (round-4
    // code M2: a metadata-spy mock is impractical without injecting
    // a fs trait, which would muddy the production code).
    let file_present = if was_spawned {
        args.screenshot
            .as_ref()
            .map(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
            .unwrap_or(false)
    } else {
        false
    };

    let sim_loaded = loads.sim.simulation.is_some();
    let sim_failed = loads.sim.last_attempt.as_ref().is_some_and(|r| r.is_err());
    let slice_present = !loads.slice.is_empty();
    let stl_present = !loads.stl.is_empty();
    let ready = loads_settled(&args, slice_present, stl_present, sim_loaded, sim_failed);
    let bevy_captured = !loads.auto_captured.is_empty();

    let ctb_pending = args.load_ctb.is_some() && !slice_present;
    let stl_pending = args.load_stl.is_some() && !stl_present;
    let sim_pending = args.load_sim.is_some() && !sim_loaded && !sim_failed;

    // Locals are SystemParam wrappers; deref to the inner T to get
    // the &mut required by capture_inner.
    let decision = capture_inner(
        &args,
        ready,
        bevy_captured,
        file_present,
        ctb_pending,
        stl_pending,
        sim_pending,
        &mut *frame_count,
        &mut *frames_since_ready,
        &mut *frames_since_spawn,
        &mut *frames_since_captured,
        &mut *spawn_fired,
        &mut *previously_ready,
        &mut *captured_observed,
        &mut *has_exited,
    );

    match decision {
        CaptureDecision::Skip => {
            if *frame_count == 1 {
                info!("--screenshot scheduled (waiting for loads_settled)");
            }
        }
        CaptureDecision::SpawnScreenshot(path) => {
            spawn_auto_screenshot(&mut commands, &path);
        }
        CaptureDecision::ExitTimeoutLoadsPending {
            load_ctb,
            load_stl,
            load_sim,
        } => {
            let pending: Vec<&str> = [
                ("--load-ctb", load_ctb),
                ("--load-stl", load_stl),
                ("--load-sim", load_sim),
            ]
            .iter()
            .filter_map(|(name, p)| p.then_some(*name))
            .collect();
            // Defensive empty render (round-4 ux L1): if a future
            // contributor adds a 4th load type and forgets to thread
            // it into pending, surface the inconsistency rather than
            // emit "still waiting on: ;".
            let pending_str = if pending.is_empty() {
                "(unknown — all loads settled but ready=false)".to_string()
            } else {
                pending.join(", ")
            };
            warn!(
                "--screenshot exceeded MAX_WAIT_FRAMES={} (10 s); \
                 still waiting on: {}; capturing anyway. If this is a \
                 large CTB on a slow GPU, the capture may show partial \
                 mesh; capture without --load-ctb to confirm.",
                MAX_WAIT_FRAMES, pending_str,
            );
            let path = args
                .screenshot
                .as_ref()
                .expect("Phase 1 timeout only fires when --screenshot is set");
            spawn_auto_screenshot(&mut commands, path);
        }
        CaptureDecision::ExitTimeoutRenderHung => {
            error!(
                "--screenshot exceeded MAX_RENDER_FRAMES={} (10 s) \
                 waiting for Bevy to complete the GPU readback. \
                 Likely causes: headless build, GPU hang, render \
                 thread deadlock, or a heavily-loaded software \
                 rasterizer (CI). No PNG produced.",
                MAX_RENDER_FRAMES,
            );
            fatal_exit(&mut writer, EXIT_SCREENSHOT_RENDER_TIMEOUT);
        }
        CaptureDecision::ExitSuccess => {
            writer.write(AppExit::Success);
        }
        CaptureDecision::ExitWriteFailed(path) => {
            error!(
                "--screenshot Captured marker fired but no file at {} \
                 after {} frames — Bevy's save_to_disk likely failed \
                 mid-write. Check stderr above for the specific Bevy \
                 error. Common causes: parent directory was deleted \
                 after validation, disk full, write permission \
                 revoked. Re-run after resolving the underlying \
                 filesystem issue.",
                path.display(),
                MAX_FILE_WAIT_FRAMES,
            );
            fatal_exit(&mut writer, EXIT_SCREENSHOT_WRITE_FAILED);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with_loads(ctb: bool, stl: bool, sim: bool) -> Args {
        // Args { all default to None } with the specified --load-* paths
        // toggled. Path values are arbitrary placeholders — loads_settled
        // only inspects is_none(), not the path content.
        Args {
            smoke_exit: false,
            load_stl: stl.then(|| PathBuf::from("a.stl")),
            load_ctb: ctb.then(|| PathBuf::from("a.ctb")),
            data_dir: None,
            resin: None,
            printer: None,
            initial_led_temp: None,
            save_sim: None,
            load_sim: sim.then(|| PathBuf::from("a.sim.json")),
            allow_mismatch: false,
            screenshot: None,
            v2: false,
        }
    }

    // ---- loads_settled truth table (9 rows) ----

    #[test]
    fn loads_settled_no_loads_requested_is_immediately_true() {
        let args = args_with_loads(false, false, false);
        assert!(loads_settled(&args, false, false, false, false));
    }

    #[test]
    fn loads_settled_ctb_requested_and_present_is_true() {
        let args = args_with_loads(true, false, false);
        assert!(loads_settled(&args, true, false, false, false));
    }

    #[test]
    fn loads_settled_ctb_requested_but_absent_is_false() {
        let args = args_with_loads(true, false, false);
        assert!(!loads_settled(&args, false, false, false, false));
    }

    #[test]
    fn loads_settled_stl_requested_and_present_is_true() {
        let args = args_with_loads(false, true, false);
        assert!(loads_settled(&args, false, true, false, false));
    }

    #[test]
    fn loads_settled_stl_requested_but_absent_is_false() {
        let args = args_with_loads(false, true, false);
        assert!(!loads_settled(&args, false, false, false, false));
    }

    #[test]
    fn loads_settled_sim_requested_and_loaded_is_true() {
        let args = args_with_loads(false, false, true);
        assert!(loads_settled(&args, false, false, true, false));
    }

    #[test]
    fn loads_settled_sim_requested_and_failed_is_true_settled_failure() {
        // Per issue 12 contract: a failed --load-sim is a SETTLED state
        // (we know it failed; not "still waiting"). Capture proceeds
        // without the heatmap.
        let args = args_with_loads(false, false, true);
        assert!(loads_settled(&args, false, false, false, true));
    }

    #[test]
    fn loads_settled_sim_requested_neither_loaded_nor_failed_is_false() {
        let args = args_with_loads(false, false, true);
        assert!(!loads_settled(&args, false, false, false, false));
    }

    #[test]
    fn loads_settled_all_three_requested_one_pending_is_false() {
        let args = args_with_loads(true, false, true);
        // CTB present, sim still loading → not ready.
        assert!(!loads_settled(&args, true, false, false, false));
    }

    // ---- capture_inner three-phase decision tests ----
    //
    // All tests build a "scratch" set of mutable state cells and an
    // Args with --screenshot set, then drive capture_inner with
    // controlled inputs across one or more frames. The pure-fn shape
    // means no Bevy App is needed.

    /// Per-frame mutable state cells for capture_inner. One instance
    /// per test = one independent capture run.
    #[derive(Default)]
    struct State {
        frame_count: u32,
        frames_since_ready: u32,
        frames_since_spawn: u32,
        frames_since_captured: u32,
        spawn_fired: bool,
        previously_ready: bool,
        captured_observed: bool,
        has_exited: bool,
    }

    fn args_with_screenshot(path: &str) -> Args {
        let mut a = args_with_loads(false, false, false);
        a.screenshot = Some(PathBuf::from(path));
        a
    }

    fn drive(
        s: &mut State,
        args: &Args,
        ready: bool,
        captured: bool,
        file: bool,
    ) -> CaptureDecision {
        capture_inner(
            args,
            ready,
            captured,
            file,
            args.load_ctb.is_some(),
            args.load_stl.is_some(),
            args.load_sim.is_some(),
            &mut s.frame_count,
            &mut s.frames_since_ready,
            &mut s.frames_since_spawn,
            &mut s.frames_since_captured,
            &mut s.spawn_fired,
            &mut s.previously_ready,
            &mut s.captured_observed,
            &mut s.has_exited,
        )
    }

    #[test]
    fn capture_inner_skip_when_screenshot_none() {
        let mut s = State::default();
        let args = args_with_loads(false, false, false); // no --screenshot
        assert_eq!(
            drive(&mut s, &args, true, false, false),
            CaptureDecision::Skip
        );
    }

    #[test]
    fn capture_inner_skip_below_settle_threshold() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        // Frame 1: ready=true, frames_since_ready becomes 1, below
        // SETTLE_FRAMES_AFTER_READY (=2) → Skip.
        assert_eq!(
            drive(&mut s, &args, true, false, false),
            CaptureDecision::Skip
        );
        assert_eq!(s.frames_since_ready, 1);
    }

    #[test]
    fn capture_inner_spawn_after_ready_and_settle() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        let _ = drive(&mut s, &args, true, false, false); // frame 1 → 1
        let d = drive(&mut s, &args, true, false, false); // frame 2 → 2 ≥ SETTLE
        assert!(matches!(d, CaptureDecision::SpawnScreenshot(_)));
        assert!(s.spawn_fired);
    }

    #[test]
    fn capture_inner_skip_after_spawn_during_render_window() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        // Drive into Phase 2: spawn_fired=true, no Captured yet.
        s.spawn_fired = true;
        for _ in 0..5 {
            assert_eq!(
                drive(&mut s, &args, true, false, false),
                CaptureDecision::Skip
            );
        }
        assert_eq!(s.frames_since_spawn, 5);
    }

    #[test]
    fn capture_inner_exit_success_after_captured_and_file_present() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        // Frame N: Captured fires, file already on disk.
        let d = drive(&mut s, &args, true, true, true);
        assert_eq!(d, CaptureDecision::ExitSuccess);
        assert!(s.has_exited);
    }

    #[test]
    fn capture_inner_exit_write_failed_after_captured_no_file() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        // Call 1: Captured observed; frames_since_captured = 1. Skip.
        let _ = drive(&mut s, &args, true, true, false);
        // Calls 2..=MAX_FILE_WAIT_FRAMES advance the counter to
        // MAX_FILE_WAIT_FRAMES; still ≤ cap → Skip.
        for _ in 1..MAX_FILE_WAIT_FRAMES {
            let _ = drive(&mut s, &args, true, false, false);
        }
        // Next call: frames_since_captured becomes MAX_FILE_WAIT_FRAMES + 1
        // → exceeds cap → ExitWriteFailed.
        let d = drive(&mut s, &args, true, false, false);
        assert!(matches!(d, CaptureDecision::ExitWriteFailed(_)));
        assert!(s.has_exited);
    }

    #[test]
    fn capture_inner_exit_render_hung_when_bevy_never_captures() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        // Captured never fires; file never lands. Phase 2 ticks until
        // frames_since_spawn exceeds MAX_RENDER_FRAMES.
        for _ in 0..MAX_RENDER_FRAMES {
            let _ = drive(&mut s, &args, true, false, false);
        }
        // Next call: frames_since_spawn becomes MAX_RENDER_FRAMES + 1
        // → exceeds cap → ExitTimeoutRenderHung.
        let d = drive(&mut s, &args, true, false, false);
        assert_eq!(d, CaptureDecision::ExitTimeoutRenderHung);
        assert!(s.has_exited);
    }

    #[test]
    fn capture_inner_skip_during_file_wait_window() {
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        let _ = drive(&mut s, &args, true, true, false); // Captured, no file yet
                                                         // Subsequent frames within file-wait window: Skip.
        for _ in 0..(MAX_FILE_WAIT_FRAMES - 5) {
            assert_eq!(
                drive(&mut s, &args, true, false, false),
                CaptureDecision::Skip
            );
        }
    }

    #[test]
    fn capture_inner_timeout_loads_pending_with_named_flags() {
        let mut s = State::default();
        let mut args = args_with_screenshot("/tmp/x.png");
        args.load_ctb = Some(PathBuf::from("a.ctb"));
        args.load_stl = Some(PathBuf::from("b.stl"));
        // Drive ready=false for MAX_WAIT_FRAMES + 1 frames.
        for _ in 0..MAX_WAIT_FRAMES {
            let _ = drive(&mut s, &args, false, false, false);
        }
        let d = drive(&mut s, &args, false, false, false);
        match d {
            CaptureDecision::ExitTimeoutLoadsPending {
                load_ctb,
                load_stl,
                load_sim,
            } => {
                assert!(load_ctb);
                assert!(load_stl);
                assert!(!load_sim);
            }
            other => panic!("expected ExitTimeoutLoadsPending, got {other:?}"),
        }
        assert!(s.spawn_fired, "Phase 1 timeout must flip spawn_fired");
        assert!(!s.has_exited, "Phase 1 timeout must NOT set has_exited");
    }

    #[test]
    fn capture_inner_idempotent_after_exit() {
        // Tests the IMMEDIATE next-call Skip after a terminal decision.
        // Distinguished from terminal_decisions_never_refire which
        // drives many frames and asserts the latch persists.
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        let _ = drive(&mut s, &args, true, true, true); // ExitSuccess
                                                        // Immediate next call: Skip via has_exited guard.
        assert_eq!(
            drive(&mut s, &args, true, true, true),
            CaptureDecision::Skip
        );
    }

    #[test]
    fn capture_inner_resets_settle_on_ready_oscillation() {
        // Drag-drop case: loads were ready, then unloaded, then
        // ready again. settle counter must reset on the dip so we
        // wait for the new loads to fully settle.
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        let _ = drive(&mut s, &args, true, false, false); // frame 1 ready: counter=1
        assert_eq!(s.frames_since_ready, 1);
        let _ = drive(&mut s, &args, false, false, false); // frame 2 NOT ready
        assert_eq!(
            s.frames_since_ready, 0,
            "oscillation must reset settle counter"
        );
    }

    #[test]
    fn capture_inner_phase_3_sticky_after_auto_despawn() {
        // Round-4 code HIGH regression guard (v6 NEW): drive
        // Captured=true frame N, Captured=false frame N+1,
        // file_present=true frame N+2 → ExitSuccess (NOT
        // ExitTimeoutRenderHung). Without the sticky latch the
        // second frame regresses into Phase 2.
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        // Frame N: Captured observed, file not yet present.
        let d1 = drive(&mut s, &args, true, true, false);
        assert_eq!(d1, CaptureDecision::Skip);
        assert!(s.captured_observed);
        // Frame N+1: Captured query empty (clear_screenshots ran),
        // file not yet present. Sticky latch keeps Phase 3 active.
        let d2 = drive(&mut s, &args, true, false, false);
        assert_eq!(d2, CaptureDecision::Skip);
        assert!(s.captured_observed);
        // Frame N+2: file lands.
        let d3 = drive(&mut s, &args, true, false, true);
        assert_eq!(d3, CaptureDecision::ExitSuccess);
    }

    #[test]
    fn capture_inner_phase_2_accepts_file_landed_before_captured() {
        // Defensive race fix (v6 NEW): file_present=true while
        // spawn_fired and captured_observed=false → ExitSuccess.
        // Theoretical window where save_to_disk's observer wrote
        // the file before our system observed Captured.
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        let d = drive(&mut s, &args, true, false, true);
        assert_eq!(d, CaptureDecision::ExitSuccess);
        assert!(!s.captured_observed, "we never observed Captured");
        assert!(s.has_exited);
    }

    // ---- spawn helpers: marker-disambiguation regression guards ----
    //
    // UAT harvest from issue 12 (manual Round B was deferred — these
    // close the gap without bevy_egui spike work).

    #[test]
    fn spawn_button_screenshot_spawns_one_entity_without_auto_marker() {
        // Round-2 plan-review HIGH (button-click captures must NOT
        // trigger AppExit) hinges on AutoCaptureMarker being absent
        // from button-spawned entities. A silent regression (someone
        // adds the marker to spawn_button_screenshot for "consistency"
        // or refactors both helpers into one) re-introduces the bug.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let path = std::env::temp_dir().join(format!(
            "resinsim-viz-button-test-{}.png",
            std::process::id()
        ));
        let id = app
            .world_mut()
            .register_system(move |mut commands: Commands| {
                spawn_button_screenshot(&mut commands, &path);
            });
        app.world_mut()
            .run_system(id)
            .expect("system runs to completion");
        let world = app.world_mut();
        let mut q = world.query_filtered::<Entity, With<Screenshot>>();
        let total = q.iter(world).count();
        let mut q_marker = world.query_filtered::<Entity, With<AutoCaptureMarker>>();
        let with_marker = q_marker.iter(world).count();
        assert_eq!(total, 1, "exactly one Screenshot entity spawned");
        assert_eq!(
            with_marker, 0,
            "button captures must NOT carry AutoCaptureMarker"
        );
    }

    #[test]
    fn spawn_auto_screenshot_spawns_one_entity_with_auto_marker() {
        // Symmetric guard for the CLI path: --screenshot's
        // capture-and-exit semantics depend on the system shim's
        // auto_captured query (Captured + AutoCaptureMarker)
        // matching only auto-captures.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let path =
            std::env::temp_dir().join(format!("resinsim-viz-auto-test-{}.png", std::process::id()));
        let id = app
            .world_mut()
            .register_system(move |mut commands: Commands| {
                spawn_auto_screenshot(&mut commands, &path);
            });
        app.world_mut()
            .run_system(id)
            .expect("system runs to completion");
        let world = app.world_mut();
        let mut q = world.query_filtered::<Entity, With<Screenshot>>();
        let total = q.iter(world).count();
        let mut q_marker =
            world.query_filtered::<Entity, (With<Screenshot>, With<AutoCaptureMarker>)>();
        let with_marker = q_marker.iter(world).count();
        assert_eq!(total, 1, "exactly one Screenshot entity spawned");
        assert_eq!(with_marker, 1, "auto captures MUST carry AutoCaptureMarker");
    }

    #[test]
    fn capture_inner_terminal_decisions_never_refire() {
        // Round-4 code M1 fix (v6 NEW): drive a terminal decision,
        // then advance many frames; the has_exited guard prevents
        // re-fire. Distinguished from idempotent_after_exit which
        // tests only the immediate next call; this verifies the
        // latch survives N extra frames.
        let mut s = State::default();
        let args = args_with_screenshot("/tmp/x.png");
        s.spawn_fired = true;
        let _ = drive(&mut s, &args, true, true, true); // ExitSuccess
        for _ in 0..50 {
            assert_eq!(
                drive(&mut s, &args, true, true, true),
                CaptureDecision::Skip,
                "terminal decision must not re-fire"
            );
        }
    }

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "resinsim-viz-screenshot-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn validate_rejects_empty_path() {
        assert_eq!(
            validate_screenshot_path(Path::new("")),
            Err(PathError::Empty)
        );
    }

    #[test]
    fn validate_rejects_existing_directory() {
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_with_png_ext = dir.join("inner.png");
        std::fs::create_dir_all(&dir_with_png_ext).unwrap();
        assert_eq!(
            validate_screenshot_path(&dir_with_png_ext),
            Err(PathError::IsDirectory)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_missing_parent() {
        let bogus = PathBuf::from("/nonexistent-parent-dir-123/shot.png");
        assert_eq!(
            validate_screenshot_path(&bogus),
            Err(PathError::ParentMissing {
                parent: PathBuf::from("/nonexistent-parent-dir-123"),
            })
        );
    }

    #[test]
    fn validate_rejects_bad_extension() {
        // Build path under an existing parent dir to isolate the
        // extension check from the parent-existence check.
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("shot.txt");
        match validate_screenshot_path(&bad) {
            Err(PathError::BadExtension { actual }) => assert_eq!(actual, "txt"),
            other => panic!("expected BadExtension, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_no_extension() {
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("shot");
        match validate_screenshot_path(&bad) {
            Err(PathError::BadExtension { actual }) => assert!(actual.is_empty()),
            other => panic!("expected BadExtension(empty), got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_accepts_absolute_png() {
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("shot.png");
        let resolved = validate_screenshot_path(&p).expect("absolute .png in existing dir");
        assert!(resolved.is_absolute());
        assert_eq!(resolved.extension().and_then(|e| e.to_str()), Some("png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_accepts_jpg_and_jpeg() {
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["jpg", "jpeg"] {
            let p = dir.join(format!("shot.{ext}"));
            assert!(
                validate_screenshot_path(&p).is_ok(),
                "extension .{ext} must be accepted"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_accepts_relative_path_when_cwd_resolvable() {
        // Don't change the actual CWD in the test process — just assert
        // that a bare filename with .png extension passes (CWD lookup
        // succeeds in the test runner).
        let resolved = validate_screenshot_path(Path::new("shot.png"))
            .expect("relative .png must resolve when CWD is available");
        assert!(resolved.is_absolute());
    }

    #[test]
    fn format_path_error_each_variant_renders_three_line_message() {
        let cases: Vec<(PathError, &str)> = vec![
            (PathError::Empty, "fix:"),
            (PathError::IsDirectory, "fix:"),
            (
                PathError::ParentMissing {
                    parent: PathBuf::from("/x"),
                },
                "mkdir -p",
            ),
            (
                PathError::BadExtension {
                    actual: "txt".to_string(),
                },
                ".png",
            ),
            (
                PathError::BadExtension {
                    actual: String::new(),
                },
                "no extension",
            ),
            (PathError::CwdUnavailable, "absolute path"),
        ];
        for (err, must_contain) in cases {
            let msg = format_path_error(Path::new("/some/path.png"), &err);
            // Each rendered message is multi-line (newline-separated)
            // and contains a WHAT/WHY/FIX shape — assert the FIX
            // section is present + the variant-specific phrase.
            assert!(msg.contains("what:") || msg.starts_with("--screenshot path is empty"));
            assert!(msg.contains("fix:") || msg.contains("mkdir -p"));
            assert!(
                msg.contains(must_contain),
                "variant {err:?} message missing '{must_contain}': {msg}"
            );
        }
    }
}
