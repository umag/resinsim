---
date: 2026-04-22
issue: uat-gherkin-runner
---

# Spike Notes: BDD UAT runner with cucumber-rs

## Context

`spec/uat/*.md` (13 files, 35 scenarios) have always been written in
prose-style Gherkin under `**Scenario:**` headings, but no automated runner
parses or executes them. This spike asked: can we adopt cucumber-rs as the
runner without disrupting the existing Rust nightly + nextest workflow?

## Setup

- Crate: `cucumber = { version = "0.22.1", features = ["libtest"] }`
- Async runtime: `tokio = { version = "1", features = ["macros", "rt"] }` (dev-deps)
- Two `.feature` files hand-copied from source markdown, matching kebab-case
  filenames (per project convention):
  - `crates/resinsim-core/tests/uat/safety-factor-zero-force.feature`
  - `crates/resinsim-core/tests/uat/cure-depth-nan-guard.feature`
- Test target: `crates/resinsim-core/tests/uat_gherkin.rs` with
  `harness = false` and a `#[tokio::main(flavor = "current_thread")]` entry
- Step bodies use existing public APIs only:
  `Energy::new`, `Energy::scale`, `PeelForce::new`, `SupportCapacity::new`,
  `SafetyFactor::compute`. `Energy::scale` confirmed `pub` at
  `src/values/cure_depth.rs:59`.

## Outcomes

### (a) Compile + run on nightly

✅ Clean. cucumber 0.22.1 + gherkin 0.15.0 + cucumber-codegen 0.22.1 compile
under the workspace's nightly toolchain (with `-Z threads=8` rustflags) with
no warnings or workarounds. Initial build added ~20 transitive crates
(synthez, derive_more, futures, etc.) but build time impact is negligible
on incremental rebuilds.

### (b) Test execution

✅ All 12 steps across 3 scenarios pass. Output (cucumber's default Basic
writer):

```
Feature: Cure depth NaN guard
  Scenario: Invalid critical energy is caught before cure depth calculation
   ✔  Given a resin profile with a critical energy Ec that is zero or non-finite
   ✔  When the Beer-Lambert cure depth calculator runs for a layer
   ...
[Summary]
2 features
3 scenarios (3 passed)
12 steps (12 passed)
```

### (c) Runner choice: `cargo test`, NOT `cargo nextest`

❌ cucumber-rs binaries are **incompatible with `cargo nextest`**.

Why: nextest enumerates each test binary via libtest's *terse listing*
protocol — the binary is invoked with `--list --format terse` and must
produce one line per test in the form `name: test`. cucumber's writers do
not speak this protocol:

- The default Basic writer prints human-readable feature/scenario tree.
- The `libtest`-feature writer (`cucumber::writer::Libtest`) emits libtest's
  *streaming JSON* dialect (`{"type":"test","event":"started",...}`),
  which is libtest's `--format json` output — a *different* protocol from
  the terse listing nextest uses.

We tried both writers; neither lets nextest enumerate. The
`harness = false` test target then refuses nextest's `--list`/`--exact`
arguments via clap, aborting the whole `cargo nextest run -p resinsim-core`
suite.

**Resolution:** added `.config/nextest.toml` with
`default-filter = "not binary(uat_gherkin)"` so the full nextest suite
keeps working (skips the cucumber binary). Cucumber UAT scenarios run via
`cargo test --test uat_gherkin -p resinsim-core` instead.

This is a **deliberate exception to the `cargo nextest` convention**
(`feedback_use_nextest` in user memory). The convention still holds for
every other test target in the workspace; only the cucumber binary is
exempted, and the exemption is documented in `.config/nextest.toml`.

