---
issue: uat-gherkin-runner-rollout
date: 2026-04-23
---

# Pattern: extracting Gherkin from markdown source of truth

## Context

UAT scenarios in `spec/uat/*.md` carry BOTH the rationale prose that
makes the spec readable AND the Given/When/Then steps the automation
needs to execute. Keeping two files in sync (.md for humans + .feature
for cucumber) introduces drift the moment one side edits without the
other. Markdown-with-Gherkin (MDG) is supported natively only by the
JavaScript `@cucumber/gherkin` implementation; the Rust `gherkin` crate
parses `.feature` files only.

This pattern solves the drift problem by keeping `.md` as the single
source of truth and extracting ```gherkin fenced code blocks at test
runtime.

## Pattern

### 1. Source file conventions

Every UAT `.md` carries a YAML frontmatter block with an `issue:` key
(the spec's owning issue slug) — used both as free-form metadata AND as
a structural signal the harness's `validate_spec_uat_dir` glob
asserts on:

```markdown
---
issue: <slug>
date: YYYY-MM-DD
---

# UAT: <short description>

## UAT-1: <scenario description>

**Rationale.** <prose>

\`\`\`gherkin
Scenario: UAT-1 <cucumber scenario name>
  Given …
  When …
  Then …
\`\`\`
```

- One ```gherkin fence per cucumber Scenario — compound UAT-Ns (e.g.
  UAT-1 has both a "printer" and a "resin" sub-case) get H3 sub-headings
  + one fence per sub-case so extractor titles remain unique.
- Step text is natural Gherkin prose. DataTable (`|` rows) and
  DocString (triple-double-quote blocks) compound inputs are supported;
  bullet-list (`- item`) continuation is NOT valid Gherkin and will
  fail cucumber parsing — use DocString or DataTable instead.

### 2. Kebab-case `.md` → snake_case `.rs` convention

Rust module names can't contain hyphens. Each step-def module file name
mirrors its source `.md` file name character-for-character with `-`
replaced by `_` — the longest meaningful snake_case variant, NOT a
truncation, so `grep -r cli_profile_by_name_loading` still finds the
whole chain:

| source `.md` | step-def `.rs` |
|--------------|----------------|
| `cli-profile-by-name-loading.md` | `cli_profile_by_name_loading.rs` |
| `legacy-resin-toml-without-recipe.md` | `legacy_resin_toml_without_recipe.rs` |
| `safety-factor-zero-force.md` | `safety_factor_zero_force.rs` |

The pairing is verified by the extractor's whole-suite invariant test
and by the harness's `validate_spec_uat_dir` frontmatter glob.

### 3. Extractor (pure, total)

`tests/uat_steps/extract.rs::extract(&str) -> Vec<ExtractedScenario>`
walks pulldown-cmark events and captures every ```gherkin fence with
its preceding heading. BOM tolerated, YAML frontmatter stripped, CRLF
line endings normalised. Total function — any byte sequence (via
`String::from_utf8_lossy`) yields a `Vec`, possibly empty, without
panicking.

Coverage: 10 unit tests + 4 proptest properties pin the contract (see
`tests/uat_steps/extract_tests.rs`).

### 4. Harness synthesis

`tests/uat_gherkin.rs::main()`:

```rust
let spec_uat = resolve_spec_uat_dir(); // ancestors(2) + canonicalize
let md_files = validate_spec_uat_dir(&spec_uat)
    .unwrap_or_else(|e| panic!("spec/uat validation failed: {e}"));

let features_dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
    .join("spec-uat-features");
let _ = std::fs::remove_dir_all(&features_dir); // stale-file isolation
std::fs::create_dir_all(&features_dir)?;

for md_path in &md_files {
    let md = std::fs::read_to_string(md_path)?;
    let scenarios = extract(&md);
    if scenarios.is_empty() { continue; }
    let feature_text = synthesize_feature(
        &md_path.file_stem().unwrap().to_string_lossy().replace('-', " "),
        &scenarios,
    );
    std::fs::write(features_dir.join(...), feature_text)?;
}

let writer = UatWorld::cucumber().run(&features_dir).await;
```

### 5. Frontmatter glob loud-fail

`validate_spec_uat_dir` rejects an empty or non-matching directory
with both the resolved path AND the expected pattern in the error, so
a "right path, wrong directory" slip surfaces loudly rather than as a
silent-green zero-scenario run.

## When to use

- A `.md`-based spec source of truth + a Rust test harness that wants
  to execute the scenarios.
- Drift between prose and executable spec is a real risk.
- The team values rationale prose alongside the Gherkin.

## When NOT to use

- If `.feature` files alone suffice (no rationale prose needed).
- If the repo uses a language with native MDG support (JavaScript's
  `@cucumber/gherkin`) — use that instead.

## Related

- `docs/patterns/cucumber-in-nextest-workspace.md` — the runner
  invariants this pattern plugs into.
- `docs/adr/0008-bdd-uat-spike-notes.md` — the rollout that
  established this pattern.
