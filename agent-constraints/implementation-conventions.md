# Implementation conventions for resinsim

These conventions extend the issue-lifecycle skill's Phase 4 (implementation)
defaults. The skill reads this file automatically when present; updates here
take effect on the next lifecycle.

## Linear-history rule (load-bearing)

resinsim keeps a **single linear history on `main`**. Every feature is a
small chain of commits stacked directly on the current tip of `main`; no
parallel feature branches accumulate.

This rule exists because issue-15 surfaced a real divergence cost: while
issue 15 was in progress on the viz-v2 redesign chain, an earlier
feature (`feat/05-layer-timeline-chart`) had been completed on a sibling
branch off `feat/12`. Two heads on the graph required a non-trivial
rebase + ADR renumbering + import-conflict resolution to merge back into
linear shape. The procedural fix below prevents that.

### Detecting divergence at session start

Before starting any new lifecycle, run:

```bash
jj log -r 'heads(all()) & ~description(glob:"*WIP*") & ~empty()' --limit 10
```

If more than one non-trunk head appears, **stop and rebase before starting
new work**. Multiple heads mean a previous lifecycle was completed without
advancing `main`, OR a parallel branch is in flight. Either way, fix the
shape first:

```bash
# Pick the canonical line and rebase the others onto its tip:
jj rebase -s <other-head> -d <canonical-tip>
# Then advance main:
jj bookmark set main -r <canonical-tip>
```

### Starting a new feature

The using-jj-workspaces skill says "jj new before jj workspace add for a
clean shared base." Tighten that: **the base must be `main`**, not the
current `@` (which may be a previous feature still in the working copy):

```bash
# WRONG — inherits the previous feature's @ as the new base:
jj workspace add ../resinsim-N-newfeature

# WRONG — uses jj new from current @, which may not be main:
jj new
jj workspace add ../resinsim-N-newfeature

# RIGHT — explicitly base on main:
jj new main
jj workspace add ../resinsim-N-newfeature
```

The `jj new main` ensures the new workspace's `@` descends directly from
`main`, regardless of where the default workspace's `@` was sitting. Even
if the previous lifecycle forgot to advance `main` (the bug below), this
catches the divergence at the new-feature boundary.

### Completing a feature

At Phase 5 → `resolved` (or after harvest → `complete`), **always advance
`main` before forgetting the workspace**:

```bash
# When the lifecycle is at `complete` (or `resolved` if skipping harvest):
jj bookmark set main -r @
# If main was on a divergent line and refuses, override:
jj bookmark set main -r @ --allow-backwards
```

If you forget, the next `jj new main` for the following feature will
branch from STALE main, recreating the divergence problem. The
issue-lifecycle skill's complete handler should run this.

### When divergence is unavoidable

Two cases legitimately need parallel heads, briefly:

1. **Hotfix on stable while a feature is in flight.** Branch the hotfix
   from `main` directly (`jj new main`), ship it, advance `main`, then
   rebase the in-flight feature onto the new `main`.
2. **Spike or experiment that may be abandoned.** Tag with
   `experimental:` prefix in the bookmark name and don't promote to
   `main` until the spike clears.

For everything else: linear stack, advance main on completion.

## Build + verification commands

- `cargo build --workspace` — fast sanity-check
- `cargo nextest run --workspace` — full test suite (pinned via memory: always `cargo nextest run`, never `cargo test`)
- `cargo fmt --all`
- `cargo clippy -p resinsim-core -p resinsim-inspect --all-targets -- -D warnings` — clippy clean on core + inspect (resinsim-viz has pre-existing warnings unrelated to issue 15; not blocking)

### Cargo feature matrix (ADR-0017, t2f1)

Crates carrying optional Cargo features (currently `field-sim`, forwarded
through resinsim-inspect and resinsim-viz from resinsim-core) require
**all four** configurations to pass before `review_code`:

1. `cargo build --workspace` — default features only; voxel modules must
   not be compiled
2. `cargo build --workspace --features resinsim-inspect/field-sim,resinsim-viz/field-sim`
   — feature-on build compiles; ndarray dep resolves; voxel modules
   compile
3. `cargo nextest run --workspace` — default tests pass; Tier-1 scalar
   path untouched
4. `cargo nextest run --workspace --features resinsim-inspect/field-sim,resinsim-viz/field-sim`
   — feature-on tests pass; voxel path exercised

The canonical failure mode for Cargo feature flags is "feature-off
build silently regressed because a `#[cfg(feature = \"…\")]` was forgotten";
configs (1) and (3) catch this and (2) and (4) catch the inverse.

Tests must pass before `review_code`.

### Fifth and sixth commands: the cucumber UAT suite (both feature configs)

`cargo uat` (alias in `.cargo/config.toml` for
`cargo test --test uat_gherkin -p resinsim-core`) and
`cargo uat-field-sim` (alias for the same binary with `--features
field-sim`).

Neither runs under nextest and therefore neither is covered by any of the
four configs above. The binary is `harness = false`, which aborts nextest's
enumeration, so `.config/nextest.toml` excludes every `uat_*` target (see
`docs/patterns/cucumber-in-nextest-workspace.md`). That exclusion is
deliberate and pinned by `nextest_filter_sanity.rs` — but it means a green
four-config matrix says nothing about the UAT suite, which is how it sat red
on main for months, and — under `field-sim` specifically — how it sat red
for two months more after that (`uat-fixtures-fieldsim-adr0020-gap`).

Run BOTH whenever `spec/uat/*.md` or `tests/uat_steps/` changes. Their
guards (rewritten by `uat-unskip-campaign` increment 1, 2026-08-01 — see
that campaign pointer below for what changed and why):

