---
issue: uat-unskip-campaign
date: 2026-08-01
---

# Pattern: Per-spec runtime skip attribution for a cucumber debt register

## Context

The old guard compared spec/uat/*.md stems against `pub mod` lines — it
answered "does this spec have a module?", never "does that module cover
every scenario?". Five scenarios sat silently dead in two stepped,
off-register specs (spec text drifted post-ADR-0015; regexes did not) —
`anti/guard-that-cannot-observe-its-own-failure-mode.md` recurring one
commit after it was harvested.

## Pattern

Drive cucumber **one feature file at a time** inside the harness main(),
accumulating per-spec (passed, skipped, failed, parsing_errors) from the
public `StatsWriter` counters (re-entrancy of repeated
`World::cucumber().run()` in one process was probe-verified first). Then:

- The register becomes `(spec, expected_skipped_scenarios)` pairs and
  **fails in three directions**: an unexpected skip in an unregistered
  spec; a registered spec with zero skips; a count mismatch either way.
- Keep the static mod.rs existence check as a second layer, and assert the
  mod.rs declaration set equals the `use uat_steps::{...}` set (the
  `-Aunused_imports` environment makes a missing `use` otherwise silent).
- The silent-green guard runs PER FEATURE, not just in aggregate.
- **Amended register rule**: entries may be ADDED only as declared debt
  naming a blocking issue (live instance:
  `("cli-temperature-flag-validation", 1)` →
  `kb153-warning-missing-from-resinsim-sim`); net scenario-debt still
  monotonically shrinks.
- **Fault-injection proof obligation**: every direction must be shown red
  on demand before the guard is trusted (disable a step, add a stale
  entry, corrupt a count, drop a use-entry — four injections, four exact
  messages).
- Scenario Outlines register their RUNTIME expansion count (3 authored
  rows → 5), documented at the entry.

## When to use

- Any custom test harness with an allowlist/debt register
- Whenever "has coverage" is answered structurally while the runtime can
  answer it empirically

## See also

- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`
- `docs/patterns/anti/spec-edited-step-regex-not.md` — the drift class
  this guard detects
- `docs/patterns/silent-green-guard-for-custom-test-harness.md`
