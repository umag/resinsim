---
issue: 01-viz-crate-scaffold
date: 2026-04-26
---

# Anti-pattern: subprocess smoke test as the primary verification of a Bevy startup

## Symptom

A test that runs the binary as a subprocess and asserts on its exit
code:

```rust
// tests/smoke.rs (DON'T)
#[test]
fn binary_launches_and_exits() {
    let path = env!("CARGO_BIN_EXE_resinsim-viz");
    let status = std::process::Command::new(path)
        .arg("--smoke-exit")
        .status()
        .expect("spawn");
    assert!(status.success());
}
```

## Why it's wrong

For a scaffold whose deliverable is "specific entities and plugins are
wired up", this test verifies almost nothing:

- ✗ Cannot assert on World state (which entities spawned, what
  components they carry).
- ✗ Cannot detect "DirectionalLight was deleted by a refactor"
  regressions.
- ✗ Cannot detect "PanOrbitCameraPlugin no longer registered"
  regressions.
- ✗ Requires a display server / GPU on CI, OR an env-var skip gate
  that turns the test into a no-op on most CI providers.
- ✗ Slow: subprocess + DefaultPlugins init + window creation = several
  seconds per run.

It only verifies the binary did not panic during plugin init. That
information is already covered the first time anyone runs `cargo run`
locally.

## What to do instead

See pattern `bevy-app-test-seam.md`: extract startup logic as
`pub fn setup_scene(Commands)` and unit-test it on
`App::new()` (no plugins) by querying `World` after `app.update()`.
Programmatic, fast (~600ms), runs anywhere.

## When subprocess tests ARE appropriate

When the thing under test IS the binary's CLI surface — argument
parsing, exit codes, stdout/stderr formatting. The `resinsim-inspect`
package's `cli_fixtures.rs` is the right home for those (CLI-shape
verification), not the viz crate.

## See also

- Pattern `bevy-app-test-seam.md` — the in-process test pattern
- 01-viz-crate-scaffold plan v1 → v2 review history — first time this
  anti-pattern was caught (adversarial review HIGH, testing)
