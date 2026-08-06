---
issue: viz-uat-cucumber-harness
date: 2026-08-06
---

# ADR-0024: A second cucumber UAT harness, hosted inside resinsim-viz, scoped to spec/uat/viz-*.md

## Status

Accepted (viz-uat-cucumber-harness, plan v1, 2026-08-06).

## Context

`spec/uat/*.md` is the single source of truth for UAT scenarios across
the whole workspace, driven by ONE cucumber harness:
`crates/resinsim-core/tests/uat_gherkin.rs`, with step defs under
`crates/resinsim-core/tests/uat_steps/`. The `uat-unskip-campaign`
(ratified 2026-07-28) split its recommended bands into A (domain), B
(nanodlp), C (CLI), D (field-sim-gated, deferred) and E (viz, split into
this issue) — because driving Bevy from inside resinsim-core's harness
would invert ADR-0001's layering, and `env!("CARGO_BIN_EXE_resinsim-viz")`
is not available to a test binary hosted in a different package.

### The blocker is real, but only for one of three reasons — state all three

It would be easy to write this ADR around a single "dev-dep cycle" claim
and stop there, but that claim alone does not survive scrutiny:
`crates/resinsim-core/tests/uat_steps/cli_fixtures.rs` already
subprocesses the `resinsim` binary (a DIFFERENT package,
`resinsim-inspect`) from INSIDE resinsim-core's harness with **no
dev-dependency on resinsim-inspect at all** — it builds the binary via a
`cargo build` subprocess and locates it by walking up from
`current_exe()`. A core-hosted, SUBPROCESS-ONLY viz harness could have
used the exact same trick and would not have needed a `resinsim-core →
resinsim-viz` dev-dependency cycle either. The honest case rests on
THREE legs, not one:

1. **The dev-dependency cycle is real only for IN-PROCESS driving.**
   `resinsim-viz` already depends on `resinsim-core` in production
   (`crates/resinsim-viz/Cargo.toml`). Driving Bevy in-process from a
   resinsim-core-hosted harness would require resinsim-core to
   dev-depend on resinsim-viz, creating `resinsim-core ↔ resinsim-viz`.
   This project chose NOT to drive Bevy in-process at all — every viz
   UAT scenario piloted here is a subprocess CLI-surface check — so this
   leg is not actually why the harness had to move. It is included
   because ruling it out is itself informative: it closes off "just add
   a dev-dep and drive Bevy directly" as a live option for future viz
   UAT increments that DO need World-state assertions.
2. **A core-hosted subprocess-only harness would impose a display/GPU
   requirement on `cargo uat`, which is display-free today.** Tier B of
   this pilot (UAT-1/3/4/9) opens a real window for a few seconds each.
   `cargo uat` and `cargo uat-field-sim` currently run with no GPU, no
   window server, no `DISPLAY` — hosting viz scenarios there would make
   the CORE suite's developer-machine requirements depend on the
   PRESENTATION layer's runtime needs, backwards from ADR-0001's
   direction.
3. **The positive reason: `env!("CARGO_BIN_EXE_resinsim-viz")` is
   available for free only inside resinsim-viz's own build graph.**
   Hosting the harness there means cargo guarantees the binary is built
   and fresh before the test runs, with zero extra apparatus — the same
   mechanism resinsim-inspect's own CLI tests already use
   (`field_inspect_cli.rs`, `report_health_time_cli.rs`,
   `thermal_cli_warnings.rs`). This is what actually motivates the
   crate-scale move, not the cycle.

## Decision

### A second harness, not an extension of the first

`crates/resinsim-viz/tests/uat_viz_gherkin.rs` — a `harness = false`
cucumber-rs binary, `cargo uat-viz` alias
(`test --test uat_viz_gherkin -p resinsim-viz`), same design as core's
harness (per-feature `.run()` for per-spec attribution, silent-green
guard per-feature and in aggregate, parse-error guard, three-direction
register check) but its OWN register,
`crates/resinsim-viz/tests/uat_viz_gherkin.rs::SPECS_WITHOUT_STEP_DEFS`,
SCOPED to `spec/uat/viz-*.md` only.