> **Scope note.** Adding `.config/nextest.toml` was not part of the
> approved spike plan (v3 step S.4 specified "capture nextest output and
> document the per-binary attribution outcome"). It was added to preserve
> spike acceptance criterion 4 ("Existing test suite stays green:
> `cargo nextest run -p resinsim-core`") which would otherwise have failed
> outright once the harness=false binary was introduced. Recorded as
> in-spike scope expansion rather than a separate change.

### (d) Clippy compliance

`cargo clippy -p resinsim-core --tests -- -D warnings` surfaces 5 errors,
**all in pre-existing `services/cavity_detector.rs` code**, none introduced
by this spike. The spike's `tests/uat_gherkin.rs` uses `.expect()` with
descriptive scenario-invariant messages throughout (matching the pattern at
`tests/cure_properties.rs`); no `.unwrap()` calls. ADR-0003
(`unwrap_used = deny`) is satisfied.

### (e) Drift between `.md` and `.feature`

Acknowledged and **deferred**. The two `.feature` files are byte-copies
from their source `.md` files but there is no mechanism preventing
divergence. Each `.feature` carries a `# Source: spec/uat/<file>.md` header
to preserve traceability. A rollout phase would either:
1. Adopt a build-time extractor (the original v1 plan), or
2. Migrate `.md` files to use ` ```gherkin ` fenced code blocks (Dragonfruit-kb
   style) and have the runner read from `.md` directly.

## Recommendation

**Proceed to a rollout** (separate lifecycle issue:
`uat-gherkin-runner-rollout`). Rationale:

1. cucumber-rs works cleanly under the workspace toolchain — no nightly
   incompatibility, no exotic build configuration.
2. Per-binary nextest attribution (one entry covering all scenarios) is
   acceptable for CI failure triage — cucumber's text output points to the
   exact failed scenario inside the binary's stderr/stdout, which CI
   surfaces.
3. Step bodies are tractable. Compound Givens (multi-attribute printer
   profiles in scenarios like `recipe-outside-printer-range`) will need a
   builder pattern but the public-API surface is rich enough.

The rollout issue should address (in priority order):
- Drift detection between `.md` and `.feature` (item (e) above).
- Compound-Given step builders (PrinterBuilder, ResinBuilder).
- Coverage guard: catch UAT files added without step coverage.
- CI wiring (resinsim has no `.github/workflows/` today).
- Decision on whether to keep two `.feature` files or migrate the markdown
  format and read scenarios directly from `.md`.
- **Bind step assertions to typed error discriminants, not message
  substrings.** The spike asserts panic and error messages by `contains(...)`
  on literal text duplicated from the source code (`Energy::new`'s "energy
  must be positive and finite", `Energy::scale`'s "scale factor must be
  positive and finite"). The .feature scenarios contain the same literal
  strings. A legitimate refactor of either message wording would break both
  the .feature and the test in lockstep, with no signal pointing at the
  contract change. When domain errors gain typed variants, rebind step
  assertions to the variant.
- **End-to-end mirror enforcement.** `then_safety_factor_infinity`
  re-implements the production formula
  `safety.map_or(f32::INFINITY, |s| s.value())` from
  `services/failure_predictor.rs:306` instead of invoking `predict_layer`.
  The test asserts a tautology about its own helper; if the production
  formula changes to return `Some(0.0)` or `Option<f32>`, the test still
  passes. The mirror is documented (the test file points at the production
  line) but not enforced. Rollout should add an integration call to
  `predict_layer` with a zero-area layer fixture and assert
  `LayerResult.safety_factor.is_infinite()` directly.
- **Nextest filter brittleness.** `.config/nextest.toml` uses
  `default-filter = "not binary(uat_gherkin)"` — exact-name match. Renaming
  the cucumber test target silently breaks the filter and re-breaks the
  full suite. Rollout should either widen to a name pattern (e.g.
  `not binary(/^uat_/)`) or add a CI check that asserts the filter still
  matches at least one binary.

## Touched files

- `crates/resinsim-core/Cargo.toml` — cucumber + tokio dev-deps + harness=false test target
- `crates/resinsim-core/tests/uat_gherkin.rs` — harness, World, 7 step defs
- `crates/resinsim-core/tests/uat/safety-factor-zero-force.feature` — copied scenario
- `crates/resinsim-core/tests/uat/cure-depth-nan-guard.feature` — copied scenarios
- `.config/nextest.toml` — new file, excludes uat_gherkin binary from nextest
- `docs/adr/0008-bdd-uat-spike-notes.md` — this document
