---
issue: 04-egui-control-panels
date: 2026-04-26
---

# UAT: PrintSimulation cumulative_times_sec accessor

## Background

Issue 04 added `PrintSimulation::cumulative_times_sec(&self) -> Vec<f32>`
as a narrow public accessor that delegates to
`LayerTimingCalculator::cumulative_times_sec` against the aggregate's
owned recipe + printer. The accessor is the time-axis bridge for
`resinsim-viz`'s plot panels (see `docs/adr/0011-egui-control-panels.md`)
and any future report extension that needs a per-layer time series.

These scenarios pin the contract surface so a future refactor of the
aggregate's internal recipe/printer ownership doesn't silently break
downstream consumers.

## UAT-1: cumulative_times_sec is parallel-indexed with layers()

**Rationale.** Plot consumers zip `sim.layers()` with
`sim.cumulative_times_sec()` to build `(t, y)` pairs. Length parity is
the load-bearing invariant — a one-off length mismatch would produce
silent zip truncation in plot rendering.

```gherkin
Scenario: cumulative_times_sec is parallel-indexed with layers()
  Given a PrintSimulation built from a 100-layer cube via
        SimulationRunner::run_from_layer_inputs
  When the cumulative_times_sec accessor is called
  Then the returned Vec has the same length as sim.layers()
  And every value is finite and non-negative
  And the sequence is monotonic non-decreasing
```

## UAT-2: cumulative_times_sec is empty for an empty aggregate

**Rationale.** `PrintSimulation::new(recipe, printer)` constructs an
aggregate with zero layers (no `add_layer` calls). The accessor must
return an empty Vec — not panic, not return a one-element zero. This
matches the `--load-sim` startup flow that may briefly observe an
empty aggregate before the JSON sidecar deserialises.

```gherkin
Scenario: cumulative_times_sec is empty for an empty aggregate
  Given a PrintSimulation constructed via PrintSimulation::new with
        no layers added
  When the cumulative_times_sec accessor is called
  Then the returned Vec is empty
```
