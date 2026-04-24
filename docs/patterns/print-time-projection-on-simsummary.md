---
issue: print-time-on-reportgenerator
date: 2026-04-24
---

# Pattern: Domain projections go on the aggregate root that OWNS the domain entities the projection reads

## Context

A user needed `resinsim report health` to surface total print duration plus
a bottom/transition/normal phase breakdown. The underlying time primitive
(`LayerTimingCalculator::cumulative_times_sec`) was already in place and fully
covered every dependency — release mechanism (ADR-0007 Linear vs Tilt), the
3-phase exposure model, wait fields, lift geometry. The work was purely at
the presentation boundary: `SimSummary` had no time fields, so
`cmd_report_health` had nothing to print. The open question was architectural:
where does the projection live?

## Part A — First-class projection on SimSummary

### Decision

Add `total_time_sec`, `bottom_time_sec`, `transition_time_sec`, `normal_time_sec`
as f32 fields on `SimSummary`. Reshape `PrintSimulation` so the aggregate
OWNS the `Recipe` + `PrinterProfile` (both by Clone-owned value). The
projection signature is arg-less: `sim.summary()` reads the aggregate's
pinned recipe + printer internally.

### Why this shape

`SimSummary` is already a projection over the `PrintSimulation` aggregate —
it scans layer results to produce extrema (max peel force, min safety factor,
etc.). Adding a time projection is consistent with that role. The tension
is that time depends on inputs the aggregate previously did not own (Recipe +
PrinterProfile). Three candidate shapes, and the winner:

- **(Chosen) Aggregate owns Recipe + PrinterProfile.** `PrintSimulation::new(recipe,
  printer)` takes both at construction. `impl Default` is removed (no sensible
  default exists once the aggregate requires these domain entities — verified
  zero callers of `PrintSimulation::default()` before removal). The struct
  holds `recipe: Recipe, printer: PrinterProfile, layers: Vec<LayerResult>,
  failures: Vec<FailureEvent>` — matching the pre-existing docstring
  ("Aggregate root: a complete simulation run for one geometry + resin +
  printer"). `summary()` is arg-less; all current and future projections on
  the aggregate consume `self.recipe + self.printer` directly, so callers
  don't re-thread parameters.

- **(Rejected) Parameter-injection: `summary(&Recipe, &PrinterProfile)`.**
  Every call site threads both refs explicitly. Less aggregate state change
  but every future projection on `PrintSimulation` (force-profile stats,
  temperature-history stats) replays the same parameter-threading burden
  and the aggregate's docstring contract stays aspirational rather than
  structural. This was the v3 plan direction; rejected in v4 review round
  as the less clean DDD move.

- **(Rejected) Free function `SimSummary::compute(&sim, &recipe, &printer)`.**
  Breaks the aggregate-owns-its-projections convention already established
  by the existing `sim.summary()` method. Two projection patterns in one
  codebase invites drift.

- **(Rejected) Compute time at the CLI layer and attach it to a derived
  struct alongside SimSummary.** Pushes a domain projection out of the
  domain. Callers can't rely on SimSummary carrying the full story.

### Short-print clamp semantics

The phase split must handle every layer-count regime:

- `total_layers == 0`: all four time fields are zero.
- `total_layers < bottom_count`: only `bottom_time_sec` is non-zero;
  `transition` and `normal` are zero.
- `total_layers < bottom_count + transition_layers`: `normal` is zero.

Implementation uses explicit `min()` clamps and conditional `cumulative[i-1]`
lookups to avoid out-of-bounds panics. Unit tests pin each regime.

### Phase-A additive

The SimSummary extension is [Phase-A](phase-boundaries-for-ddd-refactors.md):
every change is compile-forced. The signature change on
`PrintSimulation::new(recipe, printer)` is enforced by the compiler across
all 9 construction sites (1 production in `SimulationRunner::run_inner`, 5
test fixtures in `report_generator.rs`, 3 test fixtures in
`print_simulation.rs`). The `summary()` reversion to arg-less and
`impl Default` removal are also compile-forced. All three transitions land
atomically in the same commit so the workspace never enters an uncompilable
intermediate state.

## Part B — Text output formatting via H:MM:SS helper

### Decision

Introduce `crates/resinsim-core/src/app/formatters.rs` with
`pub fn format_duration_hms(secs: f32) -> String`. It renders finite
non-negative seconds as `H:MM:SS` (hours unbounded — print jobs routinely
exceed 24h), and non-finite or negative values as `—` (U+2014 em-dash) so
human output never leaks NaN/∞/negative-duration noise.

### Where this helper lives

Placed at the application layer (`crates/resinsim-core/src/app/`) rather
than at the CLI layer (`crates/resinsim-inspect/src/`) because its sole
caller is `ReportGenerator::text_format`, which is itself in the app
layer. Colocating helper with caller minimises the import graph and keeps
the text-rendering logic a pure function of domain data.

A future non-CLI consumer (e.g. the hypothetical Bevy viz UI referenced in
the `ReportGenerator` module doc) may want a different duration format
(localised, tick-marked, relative). At that time, consider promoting the
helper to a `DurationFormatter` trait or moving presentation-layer helpers
back to the consuming CLI crate. For this lifecycle the helper is
single-caller so over-engineering would be premature.

## Cross-command time formatting (documented divergence)

Within the same CLI, two commands report time using different text formats:

| Command | Text format | JSON format |
|---------|-------------|-------------|
| `resinsim report health` | `H:MM:SS` (total + per-phase) via `format_duration_hms` | raw seconds (`*_time_sec` f32 keys) |
| `resinsim inspect thermal` | float minutes (`s.time_sec / 60.0`) | raw seconds (`time_sec` f32 key) |

JSON is consistent across commands (`*_sec` seconds everywhere — the
machine-readable contract stays uniform). TEXT diverges by granularity:

- `report health` shows a single multi-hour total per print — H:MM:SS
  reads naturally ("4:42:48" vs "16968.75 seconds"). Per-phase breakdown
  follows the same format for alignment.
- `inspect thermal` shows a per-layer curve with minute-scale evolution —
  float minutes reads naturally at the curve's granularity and aligns
  with other per-layer metrics in the same table (`vat_temperature_c`,
  `viscosity_mpa_s`, etc.).

Different granularities warrant different formats. If a future unification
is desired (e.g. everyone adopts H:MM:SS), it should be a deliberate
cross-command consistency pass, not a drive-by change during a feature.

## References

- ADR-0001 — DDD layer dependency rule. Note: this ADR is specifically
  about `values/` not importing `entities/` — it is NOT a general
  "projections live in the domain" rule. The projection placement decision
  above stands on its own DDD merits, not on ADR-0001.
- ADR-0005 — Recipe owns per-layer time inputs (exposure phases, waits,
  lift_cycle).
- ADR-0007 — LED + vat are separate coupled surfaces; per-layer time
  branches on release mechanism.
- [phase-boundaries-for-ddd-refactors](phase-boundaries-for-ddd-refactors.md)
  — Phase A (additive) vs Phase B (switchover) pattern.
- [golden-file-byte-identity-guard](golden-file-byte-identity-guard.md) —
  the capture-then-verify discipline used when re-capturing the
  `report_health_athena_ii.{text,json}.golden` fixtures after adding the
  4 new fields.
- `crates/resinsim-core/src/services/layer_timing_calculator.rs` — the
  single time primitive.
- `crates/resinsim-core/src/simulation/print_simulation.rs` — projection
  site with the clamp semantics and the aggregate's owned
  recipe/printer fields.
- `crates/resinsim-core/src/app/formatters.rs` — H:MM:SS helper.
