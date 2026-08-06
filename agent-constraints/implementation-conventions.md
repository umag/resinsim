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
- formatting — do NOT run `cargo fmt`; see ### Formatting below
- `cargo clippy -p resinsim-core -p resinsim-inspect --all-targets -- -D warnings` — clippy clean on core + inspect (resinsim-viz has pre-existing warnings unrelated to issue 15; not blocking)

### Formatting

**Do NOT run `cargo fmt --all` or `cargo fmt` against the live tree.** Two
problems stack: this machine's `PATH` resolves `rustfmt` to a stale Homebrew
1.6.0-stable that shadows rustup's nightly toolchain, and — the part that
actually matters — the tree has never been formatted tree-wide, so any
`cargo fmt --all` rewrites files unrelated to your change no matter which
binary runs it. A *correct* nightly binary rewrites MORE files than the
stale one, not fewer — roughly four times as many, order-of-magnitude,
measured 2026-08-02 over the tracked mod-free leaf files under `crates/`
via scratch `--check` runs. `c0256a1` had to rebuild ~23 unrelated fixture
files after a stray `cargo fmt --all`; `9532775` is the commit where that
run actually fired and shipped restyled unrelated workspace files inside a
calibration commit.

Instead, check formatting on a scratch copy and never write to the live
tree:

```sh
# Leaf file with no `mod x;` declarations of its own
SCRATCH=$(mktemp -d)
cp path/to/your_file.rs rustfmt.toml "$SCRATCH/"
cd "$SCRATCH"
rustup run nightly rustfmt --config-path rustfmt.toml --check your_file.rs
```

```sh
# Whole crate tree — required whenever the file has sibling `mod` decls;
# a single-file copy can't resolve them and the check silently misses hunks
SCRATCH=$(mktemp -d)
cp -R crates/<crate>/src "$SCRATCH/src"
cp rustfmt.toml "$SCRATCH/"
cd "$SCRATCH"
rustup run nightly rustfmt --config-path rustfmt.toml --check src/main.rs
```

Do not add `--unstable-features`; it is not required on nightly.

Two adjudication rules, from practice: **existing files** keep their own
local convention — fix only the hunks your change touched, leave
pre-existing drift alone. **New files** may adopt the full configured style
in `rustfmt.toml` end-to-end; several recent files already do.

A large diff from the check command is EXPECTED — the tree has never been
formatted — and is not a signal to run `cargo fmt --all`.

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
  blocking issue rather than "nobody wrote the step yet" — the worked,
  ratified example is `cli-temperature-flag-validation`'s brief entry
  against filed issue `kb153-warning-missing-from-resinsim-sim` (see the
  doc comment on `SPECS_WITHOUT_STEP_DEFS` in `uat_gherkin.rs`), since paid
  down and removed once the KB-153 single-emission-seam fix landed. Net
  scenario-debt (the sum of every registered count) still monotonically
  shrinks; a blocking-issue entry demotes a skip from silent to named and
  tracked, it does not hide it.
- **(a), layer 3 (structural, MUST-DECIDE-2):** the `pub mod` set in
  `uat_steps/mod.rs` and the `use uat_steps::{...}` set in `uat_gherkin.rs`
  (which forces every module to link so its `#[given]/#[when]/#[then]`
  registrations aren't silently dropped in an optimised build — see
  `-Aunused_imports` in `.cargo/config.toml`) must agree exactly. Also
  `include_str!`-checked.
- **(c)** zero parse errors, so no scenario is silently dropped.

Authoring-time detection of malformed Gherkin lives in
`tests/spec_gherkin_wellformed.rs`, which IS nextest-visible and so runs in
the four-config matrix; the same is true of
`tests/agent_constraints_links.rs`, the authoring-time guard over this
directory's own path and symbol references.

