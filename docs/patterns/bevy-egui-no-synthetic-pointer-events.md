---
issue: viz-timeline-click-stepdefs
date: 2026-08-16
---

# Pattern: bevy_egui 0.39 has no synthetic pointer-event API

## Context

Several viz UAT scenarios need to drive egui chart interactions
in-process: clicking the layer timeline chart to seek
(`plot_ui.response().clicked()` + `plot_ui.pointer_coordinate()` in
`render_layer_timeline`, `crates/resinsim-viz/src/ui/plots.rs`),
dragging to pan without seeking, and drag-drop onto egui panels.

Bevy keyboard input is injectable — `ButtonInput::<KeyCode>::press()`
+ `reset_all()` works for in-process tests (the arrow-key UAT specs
use this successfully via the lib.rs + main.rs split). Pointer input,
however, flows through bevy_egui's internal `RawInput` pipeline:
bevy_egui reads Bevy's `CursorMoved` / `MouseButtonInput` events,
translates them into egui `RawInput`, and passes them to
`egui::Context::run()`. The user's code never touches `RawInput`
directly, and bevy_egui 0.39 exposes no public API to inject synthetic
pointer events into it.

## Pattern

When a cucumber scenario's When clause requires an egui pointer
interaction (click, drag, hover) that flows through
`EguiContexts` → `RawInput` → `PlotUi::response()`, document the
scenario as **declared debt** in the step-def module's doc comment,
citing this pattern doc and the specific bevy_egui version. The
register entry carries the scenario's skip count as before.

The doc comment template:

```
//! ALL N SCENARIOS ARE DECLARED DEBT. [Each/Every] scenario requires
//! egui chart pointer interaction — <specific API path> — which needs
//! synthetic egui pointer events that bevy_egui 0.39 cannot produce.
//! See docs/patterns/bevy-egui-no-synthetic-pointer-events.md.
```

## When to use

Any viz UAT scenario whose only blocked step is egui pointer delivery,
not Bevy type access or production logic. The distinction matters:
`snap_plot_x_to_layer` IS testable (it's a pure function, already
unit-tested in `plots.rs`); the blocked path is specifically the
chain from "user clicks at screen coordinate" to "egui's
`Response::clicked()` returns true inside the `Plot::show` closure."

## Affected scenarios (as of 2026-08-16)

- ~~`viz-screenshot-flag` UAT-6~~ — **resolved**: bypassed bevy_egui
  entirely by constructing a headless `egui::Context` and injecting
  `Event::PointerButton` via `RawInput::events` (egui 0.33 supports
  this natively). See `viz_screenshot_egui.rs`. This bypass works for
  scenarios whose UI closure can be replicated outside bevy_egui's
  system pipeline (standalone buttons, simple widgets).
- `viz-load-sim-missing-sidecar` UAT-2 (drag-drop)
- `viz-timeline-click-seeks-current-layer` UAT-1/2/3 (chart click)
- `viz-timeline-drag-pan-does-not-seek` UAT-1/2 (chart drag vs click)

The remaining scenarios involve `egui_plot`'s `PlotUi` interaction
pipeline (`plot_ui.response().clicked()`, `plot_ui.pointer_coordinate()`),
which is harder to replicate outside bevy_egui because the plot's
coordinate transform depends on its layout within the full panel
hierarchy.

## Revisit trigger

bevy_egui gains a public API for injecting synthetic pointer events
into egui's `RawInput` (e.g. an `EguiContexts::inject_pointer_event`
method, or exposure of `Context::run()` with caller-supplied
`RawInput`), OR the project migrates to a bevy_egui version that
provides this. For simple button-click scenarios, the headless
`egui::Context` bypass (see UAT-6 resolution above) is already
viable.

## See also

- `docs/patterns/env-gated-fixture-with-trivial-pass-step.md` — the
  env-gated fixture pattern (a different debt shape, not pointer-blocked)
- `docs/adr/0024-second-uat-harness-in-resinsim-viz.md` — why the viz
  harness is separate from core's
