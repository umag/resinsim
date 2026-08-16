---
issue: viz-timeline-toggle-stepdefs
date: 2026-08-16
---

# Pattern: In-process cucumber step defs via pure-function data projection

## Context

Cucumber step defs in the viz harness traditionally use subprocess
invocation (`invoke_viz`) to run the binary and assert on stderr/exit
code. This works for CLI-observable behavior but cannot reach internal
data projections (e.g. `build_layer_chart_data`'s filtering of
`f32::INFINITY` safety factors, series naming, log10 projection).

The `bevy-app-test-seam.md` pattern already established that egui
systems need a render backend and cannot be tested on a plugin-less
`App::new()`. The corollary: any assertion that depends on egui
rendering (checkbox visibility, Y-range bounds, Plot widget state) is
infeasible in a cucumber step def.

## Pattern

For specs whose load-bearing behavior lives in a **pure projection
function** (no `egui::Ui`, no Bevy system params), write cucumber step
defs that call the function directly:

1. **Given** constructs a synthetic `PrintSimulation` using
   `resinsim_core` builders (not the crate's private `ProfileRepos`).
2. **When** calls the pure function (e.g. `build_layer_chart_data`)
   and stores the result in `VizWorld`.
3. **Then** asserts on the returned data structure.

Egui-dependent steps register trivial-pass functions with doc comments
citing existing unit-test coverage.

## Requirements

- The pure function and its return types must be `pub` from the crate's
  public API. For `resinsim-viz`, this meant making `mod ui` → `pub mod ui`
  in `lib.rs` (application crate, no downstream consumers).
- `VizWorld` needs `Option<T>` fields for the in-process data
  (e.g. `Option<PrintSimulation>`, `Option<LayerChartData>`).
- The synthetic fixture helper will duplicate `#[cfg(test)]` helpers
  from the same-crate unit tests — structurally necessary since
  integration tests are a separate compilation unit.

## When to use

When a spec's assertions can be split into:
- **Pure data assertions** — testable via the projection function
- **Egui rendering assertions** — declared debt (trivial pass)

And the pure assertions carry the load-bearing correctness concern
(e.g. ∞-filtering, log10 domain safety, series naming).

## See also

- `bevy-app-test-seam.md` — the seam this pattern extends
- `env-gated-fixture-with-trivial-pass-step.md` — trivial-pass pattern
  for env-gated fixtures (complementary)