**Expected shape is IDENTICAL in both configs** (current as of
`uat-unskip-campaign` increment C2 — `cli-report-health-print-time`,
`cli-report-health-layer-height-provenance`, and
`cli-report-health-surfaces-ea-default-advisory`, 2026-08-05): 54 features,
166 scenarios (94 passed, 72 skipped, 0 failed), 560 steps (478 passed, 72
skipped, 0 failed), exit 0, register at 24 entries (sum 72). This shape
moves as the campaign lands more increments — trust
`cargo uat`'s own `[Consolidated total]` line over this paragraph if they
disagree, and update this paragraph when they do. A field-sim run reporting
FEWER total steps than the default run means a scenario is aborting early
(a fixture regressed and is panicking before reaching every step) — treat
that as a hard failure, not noise, even if the final failed-count still
reads 0.

**Campaign pointer** (`uat-unskip-campaign`, ratified 2026-07-28): recommended
bands, in order — Band A domain/default-features, Band B nanodlp, Band C
CLI (all DO); Band D field-sim-gated (DEFER); Band E viz (SPLIT into its own
issue, `viz-uat-cucumber-harness` — a second cucumber harness hosted inside
resinsim-viz, since driving Bevy in-process from this harness would invert
ADR-0001's layering). Increment A2 established Band-D membership BY SYMBOL,
not by guess: of the increment's original 3-spec selection, only
`interlayer-crack-knockdown-scales-with-perimeter` turned out to be fully
default-features; `calibration-disclosure-3of3-predicate` and
`honest-zero-yield-fraction-on-calibrated-solid` were demoted to Band D and
now carry declared-debt register entries naming the exact blocking
`#[cfg(feature = "field-sim")]` symbol each depends on and citing the filed
issue `uat-unskip-band-d` (2026-08-02) — NOT
`uat-fixtures-fieldsim-adr0020-gap`, which is the unrelated missing-TOML-
fixture-fields constraint. A2's ratified top-up
(`sim-json-roundtrips-zero-force-layer`) was symbol-verified default-
features BEFORE any step def was written, precisely the check the original
3-spec selection skipped — do the same for any future increment's scope
before writing steps, not after.

Increment A3+B (`uat-unskip-a3-b`, 2026-08-04) corrects the recommended-band
list above: **"Band B nanodlp" is NOT in-process.** All three nanodlp specs'
`When` clauses subprocess the real binary (`resinsim sim --file ...`,
`resinsim inspect calibrate --file ...`), so they follow the Band C CLI
shape through `tests/uat_steps/cli_fixtures.rs`
(`ensure_resinsim_built`, `invoke_resinsim`, `CliOutcome`,
`workspace_data_dir`) — not a distinct in-process band. Only
`cumulative-times-sec-accessor` is genuinely in-process among this
increment's five specs; the single in-process exception inside a nanodlp
spec is `nanodlp-import-simulates` UAT-2 ("the job is imported"), which
calls `io::sliced::parse_sliced` directly. All five specs were verified
default-features BY SYMBOL before any step def was written
(`docs/patterns/band-membership-by-symbol.md`), following A2's precedent;
all five were REMOVED outright from `SPECS_WITHOUT_STEP_DEFS` (none became
declared debt). The increment also introduced `tests/uat_steps/fixtures.rs`
`NanoDlpJobBuilder` — the single home for synthesised `.nanodlp` archives
(`zip::write::ZipWriter` + `CompressionMethod::Stored`, `png::Encoder`,
`flate2::write::GzEncoder`; all reachable from the test target as regular
`resinsim-core` `[dependencies]`) — for the two nanodlp specs whose
committed `mini.nanodlp` fixture could not satisfy every scenario's Given
premise (verified by a real-CLI probe before any assertion was written, not
assumed).

Increment C1 (`uat-unskip-c1`, 2026-08-04) is the Band C CLI increment
proper. It landed two modules covering 10 scenarios
(`cli-sim-producer-writes-sim-json` 6, `cli-sim-rejects-unknown-schema-
version` 4), corrected two stale `schema_version` literals in
`spec/uat/` that ADR-0019's v1→v2 clean break had left behind (the binary
was right, the spec was stale — a correction, not a weakening), and
demoted two specs to Band-D declared debt:
`cli-sim-rejects-tampered-sidecar` (4) and
`cli-sim-budget-mismatch-on-load` (3). Print-time
(`cli-report-health-print-time`, 3 scenarios) was scoped in but deferred
to C2 on sizing (6+4+3 exceeds the increment cap, and print-time is an
unrelated `ReportGenerator`-rendering surface with its own
formula-mirroring / cross-comparison review hazards) — its register entry
was paid down in C2, not left untouched (see the C2 pointer below).