`crates/resinsim-core/tests/uat_gherkin.rs`,
`crates/resinsim-core/tests/uat_steps/`, and that harness's
`SPECS_WITHOUT_STEP_DEFS` are **not touched** by this change and
currently still carry all 12 `viz-*` entries as debt, summing 30. This
is a DELIBERATE, TEMPORARY double count — see "Migration plan" below.

### Extractor reuse: cross-crate `#[path]` include, not a copy, not a shared crate

`uat_viz_gherkin.rs` includes `resinsim-core`'s
`tests/uat_steps/extract.rs` verbatim via
`#[path = "../../resinsim-core/tests/uat_steps/extract.rs"] mod extract;`
— the SAME mechanism `tests/spec_gherkin_wellformed.rs` already uses
(`#[path = "uat_steps/extract.rs"] mod extract;`), making this the
THIRD `#[path]` consumer, not a new architectural move. `extract.rs`'s
own module doc already documents itself as "Compiled into TWO binaries"
and explicitly forbids `super::`/`crate::` paths so it stays importable
from anywhere — a third consumer strengthens that existing, enforced
self-containment rather than introducing a new risk. `extract.rs` has
exactly two dependencies (`pulldown_cmark`, `std`); `resinsim-viz`'s
`Cargo.toml` gained a matching `pulldown-cmark = "0.13.3"` dev-dep,
version-pinned to resinsim-core's.

**Rejected — duplicate `extract.rs` into resinsim-viz.** Creates exactly
the drift surface `docs/patterns/anti/fixture-copy-of-shared-builder.md`
was harvested to prevent: a copy that omits a field (or fixes a bug in
one but not the other) compiles and passes under three of four configs.
Rejected outright.

**Rejected — promote `extract.rs` to a shared workspace crate
(`resinsim-uat-extract`).** Heavyweight relative to what it buys: a new
workspace member, `Cargo.toml`, `[lints] workspace = true` entry, an
ADR-0001 crate-map entry, and a test-only 263-line helper promoted to a
crate-shaped public API with semver-ish expectations. It buys nothing
over `#[path]` — both give one source of truth, both are rebuild-tracked
(rustc emits `#[path]`-included files into the dep-info `.d` file, so
cargo rebuilds `uat_viz_gherkin` when `extract.rs` changes — no
stale-copy window). **Revisit trigger**, so this rejection has an
expiry rather than being permanent by default: promote to a crate if a
THIRD consumer appears OUTSIDE the two test trees (a build script or a
production binary), if `extract.rs` acquires a dependency beyond
`pulldown-cmark`, or if the `#[path]` string appears in more than three
files.

### E1/E2 split and this pilot's tier A/B boundary

The campaign's E1/E2 labelling is per-SPEC. A per-SCENARIO read at plan
time found it does not fully survive: `viz-allow-mismatch-soft-fallback`
and `viz-load-ctb-with-sim-renders-heatmap` each mix subprocess-observable
stderr `Then`s with World-state `Then`s (`Mesh::ATTRIBUTE_COLOR`,
`LayerCursor` entity presence, window title) — genuinely HYBRID, not
pure E1. Neither is piloted here; both stay fully skipped, registered as
before.

This pilot covers exactly one spec, `viz-screenshot-flag`, chosen
because it is the ONLY spec that clears
`docs/patterns/anti/bevy-subprocess-smoke-test.md`'s bar — every piloted
scenario asserts EXACTLY an exit code, a stderr substring, and file
presence/absence, never World state. Tier A (`UAT-7a/7b/7c`) is
renderer-free: `--screenshot` path validation calls
`std::process::exit(EXIT_SCREENSHOT_BAD_PATH)` at main.rs BEFORE
`App::new()` — no window, no wgpu, no `LogPlugin`. Tier B
(`UAT-1/3/4/9`) needs a live renderer: every one of exits 2/3/4/6 is
queued by `fatal_exit` from inside the `setup_initial_load` Startup
system, which only runs once `DefaultPlugins` has brought up windowing
and wgpu; exit 0 additionally requires an actual frame to render.

