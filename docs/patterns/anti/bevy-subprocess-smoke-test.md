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

## Update (viz-uat-cucumber-harness, 2026-08-06)

The claim above — "not the viz crate" — was written for issue 01, when
`resinsim-viz` had no CLI contract worth verifying. ADR-0013 later gave
it one: an 8-code `--screenshot` exit-code surface
(`EXIT_SCREENSHOT_BAD_PATH`, `EXIT_SIM_LOAD_FAILED`,
`EXIT_LAYER_COUNT_MISMATCH`, `EXIT_BAD_SIM_PAIRING`,
`EXIT_CTB_LOAD_FAILED`, `EXIT_SCREENSHOT_WRITE_FAILED`,
`EXIT_SCREENSHOT_RENDER_TIMEOUT`). The "When subprocess tests ARE
appropriate" exception directly above — CLI surface IS the thing under
test — now applies to the viz binary too, and
`docs/adr/0024-second-uat-harness-in-resinsim-viz.md` hosts exactly that
kind of test: `crates/resinsim-viz/tests/uat_viz_gherkin.rs` +
`tests/uat_viz_steps/viz_screenshot_flag.rs` assert ONLY exit codes,
stderr substrings, and file presence/absence — never World state,
never entity/component queries — which is precisely what this doc's
"Why it's wrong" section says a subprocess test cannot verify and
therefore must not be asked to. Two viz specs that DO mix World-state
assertions into their `Then`s (`Mesh::ATTRIBUTE_COLOR`, `LayerCursor`
entity presence, window title) were identified during that ADR's
planning and deliberately NOT piloted via subprocess — see the ADR's
"E1/E2 split" section. The rule this doc states is therefore unchanged;
only the set of specs that clear it grew.

## See also

- Pattern `bevy-app-test-seam.md` — the in-process test pattern
- 01-viz-crate-scaffold plan v1 → v2 review history — first time this
  anti-pattern was caught (adversarial review HIGH, testing)
- `docs/adr/0024-second-uat-harness-in-resinsim-viz.md` — the second
  cucumber harness that exercises this doc's exception
