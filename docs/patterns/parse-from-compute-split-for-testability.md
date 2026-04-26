---
issue: 04-egui-control-panels
date: 2026-04-26
---

# Pattern: split file-parse from compute so unit tests can bypass missing fixtures

## Context

A single-shot helper that does "parse file at path → run expensive
compute on parsed data → return result" is convenient for callers
but painful for tests when:

- the file format has no in-tree writer (parse-only crates,
  proprietary binary formats),
- no fixture is committed (gitignored, large, license-restricted),
- or the parse step itself is expensive and you want to test the
  compute step in isolation.

In `resinsim-viz`: `build_simulation(req, ctb_path, repos)`
initially took a path. CTB has a parser (`ctb::parse_ctb`) but no
writer; no CTB is committed in-tree
(`data/test_cube_10mm.ctb.README.md` is just a provenance pointer
for an env-gated fixture). The default test suite couldn't
exercise the happy path.

## Pattern

Split into two layers:

```rust
/// Pure compute on parsed inputs. Default-suite testable.
pub fn build_simulation_from_layers(
    req: &RunSimRequest,
    layers: &[LayerInput],
    repos: &ProfileRepos,
    initial_led_temp: Option<InitialLedTemperature>,
) -> Result<PrintSimulation, String>;

/// Thin file-I/O wrapper. Used by the Bevy system; covered by
/// env-gated integration tests.
pub fn build_simulation_from_path(
    req: &RunSimRequest,
    ctb_path: &Path,
    repos: &ProfileRepos,
    initial_led_temp: Option<InitialLedTemperature>,
) -> Result<PrintSimulation, String> {
    let (_info, layers) = ctb::parse_ctb(ctb_path)?;
    build_simulation_from_layers(req, &layers, repos, initial_led_temp)
}
```

Default-suite tests synthesise `Vec<LayerInput>` programmatically
(mirrors `crates/resinsim-core/tests/sim_summary_time_integration.rs`)
and drive `_from_layers` directly. End-to-end CTB coverage is
env-var-gated on `RESINSIM_SLICED_FIXTURE` and tests `_from_path`.

## When to use

Pipelines where:

1. Parse + compute are both non-trivial,
2. The parse step has no test-friendly inverse (no writer / no
   fixture),
3. Compute step has invariants worth pinning in the default
   suite.

## When NOT to use

Pipelines where parse and compute are tightly coupled (e.g.
streaming parsers that emit results lazily), or where the parse
step is so trivial that splitting it adds more surface than it
removes.

## Trade-offs

- **+** default-suite happy path stays green even when fixtures
  aren't committed,
- **+** parser bugs surface separately from compute bugs,
- **+** the `_from_layers` shape is reusable from any caller
  that already has parsed inputs (e.g. a future report
  extension),
- **−** two function names instead of one — small surface tax,
- **−** the `_from_path` wrapper is thin; mock-substitution is
  harder than it would be with a parse trait.

## First-party example

`crates/resinsim-viz/src/sim.rs::{build_simulation_from_layers, build_simulation_from_path}`
(issue 04).

The same shape is a prior art at the SimulationRunner layer:
`SimulationRunner::run_stl` / `run_from_areas` /
`run_from_layer_inputs` in `resinsim-core` already split file-I/O
(`run_stl`) from compute (`run_from_areas` / `run_from_layer_inputs`).
Issue 04's split is one level up at the viz orchestration layer.

## See also

- ADR-0011 — egui control panels
- `docs/patterns/bevy-app-test-seam.md` — companion pattern for
  Bevy system tests on plugin-less Apps