**The renderer constraint is a fact about THIS machine class, recorded,
not assumed.** Step 1's empirical probe (six invocations run by hand
against the built binary, one at a time) found a working discrete GPU
(AMD Radeon Pro 5500M, Metal backend) on the implementer's machine —
`--screenshot <path>` returned exit 0, wrote a 206 KB PNG, and stderr
contained the exact `Screenshot saved to ` grep-contract string the
spec pins. Tier B therefore lands. Nothing in this repo's existing test
suite could have told us this in advance: every existing resinsim-viz
test either uses a plugin-less `App::new()` (the bevy-app-test-seam
pattern) or reads a `sim.json` fixture with no Bevy involved at all —
"the viz tests pass in nextest here" is NOT evidence the binary can
render, and reading it as such would have been exactly the
`web-search-version-compat-without-canonical-verification` failure mode
this plan was written to avoid. Had the probe returned exit 8
(`EXIT_SCREENSHOT_RENDER_TIMEOUT`) or a spawn failure, tier B would have
dropped to declared debt and this ADR would record a machine-class
constraint instead. `cargo uat-viz` opening real windows is therefore a
**developer-machine gate**, not a CI gate — there is no CI in this repo
today (`ls .github` → absent), so this imposes no CI cost, but a future
CI addition would need to either skip `cargo uat-viz` or run it on a
GPU-backed runner.

### A finding the plan did not anticipate: UAT-7d is not reachable via CLI

Tier A was expected to be 4 scenarios (`UAT-7a/7b/7c/7d`). Step 1's probe
found otherwise: passing an empty string to `--screenshot` — via
`--screenshot ""` (direct subprocess argv, bypassing shell-quoting
ambiguity) or the unambiguous `--screenshot=` form — is rejected by
**clap 4.6.1's own argument parser** (exit 2, `"a value is required for
'--screenshot <PATH.png>' but none was supplied"`) before `main()`'s own
`validate_screenshot_path` / `PathError::Empty` (screenshot.rs) ever
runs. Confirmed with an isolated minimal repro (a separate scratch
crate, `clap = "=4.6.1"`, the identical
`#[arg(long, value_name = "PATH.png")] screenshot: Option<PathBuf>`
field) — same result. `PathError::Empty`'s branch is not dead in the
sense of being unreachable in-process (it has its own unit test,
`validate_rejects_empty_path`, screenshot.rs), but it IS unreachable via
any CLI invocation of the binary, because clap intercepts the empty
value first.

Per the standing rule that a spec/harness mismatch is resolved on the
HARNESS side — never by editing the spec, never by weakening an
assertion to accept whatever code actually fires — UAT-7d stays declared
debt, covered by the pre-existing unit test, the same pattern the spec's
own "Test coverage notes" section already uses for exit 7/8. Committed
tier-A+B scope is therefore **7 scenarios** (UAT-7a/7b/7c + UAT-1/3/4/9),
not 8; `viz-screenshot-flag`'s register entry is 5 (12 − 7), not 4. This
is not a design choice — it is what the probe evidence supports, using
the SAME declared-debt mechanism the plan itself prescribes for UAT-3's
resolution (below), applied to a scenario the plan did not anticipate
would be unreachable.

### UAT-3's exit-code race, resolved empirically