The two demotions introduce a NEW Band-D sub-shape: **binary-build-seam
asymmetry**, distinct from the in-process `#[cfg]` asymmetry the existing
Band-D entries (above) carry. `tests/uat_steps/cli_fixtures.rs`'s
`ensure_resinsim_built` builds the subprocessed
`resinsim` binary with `--bin resinsim -p resinsim-inspect` and NO
`--features`, so any scenario whose only production entry point requires
`field-sim` is unreachable in BOTH `cargo uat` and `cargo uat-field-sim`
TODAY — not merely asymmetric between them. These two specs become the
canonical config-asymmetric shape only once `ensure_resinsim_built` is
taught to forward `--features field-sim` under `#[cfg(feature =
"field-sim")]` — a design decision `uat-unskip-band-d` owns, not this
increment. tampered-sidecar UAT-3 (path traversal) was checked as a
possible envelope-level exception (its `fields_sidecar` pointer parses
fine on default features) and is NOT one: the pointer's only consumer is
itself gated, so a crafted `../escape.bin` pointer is silently ignored
and `report health --in` exits 0.

C1 also generalised the incumbent `^the process exits with code 0$` step
in `ctb_layer_height_authority.rs` with an observation-mode XOR guard, so
a CLI-subprocess spec could share it without a second, ambiguous
registration. The two observation modes are captured as fields on
`crates/resinsim-core/tests/uat_steps/world.rs`'s `UatWorld`:
in-process `sim_primary`/`last_sim_err` XOR CLI-subprocess
`cli_exit_code`. See the generalised step's doc comment for the pattern
if a future spec needs the same step against a different observation
surface.

Increment C2 (`uat-unskip-c2`, 2026-08-05) closes Band C. It landed three
modules covering nine scenarios / 51 steps: `cli-report-health-print-time`
(the module C1 deferred on sizing, 3 scenarios / 17 steps),
`cli-report-health-layer-height-provenance` (3 scenarios / 17 steps), and
the NEW spec `cli-report-health-surfaces-ea-default-advisory` (3
scenarios / 17 steps). Two register entries were removed outright:
print-time 3 -> 0, and provenance's zero-scenario entry 0 -> 0. Net
scenario debt 75 -> 72, register 26 -> 24 entries. All three specs were
verified default-features BY SYMBOL at both seams before any step was
written, following A2/A3+B's precedent: the in-process `report health`
call path (every symbol on it `#[cfg]`-free) and the binary-build seam,
`ensure_resinsim_built` (`crates/resinsim-core/tests/uat_steps/cli_fixtures.rs`),
which forwards no `--features`.

The provenance promotion is the first spec in this campaign whose
scenarios were AUTHORING-blocked rather than step-blocked: three untagged
fences, the non-executable `Scenario (proposed):` keyword, and a wrapped
continuation line, all fixed in the same change as the module landing.
That promotion also retired the tree's only entry in
`SPECS_WITH_NO_EXECUTABLE_SCENARIOS`
(`crates/resinsim-core/tests/spec_gherkin_wellformed.rs`), leaving the
const empty — the mechanism itself stays for the next spec that lands
proposed-only.

The ea-default-advisory spec is a harvest-to-UAT promotion: the KB-153
consumer-path advisory (`warn_if_envelope_ea_is_default`,
`crates/resinsim-inspect/src/profile_loader.rs`) previously had only
nextest coverage (`crates/resinsim-inspect/tests/thermal_cli_warnings.rs`,
`crates/resinsim-inspect/tests/report_health_time_cli.rs`); the new UAT
asserts the user-visible CLI contract (which stream carries what, the
exactly-once property, the three-way flag semantics) without duplicating
that coverage's plumbing.

