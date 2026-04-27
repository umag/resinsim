---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# Pattern: capture-and-exit for the AI visual-feedback loop

## Why

Persistent-rendered tools (Bevy GUIs, web dashboards, terminal UIs
with dynamic redraws) create a feedback gap for autonomous AI
development: the agent can run unit tests, inspect components /
state, but cannot SEE the rendered surface. Visual regressions
(missing emissive material, wrong z-clip, broken layout) ship past
review-matrix gates because no tooling lets the agent inspect pixels
(see `docs/patterns/anti/visual-spec-drift-no-test-guard.md`).

## Pattern

Add a CLI flag like `--screenshot PATH.png` that:

1. Loads the configured scene / state (existing CLI flags work as today).
2. Renders the first 2-3 frames so the rendering pipeline settles
   (PBR shaders, transparency sort, mesh upload).
3. Captures a frame to PATH via the rendering framework's screenshot
   API (Bevy's `Screenshot::primary_window()` + `save_to_disk`
   observer for Bevy 0.18).
4. Defers process exit until BOTH the framework signals capture-complete
   AND the file lands on disk.
5. Exits with a meaningful exit code (0 success, distinct non-zero
   codes for load-failed / render-timeout / write-failed).

The agent workflow becomes:

```bash
# Make a visual change
$EDITOR src/scene.rs

# Capture the result
./bin --load-asset X --screenshot /tmp/shot.png && file /tmp/shot.png

# Read the PNG (Claude Code's Read tool handles PNGs natively).
# The agent SEES what the user sees; iterates without a
# human-in-the-loop visual round.
```

## Required components

- **Phase-aware capture state machine.** Don't fire `AppExit` until
  the file actually lands. Bevy's screenshot pipeline is asynchronous;
  see `docs/patterns/bevy-0.18-sticky-marker-latch.md` for the
  Captured-marker race.
- **Multiple distinct exit codes.** AI agents branch on `$?`. Don't
  conflate "render timeout" with "filesystem write failed" — the
  recovery actions differ (try different machine vs investigate
  filesystem). See `docs/adr/0013-screenshot-exit-code-disjunction.md`.
- **Path validation BEFORE rendering starts.** No point spinning up
  the GUI if the output path is invalid; eprintln + exit immediately.
- **Phase-1 settle timeout.** Some loads (large CTBs, network assets)
  can hang forever. After N frames of "not ready", capture anyway
  with a warn — the partial scene is still a useful signal.
- **`main()` must honour `app.run()` return.** Without this, every
  fatal_exit call is silent. See
  `docs/patterns/anti/bevy-app-exit-return-discarded.md`.

## When to adopt

- The component renders to screen and the rendering itself is a
  contract (vs. just supporting UI).
- Visual regressions have shipped past review before (KB
  `visual-spec-drift-no-test-guard` history).
- The agent workflow benefits from rapid iteration on visual changes.

## When NOT to adopt

- Rendering is purely a debug aid, not a contract surface.
- The screenshot would be uninformative (e.g., the rendering depends
  on real-time inputs the CLI can't synthesise).

## See also

- `docs/patterns/anti/visual-spec-drift-no-test-guard.md` (the
  motivation this pattern closes)
- `docs/patterns/bevy-0.18-sticky-marker-latch.md` (Bevy schedule
  precedence required to implement Phase 3 correctly)
- `docs/patterns/anti/bevy-app-exit-return-discarded.md` (exit-code
  contract requires main() to honour app.run() return)
- `docs/patterns/anti/timestamp-filenames-without-millis.md` (the
  button-click default-path footgun this pattern can hide)
- `docs/adr/0013-screenshot-exit-code-disjunction.md` (why
  --screenshot and --smoke-exit are independent triggers; why
  exit codes 7 and 8 are split)
- Issue 12 implementation: `crates/resinsim-viz/src/screenshot.rs`
- Issue 12 UAT: `spec/uat/viz-screenshot-flag.md`
