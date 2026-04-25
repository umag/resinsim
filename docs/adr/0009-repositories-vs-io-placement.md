---
issue: repos-placement-cleanup
date: 2026-04-25
---

# ADR-0009: `repositories/` vs `io/` placement rule

## Status
Accepted

## Context

`resinsim-core` has two sibling modules that both touch the filesystem:

- `repositories/` (today: `printer_repo.rs`, `resin_repo.rs`)
- `io/` (today: `athena.rs`, `ctb.rs`, `geometry.rs`, `sliced.rs`, `stl.rs`)

The `phase1-verification-audit` (Item 3) flagged that the placement rule
between the two was undocumented: `simulation_repo` and `athena_repo` were
absent, the Athena read path lived at `io/athena.rs`, and there was no
written rule explaining why or what should change as the codebase grew.

Two specific decisions needed to be taken at the same time as the rule:

1. Should the Athena read path become a `CalibrationDataset` aggregate +
   `AthenaRepository` now, or stay as an I/O adapter?
2. Does `Phase 2` (Bevy viz) need to reload prior simulation runs from
   disk? If so, a `SimulationRepository` is warranted.

## Decision

### Placement rule

- **`repositories/`** holds aggregate persistence: `load`, `save`, `list`
  (or any subset that the lifecycle requires) of an entity that is itself
  an aggregate root, with a stable identity (typically a caller-supplied
  `name`). The repository's job is to serialise and deserialise the
  aggregate while preserving its invariants.

- **`io/`** holds external read adapters with no domain identity: raw
  sensor CSV (`athena.rs`), geometry imports (`stl.rs`, `geometry.rs`),
  slicer outputs (`ctb.rs`, `sliced.rs`). The data crossing this boundary
  is inert — `ForceRecord`, `Triangle`, byte-blobs — and never carries an
  aggregate's lifecycle (no validation invariants, no in-domain mutation,
  no identity beyond "a path on disk somebody handed us").

The split is about **identity** and **invariants**, not about read-vs-write:
a future `io/` adapter that writes a slicer file is still I/O, because the
file has no aggregate lifecycle of its own.

### Aggregates across modules

Aggregates today live in **two** sibling modules:

- `entities/` — leaf aggregates whose state fits one file:
  `PrinterProfile`, `ResinProfile`, `Recipe`.
- `simulation/` — complex aggregates with owned services, projections,
  and module-private helpers that warrant their own module:
  `PrintSimulation` (owns `Recipe + PrinterProfile + Vec<LayerResult> +
  Vec<FailureEvent>`, exposes `summary()` projection, computes phase-time
  via `LayerTimingCalculator`).

`repositories/` may import from **either** `entities/` or `simulation/`
(or any future aggregate-hosting module), provided the target type
carries an explicit "aggregate root" docstring. This is consistent with
ADR-0001: layering today goes
`values → entities → services → simulation`, and `repositories/` lives
above the chain (importing from `entities/` and `simulation/` only). No
cycle is introduced because neither `entities/` nor `simulation/` imports
from `repositories/`.

### Athena: defer aggregate promotion to Phase 3

`io/athena.rs` stays an I/O adapter. `ForceRecord` and `ForceStats` are
inert read structures — no validation invariants, no aggregate boundary,
no in-domain mutation. The only consumer today is `resinsim-inspect`'s
ad-hoc CSV inspection while the team waits for production force data
from the Athena II calibration runs.

Promotion to a `CalibrationDataset` aggregate + `AthenaRepository` is
**deferred to Phase 3** (calibration pipeline). The trigger criteria for
revisiting:

- Athena force data becomes mutable in-domain (e.g. recipe-tuning loop
  that adjusts parameters from observed forces);
- Calibration runs need persistent identity (multiple runs to compare,
  by name or hash);
- A `CalibrationDataset` concept emerges in the simulation core path
  (e.g. `SimulationRunner` consumes calibration data alongside Recipe);
- Phase 3 explicitly designs a calibration-tuning workflow.

Until any of those land, Athena force data stays an `io/` adapter and
`resinsim-inspect` continues to consume it directly.

### SimulationRepository: build now

Phase 2 (Bevy viz) requires reload of prior simulation runs — simulations
must be repeatable and reproducible. `PrintSimulation` is already
documented as an aggregate root (`crates/resinsim-core/src/simulation/print_simulation.rs`
docstring at line 7-12) and already derives `Serialize + Deserialize`,
so the repository is a thin layer on top:

- Format: **JSON** via `serde_json::to_string_pretty` /
  `serde_json::from_str`. TOML handles `Vec<LayerResult>` (potentially
  hundreds of entries with per-layer fields) poorly; JSON is already
  used by `app::ReportGenerator` so consumers know the format.
- Filename: `<data_dir>/<name>.json`, caller-supplied name (no UUID or
  timestamp generation; matches `printer_repo` / `resin_repo`
  conventions).
- Directory semantics: `save` calls `fs::create_dir_all(data_dir)`
  because simulations are user-output (callers may not have pre-created
  the dir); `load` and `list` error on missing directory like the
  existing repos do (read semantics: fail loud).

#### Deserialize-bypass guard

The `PrintSimulation` deserialize path bypasses both child-entity
`validate()` calls and the `add_layer` constructor invariant
("layers must be added sequentially", `print_simulation.rs:75-79`).
This ADR introduces `pub fn validate(&self) -> Result<(), String>` on
`PrintSimulation`, which performs three checks:

1. `self.recipe.validate()`
2. `self.printer.validate()`
3. `layers[i].index == i as u32` for every i in `0..self.layers.len()`

`SimulationRepository::load` calls `sim.validate()` after
`serde_json::from_str` and before returning, so a tampered or
schema-evolved file cannot silently violate aggregate invariants.

### JSON forward-compatibility (followup)

Additive `PrintSimulation` field changes work via `serde` defaults.
Field renames or removals will break existing files.

This is **not** a v1 concern — no on-disk corpus exists yet — but
becomes a real concern once Phase 2 callers start persisting runs. The
natural mitigation is a top-level `schema_version: u32` field on
`PrintSimulation` plus version-aware `load`. This is recorded as a
followup; the trigger is "an on-disk corpus emerges that we need to
preserve across a `PrintSimulation` field rename".

## Consequences

- One new ADR file (this one). One new aggregate-level method
  (`PrintSimulation::validate`). One new repository
  (`SimulationRepository`). One re-export line in `repositories/mod.rs`.
  Phase A additive — no field moves, no caller signatures change.
- `repositories/` gains a new import edge to `simulation/`. Documented
  here; ADR-0001 layering rule preserved.
- Athena placement is now codified as deferred-by-design. Future
  contributors picking up calibration work have an explicit "revisit
  this ADR" checklist instead of an undocumented ambiguity.
- The placement rule is concrete enough that the next person to add a
  module-spanning concern (e.g. a slicer writer, a calibration-results
  store) can decide `io/` vs `repositories/` from the rule alone, without
  re-litigating this question.

## See also

- ADR-0001 — values layer must not import entities (sets the layering-as-ADR precedent)
- `docs/patterns/aggregate-shape-matches-docstring-contract.md` —
  aggregate boundaries are docstring contracts
- `docs/patterns/phase-boundaries-for-ddd-refactors.md` — Phase A
  additive vs Phase B switchover (this whole change is Phase A)
