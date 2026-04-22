---
issue: uat-gherkin-runner
date: 2026-04-23
---

# Pattern: Silent-green guard for harness=false test binaries

## Context

A `[[test]] harness = false` target is a plain `fn main()`. cargo test
runs the binary and treats exit-0 as pass. Nothing in this contract
guarantees the binary did any actual work:

- A cucumber-style harness with a typo'd or empty feature directory:
  `0 scenarios run`, exit 0, cargo says "passed".
- A harness with relative paths (e.g. `Path::new("tests/uat")`) when
  invoked via `cargo --manifest-path` or under a debugger that sets a
  different CWD: same outcome.
- A harness with a `#[cfg(...)]` that conditionally compiles all test
  bodies away: empty binary that exits 0.

Silent green is the most dangerous regression mode because it replaces a
passing test with a passing test that asserts nothing. CI stays green;
nobody investigates.

## Pattern

Two complementary guards in the harness `fn main()`:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let writer = MyHarness::run().await;

    // Guard 1 — assert work was done.
    let total = writer.passed_steps() + writer.skipped_steps() + writer.failed_steps();
    assert!(
        total > 0,
        "harness did not exercise any test units — check resource paths and filters",
    );

    // Guard 2 — propagate failures via exit code.
    if writer.execution_has_failed() {
        std::process::exit(1);
    }
}
```

Plus: anchor any resource paths with `env!("CARGO_MANIFEST_DIR")` so CWD
shifts can't repurpose the harness onto an empty/wrong directory.

## When to use

Any time you write a `[[test]] harness = false` binary in this workspace.
The cost is ~5 lines; the cost of a silent-green regression is unbounded.

## Related

- `cucumber-in-nextest-workspace.md` — uses this pattern in its
  `tests/uat_gherkin.rs` entry point.