- **(a), layer 1 (static):** every `spec/uat/*.md` file with NO step-def
  module at all must be named in `SPECS_WITHOUT_STEP_DEFS`
  (`uat_gherkin.rs`). Checked via `include_str!` against `uat_steps/mod.rs`'s
  `pub mod` declarations — deterministic, independent of cucumber actually
  running.
- **(a), layer 2 (runtime, the real fix):** `main()` now drives cucumber
  ONCE PER SYNTHESISED FEATURE FILE (not once over the whole tree), so
  `StatsWriter::skipped_steps()` scoped to a single run IS that spec's
  skipped-SCENARIO count (cucumber halts a scenario at its first undefined
  step, so "skipped steps" and "skipped scenarios" are the same number,
  per-spec). `SPECS_WITHOUT_STEP_DEFS` is therefore `&[(&str,
  expected_skipped_count)]`, not just spec names, and fails in THREE
  directions: an unregistered spec with actual skips (new debt smuggled
  in, or drift re-appearing in an already-stepped spec — this is what
  layer 1 structurally cannot see: a spec CAN have a module and still lose
  scenarios if a step regex drifts from edited spec text); a registered
  spec whose actual skips dropped to zero (stale entry — remove it); a
  registered spec whose actual count differs from its expected count
  (partial progress not reflected). The debt register is still meant to
  shrink, with ONE amendment: an entry may be ADDED when it names a
  blocking issue rather than "nobody wrote the step yet" (see
  `cli-temperature-flag-validation`'s entry in `uat_gherkin.rs` for the
  live example — one scenario stays declared debt against filed issue
  `kb153-warning-missing-from-resinsim-sim`). Net scenario-debt (the sum of
  every registered count) still monotonically shrinks; a blocking-issue
  entry demotes a skip from silent to named and tracked, it does not hide
  it.
- **(a), layer 3 (structural, MUST-DECIDE-2):** the `pub mod` set in
  `uat_steps/mod.rs` and the `use uat_steps::{...}` set in `uat_gherkin.rs`
  (which forces every module to link so its `#[given]/#[when]/#[then]`
  registrations aren't silently dropped in an optimised build — see
  `-Aunused_imports` in `.cargo/config.toml`) must agree exactly. Also
  `include_str!`-checked.
- **(c)** zero parse errors, so no scenario is silently dropped.

Authoring-time detection of malformed Gherkin lives in
`tests/spec_gherkin_wellformed.rs`, which IS nextest-visible and so runs in
the four-config matrix.

**Expected shape is IDENTICAL in both configs** (current as of
`uat-unskip-campaign` increment 1, 2026-08-01): 52 features, 160 scenarios
(57 passed, 103 skipped, 0 failed), 395 steps (292 passed, 103 skipped, 0
failed), exit 0, register at 36 entries. This shape moves as the campaign
lands more increments — trust `cargo uat`'s own `[Consolidated total]` line
over this paragraph if they disagree, and update this paragraph when they
do. A field-sim run reporting FEWER total steps than the default run means
a scenario is aborting early (a fixture regressed and is panicking before
reaching every step) — treat that as a hard failure, not noise, even if the
final failed-count still reads 0.

**Campaign pointer** (`uat-unskip-campaign`, ratified 2026-07-28): recommended
bands, in order — Band A domain/default-features, Band B nanodlp, Band C
CLI (all DO); Band D field-sim-gated (DEFER, blocked on
`uat-fixtures-fieldsim-adr0020-gap`); Band E viz (SPLIT into its own issue,
`viz-uat-cucumber-harness` — a second cucumber harness hosted inside
resinsim-viz, since driving Bevy in-process from this harness would invert
ADR-0001's layering).

Hand-rolled resin/printer TOML fixtures under `tests/uat_steps/` MUST
compose from the shared builders (`world.rs::ResinBuilder`,
`world.rs::PrinterBuilder`, `world.rs::RecipeBuilder`) or the shared
fragments in `fixtures.rs` (`resin_chemistry_root[_pre_t2f4]`,
`valid_recipe_table`, `RESIN_FIELD_SIM_THERMAL_LINES`,
`PRINTER_FIELD_SIM_SCALARS`, `PRINTER_BUILD_ENVELOPE_INLINE`) rather than
hand-copying literals — see
`docs/patterns/anti/fixture-copy-of-shared-builder.md`. A copy that omits a
field required only under `field-sim` compiles, parses, and passes under
the other three ADR-0017 configs, and fails only when `cargo uat-field-sim`
runs.

## PR convention

Per project memory: PRs target `dev`, not `main`. `main` is reserved for
stable releases. The linear-history rule above governs `main` topology;
`dev` topology is governed by your usual git/jj remote workflow.

## Acceptance gate hand-off

The issue-lifecycle skill is the source of truth for state transitions.
This file does not override the sacred rules:

- Never auto-call `approve_plan`, `resolve_findings`, or `complete`
  without an explicit human trigger phrase.
- Run `tessl__review-*` skills inline, not as subagents (per project
  memory `feedback_inline_reviews.md`).
- For resinsim/, jj commits stay inside the resinsim/ workspace tree;
  ora-root changes are curated by Mag (per `feedback_no_ora_commits.md`).

## See also

- `agent-constraints/uat-conventions.md` — UAT format + location
- `agent-constraints/knowledge-base.md` — KB layout (docs/patterns/, docs/patterns/anti/, docs/adr/)
- `agent-constraints/iteration-limits.md` — autonomous loop caps
- `using-jj-workspaces` skill — sibling-workspace mechanics
- ADR-0015 (this issue) — example of a clean linear-history feature commit
