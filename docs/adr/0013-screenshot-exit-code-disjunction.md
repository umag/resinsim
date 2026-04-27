---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# ADR-0013: --screenshot exit-code propagation is independent of --smoke-exit; codes 7 (write) and 8 (render) are split

## Status

Accepted (issue 12, plan v6, 2026-04-27).

## Context

Issue 12 added `--screenshot PATH.png` to resinsim-viz for the AI
visual-feedback loop (closes the
`visual-spec-drift-no-test-guard` anti-pattern). The pre-existing
exit-code surface was:

  - 2/3/4 — sim load / layer mismatch / pairing failures
  - Gated on `--smoke-exit` only (CI smoke-test mode)

The plan needed:

  - A new exit code for `--screenshot` path validation failures (5)
  - A new exit code for CTB load failures (6) — pre-existing gap;
    `--smoke-exit + bad-ctb` previously did not propagate
  - Some way to distinguish "screenshot didn't land" from "GPU
    didn't render"

## Decision

### Codes 7 and 8 are split

  - **EXIT_SCREENSHOT_WRITE_FAILED = 7**: Captured marker fired but
    no file landed within `MAX_FILE_WAIT_FRAMES`. Agent action:
    investigate filesystem (disk full, permission revoked, parent
    dir deleted after validation).
  - **EXIT_SCREENSHOT_RENDER_TIMEOUT = 8**: Spawned Screenshot but
    Bevy never produced Captured within `MAX_RENDER_FRAMES`. Agent
    action: try a different machine, headless config, or extend
    timeout for slow CI software rasterizers.

### --screenshot propagates exit codes WITHOUT requiring --smoke-exit

  - `should_propagate_exit_codes(&args)` returns
    `args.smoke_exit || args.screenshot.is_some()`. The two flags
    are INDEPENDENT triggers for the same propagation behaviour.
  - When both are set, --screenshot wins for the capture-and-exit
    contract (`smoke_exit_after_one_frame` is NOT registered) but
    both flags continue to gate the load-failure exit-code paths.

### --screenshot wins over --smoke-exit for exit-on-success

  - `--smoke-exit` alone fires `AppExit::Success` on frame 1.
  - `--screenshot` alone defers exit until Phase 3 succeeds (Captured
    AND file landed) — typically 2-5 frames.
  - Both set: --screenshot wins (`smoke_exit_after_one_frame` NOT
    registered). Otherwise --smoke-exit would fire on frame 1
    before the screenshot pipeline has a chance to capture.

## Consequences

### Good

- AI agents can disambiguate render-failure from filesystem-failure
  via exit code alone (no log parsing required).
- CI scripts using `--smoke-exit` see no change in behaviour.
- New CI/AI scripts using `--screenshot` get the same exit-code
  contract without redundantly passing both flags.

### Bad

- More distinct exit codes (8 instead of 5) — slightly more surface
  to memorise. Mitigated by the table in `--help` and README.
- The "--screenshot wins over --smoke-exit" rule is non-obvious;
  documented in the --screenshot docstring (rendered in --help) and
  the README "Screenshot capture" section.

### Neutral

- Round-D bug discovered during manual gate (`main()` discarding
  `app.run()` return) is orthogonal to this decision but blocked
  the contract from working in practice. See
  `docs/patterns/anti/bevy-app-exit-return-discarded.md`.

## Alternatives considered

### Collapse 7+8 into one code

Round-3 plan had `EXIT_SCREENSHOT_WRITE_FAILED = 7` cover both
cases. Round-4 adversarial M1 caught that an AI agent reading exit
7 cannot distinguish "retry filesystem" from "retry on different
machine". Split into 7+8 has near-zero implementation cost.

### Require --smoke-exit for any non-zero exit propagation

Would have meant scripts using --screenshot also pass --smoke-exit
redundantly. Adds friction; no benefit. The propagation contract
is INTENT-driven (the user wants exit codes), not flag-driven.

### Keep --screenshot as a "render and exit anyway" flag (no
exit-code propagation)

Would have shipped the AI feedback loop without the exit-code
contract. Agents would have to parse stderr to distinguish failure
modes. Strictly less useful.

## See also

- `crates/resinsim-viz/src/main.rs` `should_propagate_exit_codes`
- `crates/resinsim-viz/src/main.rs` EXIT_* constants
- `crates/resinsim-viz/src/screenshot.rs` `capture_screenshot_and_exit`
- `spec/uat/viz-screenshot-flag.md` UAT-3 / UAT-4 / UAT-9 (the
  exit-code propagation scenarios)
- `docs/patterns/capture-and-exit-for-ai-feedback-loop.md`
- `docs/patterns/anti/bevy-app-exit-return-discarded.md` (the
  bug that nearly broke this contract)