The spec's `UAT-3` says `--load-sim foo.sim.json --screenshot ...`
exits 4 (`EXIT_BAD_SIM_PAIRING`). `foo.sim.json` does not exist, and
`setup_initial_load` attempts the sim load FIRST — a nonexistent path
queues `EXIT_SIM_LOAD_FAILED=2` before the bad-pairing check queues
`EXIT_BAD_SIM_PAIRING=4`, and step 1's probe confirmed exit 2 wins that
race (even though the bad-pairing stderr line is still logged). The step
def substitutes the checked-in valid fixture
`crates/resinsim-viz/tests/fixtures/lilith-torso.sim.json` for
`foo.sim.json`, so the sim loads successfully and only the pairing
failure is in play — confirmed empirically (exit 4, correct stderr, no
PNG written). This is faithful to the scenario's intent ("any
`.sim.json`", not a specific nonexistent one); it is neither a spec edit
nor a weakened assertion.

### Amendment to `docs/patterns/anti/bevy-subprocess-smoke-test.md`

That doc states the right home for CLI-shape verification is
"the `resinsim-inspect` package's `cli_fixtures.rs` ... not the viz
crate" — written for issue 01, before `resinsim-viz` had any CLI
contract worth verifying. ADR-0013 later gave it one (the 8-code
`--screenshot` exit-code surface). The exception the doc already
carves out — "When subprocess tests ARE appropriate: when the thing
under test IS the binary's CLI surface" — now applies to the viz binary
too. A short "Update" section is added to that doc in this same change
(see the doc itself) so a reader is not left with the stale claim.

## Consequences

### Good

- `cargo uat-viz` closes a real regression gap: `anti/bevy-app-exit-return-discarded.md`
  records that `main()` discarding `app.run()`'s return once made every
  exit code silently 0, caught only by manual verification. Tier B's
  four exit codes (0, 4, 0, 6) are now an automated, permanent guard
  against that exact regression class.
- One register entry (`viz-screenshot-flag`) covers the single largest
  viz UAT debt item — 40% of viz debt, 16% of the whole workspace's
  remaining scenario debt sat in this one file before this change.
- The extractor-reuse mechanism (`#[path]`) and the harness DESIGN are
  both proven reusable a third and second time respectively, ahead of
  any future viz UAT increment needing them.

### Bad

- `resinsim-viz`'s dev-dependency graph gains `cucumber`, `tokio`, and
  `pulldown-cmark`, lengthening `cargo nextest run -p resinsim-viz`
  build time (these compile for its dev graph even though the new test
  binary itself is nextest-excluded). This is a build-time cost, not a
  test-count change — do not read a slower `-p resinsim-viz` build as a
  regression.
- `cargo uat-viz` opens real windows for a few seconds per tier-B
  scenario, stealing focus on macOS and making the run visually
  disruptive. Documented in the alias comment (`.cargo/config.toml`),
  the README, and here.
- A transitional double count: the 7 now-piloted scenarios are counted
  as passing debt-paid-down in `cargo uat-viz`'s register AND still
  counted as skipped debt in core's `SPECS_WITHOUT_STEP_DEFS`. See
  "Migration plan".
- `Screenshot saved to ` is Bevy's own `LogPlugin`/`bevy_render` log
  line, not resinsim code — the spec pins it as an explicit grep
  contract with a `bevy_render` source reference specifically because a
  future Bevy upgrade could move it off stderr; a step-def comment cites
  that contract so a future break reads as "Bevy changed its logging",
  not "resinsim regressed".

### Neutral

- `docs/adr/0001-ddd-layer-dependency-rule.md` governs resinsim-core's
  INTERNAL Values/Entities/Services layering only. This ADR EXTENDS that
  direction to CRATE scale (core must not depend on viz, even at
  dev-dependency granularity, for in-process driving) rather than citing
  ADR-0001 as already saying so — the extension is claimed explicitly,
  here, not assumed.
- The new harness's register is SCOPED (`spec/uat/viz-*.md` only),
  diverging from core's whole-directory register. This makes the `viz-`
  stem prefix a load-bearing OWNERSHIP BOUNDARY between the two
  registers, not merely a naming convention — enforced by a fourth guard
  core has no equivalent for: the harness's spec set must equal exactly
  the on-disk `viz-*.md` set, so a future non-`viz-`-prefixed
  presentation spec cannot silently fall through both harnesses.

