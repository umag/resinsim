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

## Register row shape widened to two columns (`uat-unskip-band-d`, 2026-08-06)

The register entry is no longer a bare `(spec, expected_skipped_count)`
tuple. Each row is a `SpecDebt { spec, default_features, field_sim }`,
built via one of two `const fn`s:

- `both_configs(spec, n)` — the symmetric majority: `cargo uat` and
  `cargo uat-field-sim` skip exactly the same count.
- `per_config(spec, default_features, field_sim)` — a declared
  CONFIG-ASYMMETRIC row, where a field-sim-gated step-def module makes the
  spec's scenarios reachable in exactly one config.

A `HARNESS_CONFIG` marker (a mutually-exclusive `#[cfg(feature =
"field-sim")]` / `#[cfg(not(feature = "field-sim"))]` attribute pair, never
a bare `cfg!(...)`) selects which column the runtime guard compares
against — a typo'd feature name in either arm is then a compile error in
at least one of the two configs, rather than a silently false runtime
boolean. See `docs/patterns/band-membership-by-symbol.md` for the full
column-selection design and the columns-equal-by-construction rule.

## Two new failure directions (`uat-unskip-band-d`, 2026-08-06)

The three-direction fault-injection obligation below is now FIVE
directions, because the config-aware guards introduce two failure modes
the single-column register could not have:

4. **Unrecognised gate attribute.** The shared cfg-classifying parser
   (`classify_step_def_modules` in `uat_gherkin.rs`) reads the attribute
   line(s) immediately above each `pub mod` declaration and PANICS on
   anything it does not recognise — an unrecognised gate must be a loud
   parser failure, never silently treated as ungated (`Always`), which
   would misreport a genuinely conditional module's reachability.
5. **Asymmetric row with no gated module backing it.** Layer 1b
   (`assert_asymmetric_rows_have_a_gated_module`) fails whenever a row's
   two columns differ but no `FieldSimOnly`-gated module exists for that
   spec — closing the gap layer 2 cannot see alone (layer 2 only ever
   observes ONE column per process, so a false asymmetric claim could
   otherwise persist through an entire run of one alias before the OTHER
   alias ever disagreed with it).

## Extended fault-injection proof obligation

The four-injection obligation above is now TEN, split into two groups: the
original four RE-PROVED against any rewritten guard before new coverage
lands on top of it, plus six new ones proving the config-aware machinery
itself: a swapped asymmetric row's two columns; a typo'd feature name on a
gated `pub mod` line; a removed gate from a gated `pub mod` line (proving
the gate is load-bearing, not decorative); a deleted gated `use` block
with the gated `pub mod` line kept; a typo'd arm of the `HARNESS_CONFIG`
pair; and a symmetric row given unequal columns with no gated module.
Four of the ten surface as guard assertion failures with an exact message;
two (the removed gate, and the typo'd `HARNESS_CONFIG` arm) surface as
COMPILE errors instead — proving the gate and the marker pair are both
structurally load-bearing, not merely conventions a step-def author could
quietly violate. See the verified matrix in `uat_gherkin.rs`'s own doc
comment (precedent: the matrix in `agent_constraints_links.rs`) for every
exact message.

## When to use

- Any custom test harness with an allowlist/debt register
- Whenever "has coverage" is answered structurally while the runtime can
  answer it empirically

## See also

- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`
- `docs/patterns/anti/spec-edited-step-regex-not.md` — the drift class
  this guard detects
- `docs/patterns/silent-green-guard-for-custom-test-harness.md`
- `docs/patterns/band-membership-by-symbol.md` — the column-selection
  design and columns-equal-by-construction rule the two-column row shape
  above depends on
