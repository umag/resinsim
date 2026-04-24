---
issue: print-time-on-reportgenerator
date: 2026-04-24
---

# Pattern: Direction invariants belong in factory-scoped tests; proptest only rank invariants

## Context

Property tests are most useful when the invariant holds across the *full
domain* of inputs the generator can produce. "Cumulative time is
monotonic" is true for every valid Recipe + PrinterProfile — a genuine
proptest candidate. "Tilt total < Linear total" is true on the *shipped
factory defaults* but not across the full domain of recipes — a hand-
crafted Tilt recipe with a large `lift_cycle_sec` can reverse the
direction.

Mixing the two kinds of invariant in the same proptest body produces a
test that either fires on legitimate recipe variations (flaky) or is
weakened so far (`differ` instead of `<`) that it stops catching the
thing you actually wanted to assert.

## Rule

- **Rank invariants** — monotonic, non-negative, bounded, phase sum equals
  total, etc. — belong in proptest. They hold over the whole input space,
  so the generator is the right source.
- **Direction invariants** — "A < B" between two specific configurations —
  belong in factory-scoped `#[test]` functions that exercise the
  configurations by name. If the direction is worth asserting, it's
  because those specific configurations have a real-world meaning that a
  proptest generator wouldn't preserve.

## Concrete example

`crates/resinsim-core/tests/sim_summary_time_properties.rs`:

```rust
proptest! {
    // Rank invariants — hold over all valid Recipes + PrinterProfiles.
    fn total_time_monotonic_in_layer_count(n in 1u32..300) { ... }
    fn total_positive_when_nonempty(n in 0u32..100) { ... }
    fn phase_sum_equals_total(n in 1u32..300, tilt in any::<bool>()) { ... }
}

// Direction invariant — factory-scoped, explicit configurations.
// Under v4 (print-time-on-reportgenerator), PrintSimulation owns
// Recipe + PrinterProfile — summary() is arg-less.
#[test]
fn tilt_strictly_less_than_linear_on_default_factories() {
    let recipe = default_recipe();
    let linear = PrinterProfile::generic_msla_4k();
    let tilt = PrinterProfile::elegoo_mars5_ultra();
    for n in [1u32, 10, 100, 500] {
        let s_linear = build_sim(n, recipe.clone(), linear.clone()).summary();
        let s_tilt = build_sim(n, recipe.clone(), tilt.clone()).summary();
        assert!(s_tilt.total_time_sec < s_linear.total_time_sec);
    }
}
```

The comment above the direction test cites the numerical source of truth
(`layer_timing_calculator.rs` per-layer test values: 10.5s vs 14.0s) and
explains the factory scope. If factory defaults change and the direction
reverses, the test fires with a clear signal — informative, not flaky.

## When the invariant's scope is ambiguous

Ask: "If the proptest generator produced an edge-case input that reversed
the direction, would I want the test to fail?" If yes, it's a rank
invariant (pull the factory out of the generator). If no, it's a direction
invariant (pull the generator out of the factory).

## Related

- `crates/resinsim-core/tests/layer_timing_properties.rs` — older version
  of this idea. Every property in that file is a rank invariant; direction
  assertions live in the module's unit tests.
- ADR-0007 — the release-mechanism branch that makes the direction
  invariant meaningful in the first place.
