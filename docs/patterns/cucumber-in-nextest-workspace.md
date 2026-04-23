---
issue: uat-gherkin-runner
date: 2026-04-23
---

# Pattern: cucumber-rs alongside cargo-nextest

## Context

The workspace standard test runner is `cargo nextest` (per
`feedback_use_nextest`). cucumber-rs binaries built with `harness = false`
do NOT speak nextest's libtest terse listing protocol
(`--list --format terse`). The cucumber `libtest` feature emits libtest's
*streaming JSON* dialect (`{"type":"test","event":"started",...}`), which
is libtest's `--format json` output — a different protocol from the terse
listing nextest uses for discovery.

A cucumber binary added naively into the workspace causes the FULL
`cargo nextest run -p <crate>` sweep to abort with
"creating test list failed", because nextest probes every test target.

## Pattern

Three coordinated pieces:

### 1. Test target with harness=false and a tokio entry point

```toml
# crates/<crate>/Cargo.toml
[[test]]
name = "<bdd_target>"
harness = false
```

```rust
// tests/<bdd_target>.rs
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let features = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/<features-dir>");
    let writer = MyWorld::cucumber().run(features).await;

    // Silent-green guard — see anti-pattern entry.
    let total = writer.passed_steps() + writer.skipped_steps() + writer.failed_steps();
    assert!(total > 0, "no scenarios discovered");

    if writer.execution_has_failed() {
        std::process::exit(1);
    }
}
```

### 2. Workspace-level nextest exemption

Widen the filter to a name pattern — exact-name match regresses
silently when the binary is renamed, or when a sibling cucumber binary
is added:

```toml
# .config/nextest.toml
[profile.default]
default-filter = "not binary(/^uat_/)"
```

Pin the pattern with a regression-guard test that reads the config
file (NOT a `cargo nextest list` subprocess — recursion / lock
contention). See
`resinsim-core/tests/nextest_filter_sanity.rs` for the implementation
pattern.

### 3. Documented runner command

Cucumber UAT runs via `cargo test --test <bdd_target> -p <crate>`, NOT
nextest. Document this prominently in the test file's header comment AND
in any CI / contributor docs.

## When to use

When you want BDD-style scenario tests (cucumber-rs) in a workspace whose
default runner is cargo-nextest. The exemption is acceptable when the
trade-off (per-binary attribution instead of per-scenario in nextest) is
worth the BDD ergonomics.

## When NOT to use

- If per-scenario CI failure attribution is a hard requirement, the BDD
  runner needs different wiring (cucumber JUnit XML output → nextest junit
  ingestion is one path).
- If the project doesn't use nextest, the exemption is irrelevant and a
  plain `[[test]] harness = false` + cucumber works directly.

## Read-from-md extension (rollout outcome)

The rollout replaced the per-spike `.feature` file duplicates under
`tests/uat/` with runtime extraction from `spec/uat/*.md`. The harness:

1. Resolves `spec/uat/` from `CARGO_MANIFEST_DIR` via `ancestors()` +
   `canonicalize()`.
2. Validates the directory carries `issue:` YAML frontmatter across at
   least one `.md` file — the "right path, wrong directory" slip loud-
   fails here rather than silently producing zero scenarios.
3. Extracts ```gherkin fenced code blocks from each file.
4. Synthesises a `Feature:` block per file under
   `$CARGO_TARGET_TMPDIR/spec-uat-features/`.
5. Runs cucumber once against that synthesised tree.

See `docs/patterns/extracting-gherkin-from-markdown.md` for the
extractor's `.md` fence conventions (kebab-case file name → snake_case
step-def module; H2 `## UAT-N:` sub-heading per scenario; DataTable +
DocString compound inputs).

## Trade-offs

- Loses uniform `cargo nextest run` invocation for the cucumber tests.
- Widened filter (`/^uat_/`) catches any future `uat_*` test binary in
  the workspace — deliberately broad. `tests/nextest_filter_sanity.rs`
  locks the pattern against accidental narrowing.

## Related

- `silent-green-guard-for-custom-test-harness.md` — the assert pattern
  used in the harness entry point.
- `extracting-gherkin-from-markdown.md` — the `.md` fence convention
  the harness reads from.
- `docs/adr/0008-bdd-uat-spike-notes.md` — the spike + rollout that
  established this pattern.
