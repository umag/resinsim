---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# UAT: `resinsim-viz --screenshot PATH.png` and Capture-screenshot button

## Rationale

Closes the AI visual-feedback gap (KB-3 anti-pattern: review matrix
tick-the-box failure on issue 03 because no tooling let the agent SEE
rendered pixels). With `--screenshot`, the agent can:

  1. Make a visual change
  2. Run `resinsim-viz --load-ctb X --load-sim Y --screenshot /tmp/shot.png`
  3. Read the PNG (Claude Code is multimodal — Read handles PNGs)
  4. See what the user sees; iterate without a human-in-the-loop visual round

## Grep contract

UAT scenarios assert on the literal substring `Screenshot saved to ` (Bevy
native, `bevy_render-0.18.1/src/view/window/screenshot.rs:141`). Stable
across log-subscriber configurations.

## Test coverage notes

`EXIT_SCREENSHOT_WRITE_FAILED=7` (mid-capture filesystem failure) and
`EXIT_SCREENSHOT_RENDER_TIMEOUT=8` (Bevy never produces Captured) are
tested at unit-test level only because reliable mid-capture failure /
GPU hang is platform-specific and racy. See:

  - `capture_inner_exit_write_failed_after_captured_no_file`
  - `capture_inner_exit_render_hung_when_bevy_never_captures`

in `crates/resinsim-viz/src/screenshot.rs` tests.

`UAT-10` (button-click programmatic test) was originally planned but
dropped per code-r5 MEDIUM #3: bevy_egui 0.39 doesn't expose a stable
synthetic-pointer-click API, and a 1-hour spike would yak-shave the
test harness without strengthening the regression signal. Verified
instead via Round B of the manual gate (see `spec/uat/`'s associated
manual checklist or the implementation summary).

## UAT-1: --screenshot writes a PNG and exits success

```gherkin
Scenario: UAT-1 --screenshot writes a PNG of the default scene and exits 0
  Given the resinsim-viz binary
  When the user invokes it with --screenshot /tmp/uat1.png
  Then the process exits with code 0
  And the file /tmp/uat1.png exists
  And stderr contains "Screenshot saved to "
```

## UAT-2: --screenshot + --load-ctb + --load-sim captures a coloured slice-stack with cursor (KB-3 motivation)

```gherkin
Scenario: UAT-2 --screenshot captures the issue-03 visual surface
  Given a fixture .ctb file at $RESINSIM_SLICED_FIXTURE
  And a matching sim JSON at $RESINSIM_SIM_FIXTURE
  When the user invokes it with --load-ctb <ctb> --load-sim <sim> --screenshot /tmp/uat2.png
  Then the process exits with code 0
  And /tmp/uat2.png exists
  And the file size of /tmp/uat2.png is > 10240 bytes
  And the agent reading the PNG observes a coloured slice-stack with a layer cursor
```

## UAT-3: --screenshot propagates EXIT_BAD_SIM_PAIRING=4 when --load-sim is passed without --load-ctb

```gherkin
Scenario: UAT-3 --screenshot propagates exit-4 on bad sim pairing
  Given the resinsim-viz binary
  When the user invokes it with --load-sim foo.sim.json --screenshot /tmp/uat3.png
  Then the process exits with code 4
  And stderr contains "--load-sim was supplied without --load-ctb"
  And /tmp/uat3.png does NOT exist
```

## UAT-4: --screenshot + --smoke-exit — screenshot wins, PNG written, exit 0

```gherkin
Scenario: UAT-4 --screenshot wins over --smoke-exit (capture-and-exit beats one-frame-and-exit)
  Given the resinsim-viz binary
  When the user invokes it with --smoke-exit --screenshot /tmp/uat4.png
  Then the process exits with code 0
  And /tmp/uat4.png exists
  And stderr contains "Screenshot saved to "
```

## UAT-5: --screenshot writes a PNG of plausible size

```gherkin
Scenario: UAT-5 --screenshot produces a non-trivial PNG
  Given a fixture .ctb file at $RESINSIM_SLICED_FIXTURE
  When the user invokes it with --load-ctb <ctb> --screenshot /tmp/uat5.png
  Then the process exits with code 0
  And the file size of /tmp/uat5.png is > 10240 bytes
  And `file /tmp/uat5.png` reports a valid PNG image
```

## UAT-6: clicking Capture screenshot writes a PNG to CWD with a `resinsim-viz-<unix-secs>.png` filename

```gherkin
Scenario: UAT-6 Capture-screenshot button writes a CWD-scoped timestamped PNG
  Given a running resinsim-viz session (no --screenshot, no --smoke-exit)
  When the user clicks the "Capture screenshot" button in the left panel
  Then a file matching `resinsim-viz-<digits>.png` appears in the current working directory
  And stderr contains "Screenshot saved to "
  And the application keeps running (no AppExit)
```

## UAT-7: --screenshot rejects invalid paths before opening the window

```gherkin
Scenario: UAT-7a directory path → exit 5
  Given /tmp exists as a directory
  When the user invokes resinsim-viz --screenshot /tmp/uat7-dir.png
  And /tmp/uat7-dir.png is created as a directory before launch
  Then the process exits with code 5
  And stderr contains "is a directory"

Scenario: UAT-7b missing parent → exit 5
  When the user invokes resinsim-viz --screenshot /no/such/dir/x.png
  Then the process exits with code 5
  And stderr contains "parent dir"

Scenario: UAT-7c wrong extension → exit 5
  When the user invokes resinsim-viz --screenshot /tmp/x.txt
  Then the process exits with code 5
  And stderr contains "unsupported extension"

Scenario: UAT-7d empty path → exit 5
  When the user invokes resinsim-viz --screenshot ""
  Then the process exits with code 5
  And stderr contains "is empty"
```

## UAT-8: --screenshot + --allow-mismatch + sim layer-count mismatch — capture proceeds (sim soft-fail path)

```gherkin
Scenario: UAT-8 --screenshot with --allow-mismatch tolerates layer-count mismatch
  Given a fixture .ctb with N layers
  And a sim JSON with M ≠ N layers
  When the user invokes it with --load-ctb <ctb> --load-sim <sim> --allow-mismatch --screenshot /tmp/uat8.png
  Then the process exits with code 0
  And /tmp/uat8.png exists
  And stderr contains "--allow-mismatch is set, rendering uncoloured"
```

## UAT-9: --screenshot + --load-ctb /nonexistent.ctb → exit 6 + stderr line matching `CTB load failed for /nonexistent.ctb:`

```gherkin
Scenario: UAT-9 --screenshot propagates exit-6 on CTB load failure
  Given the file /nonexistent.ctb does NOT exist
  When the user invokes it with --load-ctb /nonexistent.ctb --screenshot /tmp/uat9.png
  Then the process exits with code 6
  And stderr matches "CTB load failed for /nonexistent.ctb:" followed by the underlying CTB parser error
  And /tmp/uat9.png does NOT exist
```
