---
issue: supportanalyzer-restoration
date: 2026-04-24
---

# Pattern: Decomposition invariant test for aggregated result structs

## Context

Domain services frequently return a result struct that carries both
intermediate components AND a total derived from them:

```rust
pub struct SupportAssessment {
    pub support_capacity: SupportCapacity,   // tips
    pub plate_capacity_n: f32,               // plate
    pub total_capacity: SupportCapacity,     // plate + tips
    pub safety_factor: Option<SafetyFactor>,
    pub overload: Option<FailureEvent>,
}
```

Downstream consumers often read `total_capacity` without checking how it
was built. A future edit to the building code could silently:

1. Swap operands (`plate_cap - support_cap` instead of `+`)
2. Drop a component (`plate_cap` only, forgetting supports)
3. Rename the field and accidentally miss updating the construction

Existing integration tests exercise end-to-end behaviour but usually
don't single out the combination rule, so they may pass even with the
drift (especially when one component is small or zero).

## Pattern

Add a dedicated unit test that constructs inputs yielding known non-zero
values for every component, then asserts the total equals their sum
within float tolerance:

```rust
#[test]
fn assess_total_capacity_equals_plate_plus_supports() {
    let assessment = SupportAnalyzer::assess(/* known inputs */);
    let expected = assessment.plate_capacity_n
        + assessment.support_capacity.value();
    assert!(
        (assessment.total_capacity.value() - expected).abs() < 1e-4,
        "total {} ≠ plate ({}) + supports ({}) = {}",
        assessment.total_capacity.value(),
        assessment.plate_capacity_n,
        assessment.support_capacity.value(),
        expected,
    );
}
```

Use a fixture where BOTH components are non-zero and distinguishable
(e.g. 250 N + 37.7 N, not 0 N + 37.7 N) so that swapping operands or
dropping a component produces a numerically different answer.

## When to apply

- Service result structs with a "total" or "aggregated" field plus its
  constituent parts.
- Any refactor that moves aggregation logic across module boundaries
  (the target of this pattern).

Not needed for: result structs with a single scalar field, or where the
total is computed by a well-tested existing primitive (`BuildPlate::total_capacity`)
— in that case the primitive itself has the test coverage.

## Counter-indication: trivial sums

If the "total" is literally `a + b` in one line with no intermediate
wrapping, the test is low-signal. Apply the pattern when the total
involves type wrapping (`SupportCapacity::new(plate + tips)`), unit
conversions, or multi-step composition that can go wrong in ways the
compiler can't catch.

## See also

- `resinsim/crates/resinsim-core/src/services/support_analyzer.rs`
  (test `assess_total_capacity_equals_plate_plus_supports`)
- Plan-review finding A-MED2 in the `supportanalyzer-restoration` issue
  history
