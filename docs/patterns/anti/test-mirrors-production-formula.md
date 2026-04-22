---
issue: uat-gherkin-runner
date: 2026-04-23
---

# Anti-pattern: Test mirrors production formula instead of invoking it

## Description

A test that asserts behavior by re-implementing the production formula
in the test body, rather than calling the production code path.

## Example (caught in spike — uat-gherkin-runner)

`crates/resinsim-core/tests/uat_gherkin.rs:55` originally read:

```rust
fn then_safety_factor_infinity(world: &mut SpikeWorld) {
    // Mirrors LayerResult construction at services/failure_predictor.rs:306:
    //   safety_factor: safety.map_or(f32::INFINITY, |s| s.value())
    let recorded = world.computed_safety
        .expect("...")
        .map_or(f32::INFINITY, |s| s.value());
    assert!(recorded.is_infinite() && recorded.is_sign_positive());
}
```

The test does NOT call `FailurePredictor::predict_layer`. It calls
`SafetyFactor::compute(...)` directly (one component) then asserts a
formula it copied from the layer-result construction code. If
`failure_predictor.rs:306` ever changes — e.g. swap `INFINITY` for
`f32::MAX`, or wrap in `Option<f32>` — both the production code and the
test's mirror break together, but the mirror was already wrong before
the production change; nothing flagged the divergence.

## Why this happens

The "right" way (calling `predict_layer` directly) requires constructing
a fixture of 10 inputs (PrinterProfile, ResinProfile, Recipe,
SupportConfig, PlateAdhesionProfile, ThermalContext, ...). That cost
discourages end-to-end assertion; engineers reach for the smaller scope
and end up mirroring formulas. Spike scope decisions reinforce this.

## What to do instead

1. **Build the fixture.** Even at painful cost. The test then exercises
   the actual production execution path. If 10 args is too many,
   factor a `PredictLayerInputs::default_for_test()` builder and
   accept that as a per-test-suite shared helper.
2. **Test the formula at its definition site.** If the formula lives in
   one function (`fn record_safety_factor(safety: Option<SafetyFactor>) -> f32`),
   call THAT function from the test. Don't re-derive it.
3. **At the very least, link the mirror.** If you accept the mirror as a
   trade-off, leave a comment pointing at the production line so
   `git blame` plus a search for "Mirrors XXX" finds the test. (The
   spike does this — it's a soft link, not enforcement, but it's better
   than nothing.)

## Detection

Search for tests with comments containing "Mirrors", "duplicates", or
"copy of <production-file>". Each is a candidate for refactor.

## Related

- `phase-boundaries-for-ddd-refactors.md` — phase B atomic-commit pattern
  applies here: if you're making the production formula change, MOVE
  the mirror to the production-call form in the same commit.
