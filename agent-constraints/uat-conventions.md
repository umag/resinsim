# UAT conventions for resinsim

These conventions extend the issue-lifecycle skill's Phase 2 Step 6 UAT
lookup and Phase 6 Step 1 harvest defaults. The skill reads this file
automatically when present.

## Where UAT lives

`spec/uat/*.md` is the single source of truth for UAT scenarios — **not**
`tests/uat/`, which the skill's fallback list names and which does not
exist in this tree. Step definitions live at
`crates/resinsim-core/tests/uat_steps/`; the cucumber harness binary is
`crates/resinsim-core/tests/uat_gherkin.rs`.

No `.feature` file is checked in. The harness synthesises one per spec at
run time into `$CARGO_TARGET_TMPDIR/spec-uat-features`, named
`<spec-stem>.feature`. If you're expecting a checked-in `tests/uat/*.feature`
tree from an older convention, it was real once (spike-era, see
`docs/adr/0008-bdd-uat-spike-notes.md`) and was removed at rollout — treat
any memory of a checked-in `.feature` pairing as stale.

## Spec file format

### Filename and frontmatter

The kebab-case filename stem IS the spec's identity — it is the debt
register's key (see below) and the source for the step-def module name.
Frontmatter is YAML with `issue:` and `date:`. `issue:` is load-bearing:
`validate_spec_uat_dir` (`uat_steps/extract.rs`) loud-fails a directory
whose `.md` files don't carry it, rather than silently treating an
empty/wrong directory as "no specs."

### Body shape

`# UAT: <title>` H1; optional `## Rationale` or ADR-note prose; one
`## UAT-N: <scenario title>` H2 per scenario, each holding exactly one
` ```gherkin`-tagged fence. The heading text becomes the scenario name,
with any `Scenario:` / `Scenario Outline:` keyword prefix stripped
(`normalize_heading` in `uat_steps/extract.rs`).

### Two ways to lose every scenario in a file, silently

- An untagged ` ``` ` fence (no `gherkin` language tag) — the extractor
  skips it. `extract()` only recognises fences tagged `gherkin`.
- Markdown bullets, or a wrapped continuation line, inside step text —
  cucumber's Gherkin parser rejects it and the whole file's scenarios drop
  to zero. `spec_gherkin_wellformed.rs`'s
  `every_spec_uat_md_synthesises_well_formed_gherkin` is the authoring-time
  guard that catches this
  (`the_guard_rejects_a_wrapped_continuation_line` pins the regression);
  see `docs/patterns/anti/markdown-bullets-in-gherkin-step.md` for why the
  trap is easy to reintroduce — it was violated repeatedly before the
  guard existed.

## Adding a scenario (the Phase 6 harvest destination)

Destination is `spec/uat/<kebab-slug>.md` — **not** the skill's
`tests/uat/<path>.feature` default. Append a `## UAT-N:` to an existing
spec when the subject matches; start a new file otherwise. A new spec
landing without a step-def module must arrive with a
`SPECS_WITHOUT_STEP_DEFS` entry in the same commit, or the suite goes red
(see below).

## Step definitions

### Module naming and wiring

One module per spec. Name = spec stem, kebab→snake, for grep
traceability. Exceptions are recorded explicitly in
`STEP_DEF_MODULE_RENAMES` (`uat_gherkin.rs`) — each entry is checked at
test time to still resolve to a real spec file. A module must appear BOTH
as `pub mod` in `uat_steps/mod.rs` AND in the `use uat_steps::{...}` list
in `uat_gherkin.rs`; `assert_mod_rs_and_use_list_agree` asserts the two
sets are equal, which matters because `-Aunused_imports`
(`.cargo/config.toml`) means a missing `use` cannot warn its way to being
noticed.

### Shared support code

