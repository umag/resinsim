---
issue: viz-uat-cucumber-harness
date: 2026-08-06
---

# Pattern: a second cucumber harness for a presentation crate

## Context

A workspace already has one cucumber-rs UAT harness hosted in its
innermost/domain crate (`cucumber-in-nextest-workspace.md`), reading
`spec/uat/*.md`. A PRESENTATION crate downstream of it (a GUI, a
renderer, a CLI-with-a-window) accumulates its own UAT specs, and those
specs' CLI surface is worth automated coverage — but hosting them in the
existing harness is wrong for reasons that are easy to state
incompletely. See `docs/adr/0024-second-uat-harness-in-resinsim-viz.md`
for the full worked example this pattern is extracted from.

## The three-leg test for "does this need a SECOND harness"

Check all three before deciding a second harness is warranted. Any one
missing weakens the case; state which legs apply and which don't rather
than citing "avoids a dependency cycle" alone — that claim alone often
does not survive scrutiny, because a SUBPROCESS-ONLY step def (never an
in-process one) usually does not need a dev-dependency on the
presentation crate at all (a `cargo build` subprocess + binary-path walk
works with zero dev-dep, same as this workspace's
`cli_fixtures.rs::ensure_resinsim_built` already proves for a
CROSS-PACKAGE subprocess).

1. **In-process driving would need a dependency cycle.** Only applies if
   the harness plans to construct the presentation layer's types
   in-process (e.g. spin up a GUI framework's `App` and inspect World
   state) rather than subprocess the built binary. If every planned
   scenario is subprocess-only, this leg does not apply — say so, don't
   claim it anyway.
2. **A shared harness would impose the presentation layer's runtime
   requirements (display, GPU, window server) on the domain suite,
   which is typically requirement-free.** This is usually the leg that
   actually matters. Check what the domain suite's runtime shape is
   TODAY (headless? CI-safe?) and state explicitly that hosting
   presentation scenarios there would change it.
3. **The positive reason: `env!("CARGO_BIN_EXE_<bin>")` is available for
   free only inside the binary's OWN package.** A harness hosted
   elsewhere needs a `cargo build` subprocess + `current_exe()` walk (or
   equivalent) to locate the binary; hosted in the binary's own package,
   cargo guarantees the binary is built and fresh before the test runs,
   with zero extra apparatus.

If only leg 3 applies (no cycle risk, no runtime-requirement conflict),
a second harness may still be the right call for build-graph simplicity
alone — but say so explicitly rather than reaching for the cycle
argument by default.

## The shape

1. **New `[[test]] harness = false` target in the presentation crate's
   own `Cargo.toml`**, plus a matching `cargo <alias>` entry in
   `.cargo/config.toml` pointing at it (`test --test <target> -p
   <crate>`). `.config/nextest.toml`'s existing `not binary(/^uat_/)`
   pattern (or equivalent) covers the new binary with ZERO config
   change if the new target's name also starts with the same prefix —
   confirm this with a cheap `cargo nextest list -p <crate>` sanity
   check rather than assuming.
2. **Reuse the extractor via cross-crate `#[path]`, do not copy it, do
   not promote it to a shared crate by default.** If the domain crate's
   extractor module already documents itself as multi-binary (already
   `#[path]`-included by an authoring-time guard target) and forbids
   `super::`/`crate::` paths, a third `#[path]` consumer is the same
   already-ratified move, not new architecture. Promote to a shared
   crate only past a stated REVISIT TRIGGER (a third consumer OUTSIDE
   the two original test trees; a new dependency; the `#[path]` string
   appearing in more than N files) — write the trigger down so the
   rejection has an expiry.
3. **Reuse the harness DESIGN verbatim** (per-feature `.run()` for
   per-spec attribution, silent-green guard per-feature AND in
   aggregate, parse-error guard, the three-direction register check:
   unexpected skip / stale entry / count mismatch) but give the second
   harness its OWN register, SCOPED to a spec-name prefix the two
   harnesses agree on (e.g. `viz-*.md`). Add a guard the first harness
   does not need: the second harness's spec set must equal EXACTLY the
   on-disk prefix-filtered set, so a new spec authored by a concurrent
   lane cannot silently fall through both harnesses' registers. The
   prefix becomes a load-bearing OWNERSHIP BOUNDARY, not a convention —
   say so in the ADR.
4. **Do NOT reuse the domain crate's World, builders, or CLI-fixture
   apparatus wholesale.** Domain builders are typically irrelevant to a
   CLI-surface harness. The cross-package CLI-fixture apparatus
   (`ensure_<bin>_built`, a `current_exe()` walk) is specifically NOT
   needed once the harness lives inside the binary's own package —
   `env!("CARGO_BIN_EXE_<bin>")` replaces the whole thing. Write a
   small, local `<bin>_cli.rs` (`CliOutcome` + `invoke_<bin>`) instead;
   expect it to be an order of magnitude smaller than the cross-package
   version.
5. **If any piloted scenario needs a live GPU/display/renderer, gate it
   behind a PREFLIGHT that PANICS, never skips, on failure.** A silent
   skip is a silent green; the whole point of a UAT harness is to
   fail loudly. Use the failure code/signal as a DISCRIMINATOR (e.g., a
   render-timeout exit code distinct from other failure exit codes),
   never as something the harness quietly absorbs. State the
   consequence plainly in the alias's doc comment: this makes the new
   alias a developer-machine gate, not (yet) a CI gate, if the workspace
   has no CI today.
6. **Land the register count change in the SAME commit as the step defs
   that justify it**, never a later "shrink" commit — a registered spec
   whose actual skip count drops to zero (or changes) fails the
   three-direction check on the very next run, so a two-step landing is
   not merely untidy, it is not executable.

## What NOT to migrate on day one

Leave the FIRST harness's register entries for the migrated specs in
place, at their current counts, even after the second harness starts
stepping them. This produces a deliberate, temporary DOUBLE COUNT (the
same scenarios counted as debt in both registers) — correct and
intended, because removing the first harness's entries requires PROVING
the second harness covers them, which cannot be proven until it has run
green at least once. Filing that removal as its own follow-up lifecycle,
one spec at a time, avoids taking both harnesses red simultaneously with
no way to bisect which one broke.

## Empirically resolve environmental branches ONCE, never at harness run time

If any scenario's outcome depends on the environment (does this machine
have a renderer? does clap in this version reject an edge-case value
before production code sees it?), resolve it via a real, empirical probe
against the BUILT BINARY before writing the harness, and hardcode the
result. A register whose expected counts depend on an environment
variable checked at harness run time is the exact defect a `const`
debt-register design cannot express — see the domain harness's own
documentation of this class of bug for the long version. Record the
probe's raw invocations and outputs verbatim wherever the decision is
made (a scratch note, a commit message, an ADR) so the decision did not
come from guessing.

## Related

- `docs/adr/0024-second-uat-harness-in-resinsim-viz.md` — the worked
  example this pattern generalises from
- `docs/patterns/cucumber-in-nextest-workspace.md` — the first harness's
  own pattern doc; this is its sibling, not its replacement
- `docs/patterns/anti/bevy-subprocess-smoke-test.md` — when a subprocess
  CLI-surface test is the RIGHT tool (as opposed to an in-process
  World-state test)
- `docs/patterns/anti/fixture-copy-of-shared-builder.md` — why the
  extractor is included via `#[path]`, not copied
- `docs/patterns/per-spec-runtime-skip-attribution.md` — the
  three-direction register check this pattern's harness reuses
- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`
  — why the spec-set-equality guard exists rather than trusting the
  prefix convention alone