## Migration plan

Core keeps all 12 `viz-*` `SPECS_WITHOUT_STEP_DEFS` entries, at their
current counts, for the life of this change — this is a hard scope
invariant, not an oversight. Removing them requires proving the viz
harness covers them, which cannot be proven until the viz harness has
run green at least once (this change ships it, but does not yet lean on
that proof). The removal is a FOLLOW-UP LIFECYCLE,
`viz-uat-register-migration`, filed at harvest, which removes core's
entries ONE SPEC AT A TIME as the viz harness steps them — starting with
`viz-screenshot-flag`'s entry, the only one with any coverage today.
Attempting the removal in THIS change would fire core's direction-2
("registered spec now has zero actual skips") guard against a register
whose replacement is unproven, taking both suites red simultaneously
with no way to bisect which harness caused it.

A second, separate follow-up covers the fixture blocker: UAT-2/5/8 and
three fixture-blocked specs (`viz-layer-count-mismatch-hard-error`,
`viz-allow-mismatch-soft-fallback`, `viz-load-ctb-with-sim-renders-heatmap`)
cannot move until the repo has a deterministic `.ctb` —
`docs/patterns/synthesise-archive-fixture-not-committed-binary.md`
suggests synthesising one rather than committing a binary fixture, which
is a real design question deserving its own issue.

## Alternatives considered

### Host the harness in resinsim-core, subprocess-only (no dev-dep cycle)

Genuinely possible — `cli_fixtures.rs` proves the pattern works with no
dev-dependency. Rejected on the second and third legs above: it would
impose tier B's display/GPU requirement on `cargo uat`, which is
display-free today, and it would forgo the free, guaranteed-fresh
`env!("CARGO_BIN_EXE_resinsim-viz")` resinsim-viz gets for nothing.

### Drive Bevy in-process from a resinsim-core-hosted harness

Would invert ADR-0001's layering (resinsim-core dev-depending on
resinsim-viz) and would let the CORE suite assert on PRESENTATION
behaviour — backwards from the intended knowledge direction. Not
attempted; every piloted scenario here is subprocess-only by design (see
the anti-pattern doc).

### Pilot the load/mismatch/pairing bundle instead of viz-screenshot-flag

Rejected: fixture-blocked at the root (`find . -name '*.ctb'` returns
zero hits workspace-wide), would require either committing a binary
fixture or env-conditional step behaviour (the exact Band-D shape a
`const` register cannot express), and would spread across 4 register
entries for 4 scenarios instead of one entry for the pilot's whole
value.

## See also

- `crates/resinsim-viz/tests/uat_viz_gherkin.rs`
- `crates/resinsim-viz/tests/uat_viz_steps/viz_cli.rs`
- `crates/resinsim-viz/tests/uat_viz_steps/viz_screenshot_flag.rs`
- `crates/resinsim-core/tests/uat_gherkin.rs` `SPECS_WITHOUT_STEP_DEFS`
  (unchanged by this ADR; the migration target)
- `spec/uat/viz-screenshot-flag.md`
- `docs/adr/0001-ddd-layer-dependency-rule.md` (the direction this ADR
  extends to crate scale)
- `docs/adr/0013-screenshot-exit-code-disjunction.md` (the exit-code
  contract tier B guards)
- `docs/patterns/second-cucumber-harness-for-a-presentation-crate.md`
- `docs/patterns/anti/bevy-subprocess-smoke-test.md` (amended by this
  change)
- `docs/patterns/anti/bevy-app-exit-return-discarded.md` (the regression
  tier B now guards)
- `docs/patterns/anti/fixture-copy-of-shared-builder.md` (why
  `extract.rs` is included, not copied)
- `docs/patterns/synthesise-archive-fixture-not-committed-binary.md`
  (the fixture-blocker follow-up)
- `agent-constraints/uat-conventions.md`, `agent-constraints/implementation-conventions.md`