`NON_STEP_MODULES` (`uat_gherkin.rs`) names the modules under
`uat_steps/` that are shared support, not per-spec bindings: `extract`,
`extract_tests`, `world`, `fixtures`, `cli_fixtures`. Hand-rolled
resin/printer TOML fixtures must compose from `world.rs`'s builders
(`ResinBuilder`, `PrinterBuilder`, `RecipeBuilder`) or `fixtures.rs`'s
shared fragments, never hand-copied literals — see
`docs/patterns/anti/fixture-copy-of-shared-builder.md` for why a copy that
omits a field compiles and passes under three of the four ADR-0017
configs and only fails under `cargo uat-field-sim`. CLI specs additionally
go through `cli_fixtures.rs` (`ensure_resinsim_built`).

### Step regexes

One registration per regex — an ambiguous match is a runtime cucumber
error, not a compile error. Every regex should carry a pointer comment
naming its spec and `UAT-N`, because nothing else links step-text prose
back to the regex that matches it.

### Verify the band by symbol before writing a step

Before writing a step for a scenario, enumerate each Given/When/Then's
production symbol and grep for `#[cfg(feature = "field-sim")]` on it; the
union of what you find determines whether the scenario belongs on default
features or is field-sim-gated. See
`docs/patterns/band-membership-by-symbol.md` — band membership determined
by label or guesswork has been wrong before.

## The debt register: `SPECS_WITHOUT_STEP_DEFS`

Shape: `&[(&str, usize)]` — `(spec stem, expected_skipped_scenario_count)`.
Three debt classes can occupy an entry, and each must name the reason in
its own comment: no step-def module at all; one blocked scenario in an
otherwise-stepped spec, which must cite a **filed issue**; or a
config-asymmetric field-sim scenario, whose production entry point only
exists under `#[cfg(feature = "field-sim")]` so no single step-def
gating can satisfy both `cargo uat` and `cargo uat-field-sim` against the
one shared register. Net debt (the sum of every registered count) is
meant to shrink monotonically; a Scenario Outline registers its RUNTIME
row count, not its authored row count. `expected == 0` entries are a
legitimate steady state, not a smell.

This paragraph is a summary and a pointer — the authoritative text is the
doc comment on `SPECS_WITHOUT_STEP_DEFS` in `uat_gherkin.rs` and the guard
section of `implementation-conventions.md`; read those before relying on
the summary above for anything precise.

## Running the suite

`cargo uat` AND `cargo uat-field-sim` (both aliased in
`.cargo/config.toml`), whenever `spec/uat/*.md` or `tests/uat_steps/`
changes. Neither runs under `cargo nextest run` — `.config/nextest.toml`
excludes every `uat_*`-named binary because the harness is
`harness = false` and aborts nextest's enumeration otherwise. The
authoring-time guard `spec_gherkin_wellformed.rs` is NOT `uat_`-prefixed,
so it IS nextest-visible and runs in the four-config matrix. For the full
verification battery and the current expected shape, see
`implementation-conventions.md` — no counts are restated here because they
move with every campaign increment.

## Prior-art lookup at planning Step 6

Grep `spec/uat/*.md` by `affectedAreas` and the issue title; also grep
`tests/uat_steps/` for a matching module — a hit there without a
corresponding spec entry usually means `STEP_DEF_MODULE_RENAMES` is in
play. Record `priorArt.uatScenarios` entries as `spec/uat/<stem>.md`
paths, not as `.feature` paths — there are no checked-in `.feature` files
to point at.

## See also

- `agent-constraints/implementation-conventions.md` — build/verify
  commands, the four-config matrix, the full guard-layer description and
  the current expected shape
- `agent-constraints/knowledge-base.md` — where a UAT finding gets written
  up once it stops being just a test
- `docs/patterns/cucumber-in-nextest-workspace.md` — why the suite needs
  its own alias instead of joining nextest
- `docs/patterns/extracting-gherkin-from-markdown.md`
- `docs/patterns/per-spec-runtime-skip-attribution.md`
- `docs/patterns/band-membership-by-symbol.md`
- `docs/patterns/anti/markdown-bullets-in-gherkin-step.md`
- `docs/patterns/anti/fixture-copy-of-shared-builder.md`