**Band C is now CLOSED.** Remaining campaign work: Band D (field-sim-gated,
`uat-unskip-band-d`) and Band E (viz, `viz-uat-cucumber-harness`).

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

### Seventh command: the viz UAT suite (`cargo uat-viz`)

`cargo uat-viz` (alias in `.cargo/config.toml` for
`test --test uat_viz_gherkin -p resinsim-viz`). A SECOND, independent
cucumber harness (`viz-uat-cucumber-harness`, 2026-08-06), hosted inside
`resinsim-viz` rather than joining the fifth/sixth commands above,
because `env!("CARGO_BIN_EXE_resinsim-viz")` is only available inside
that crate's own build graph and because a shared harness would impose a
display/GPU requirement on `cargo uat`, which is display-free today. See
`docs/adr/0024-second-uat-harness-in-resinsim-viz.md` for the full
rationale (including the honest three-leg case, not just "avoids a
dependency cycle" — that claim alone does not survive scrutiny given
`cli_fixtures.rs` already proves a cross-package subprocess needs no
dev-dep).

Like the fifth/sixth commands, it does not run under `cargo nextest
run` — the SAME `.config/nextest.toml` `not binary(/^uat_/)` pattern
excludes `uat_viz_gherkin` with zero configuration change, confirmed via
`cargo nextest list -p resinsim-viz`. Reuses core's harness DESIGN
(silent-green guard per-feature and in aggregate, parse-error guard, the
three-direction register check) via the SAME extractor
(`crates/resinsim-core/tests/uat_steps/extract.rs`, included by a
cross-crate `#[path]`, matching the mechanism
`tests/spec_gherkin_wellformed.rs` already uses), but keeps its OWN
register scoped to `spec/uat/viz-*.md` only, plus a fourth guard core
has no equivalent for: the harness's spec set must equal exactly the
on-disk `viz-*.md` set.

`cargo uat-viz` opens real windows for its tier-B scenarios (a live GPU
is required) — a developer-machine gate, not a CI gate (there is no CI
in this repo today). Run it whenever `spec/uat/viz-*.md` or
`crates/resinsim-viz/tests/uat_viz_steps/` changes.

**Expected shape** (as of the pilot, `viz-screenshot-flag` only, 7 of 12
scenarios stepped): 12 features, 12 register entries summing 23, 35
steps passed / 23 skipped / 0 failed, 0 parsing errors, exit 0.
`viz-screenshot-flag` itself: 31 steps passed, 5 skipped (UAT-2/5/8
env-conditional — this repo has zero committed `.ctb` fixtures; UAT-6
not drivable, `bevy_egui` 0.39 has no synthetic pointer-click API;
UAT-7d declared debt — `clap` 4.6.1 rejects an empty `--screenshot`
value at the parser level before the app's own validation ever runs,
covered instead by an existing unit test). Trust `cargo uat-viz`'s own
`[Consolidated total]` line over this paragraph if they disagree, same
rule as `cargo uat` above.

**Transitional double count, deliberate:** the 7 scenarios `cargo
uat-viz` now steps are STILL counted as skipped debt in core's
`SPECS_WITHOUT_STEP_DEFS` (all 12 `viz-*` entries there are unchanged by
this addition). Removing them is the follow-up lifecycle
`viz-uat-register-migration`, one spec at a time, starting with
`viz-screenshot-flag`'s entry — not a same-change cleanup. See the ADR's
"Migration plan".

`cargo nextest run -p resinsim-viz --all-targets` build time lengthens
(cucumber, tokio, and pulldown-cmark now compile for `resinsim-viz`'s
dev graph) — a build-time cost, not a test-count change; the four-config
matrix's test counts are unaffected.

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
- `agent-constraints/knowledge-base.md` — KB layout (docs/patterns/, docs/patterns/anti/, docs/kb/, docs/adr/)
- `agent-constraints/iteration-limits.md` — autonomous loop caps
- `using-jj-workspaces` skill — sibling-workspace mechanics
- ADR-0015 (this issue) — example of a clean linear-history feature commit
- `docs/adr/0024-second-uat-harness-in-resinsim-viz.md` — the seventh
  command's full rationale
