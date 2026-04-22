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

```toml
# .config/nextest.toml
[profile.default]
default-filter = "not binary(<bdd_target>)"
```

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

## Trade-offs

- Loses uniform `cargo nextest run` invocation for the cucumber tests.
- Filter is exact-name match (`binary(<name>)`) — renaming the cucumber
  test target silently breaks the filter. Either widen to a name pattern
  (`/^uat_/` etc.) or add a CI sanity check that asserts the filter still
  matches at least one binary.

## Related

- `silent-green-guard-for-custom-test-harness.md` — the assert pattern
  used in the harness entry point.
- `docs/adr/0008-bdd-uat-spike-notes.md` — the spike that established
  this pattern.
