---
issue: supportanalyzer-restoration
date: 2026-04-24
---

# Pattern: Preserve inclusive boundaries when refactoring `!predicate()` checks

## Context

Refactoring a predicate-based branch across module boundaries — e.g.
extracting an inline `if let Some(x) = option.filter(|x| !x.is_safe()) { ... }`
block into a named method — invites two failure modes:

1. **Prose drift in planning/review.** Documentation describes the new
   behaviour as "fires when SF < 1.0" when the code actually fires when
   `!is_safe()` and `is_safe()` is `self.0 > 1.0`, which means the overload
   also fires at exactly SF = 1.0. The prose and the code diverge at the
   boundary.
2. **Silent boundary flip.** A subsequent edit "fixes" the predicate to
   match the prose (`if sf < 1.0` instead of `!sf.is_safe()`), losing
   the inclusive case. No test fails because no test constructs inputs
   that land at the exact boundary.

Example from `supportanalyzer-restoration`: plan v1 step 1 said
"Overload: `Some when SF < 1.0`"; current code is
`safety.filter(|s| !s.is_safe())` where `SafetyFactor::is_safe()` is
`self.0 > 1.0`. Plan-review finding A-MED1.

## Pattern

Two rules, applied together:

### Rule 1 — Mirror the predicate literally in prose

When documenting a predicate-based branch, quote the exact code form.
Don't paraphrase into a comparison operator.

```text
# Bad — paraphrase drops the boundary:
"overload fires when SF < 1.0"

# Good — mirror the code:
"overload fires when `safety.filter(|s| !s.is_safe())` yields `Some`
(i.e. SF ≤ 1.0, because `is_safe()` is `self.0 > 1.0`)"
```

### Rule 2 — Boundary test with exact-equal inputs

Construct inputs that yield the predicate's exact boundary value in the
numeric type used (here f32), and assert the expected branch fires.

For f32 ratio-boundaries like `SF = 1.0 exactly`, pick inputs that divide
to 1.0 under IEEE-754: if `support_cap = std::f32::consts::PI` and
`peel_force = std::f32::consts::PI`, then `sf = PI/PI = 1.0` is exact
because both numerator and denominator have the same bit pattern.

```rust
#[test]
fn assess_at_sf_exactly_one_fires_overload() {
    // σ=1.0, r=1.0, N=1 → support_cap = π (f32 const); plate=0
    // peel = π → SF = 1.0 exact
    let assessment = SupportAnalyzer::assess(
        50, area(100.0), peel(std::f32::consts::PI),
        &resin_tensile(1.0), &supports_1mm(), &plate_zero(),
    );
    let sf = assessment.safety_factor.expect("nonzero force yields Some");
    assert!((sf.value() - 1.0).abs() < 1e-6);
    assert!(!sf.is_safe());
    assert!(assessment.overload.is_some(), "SF=1.0 must trigger (inclusive)");
}
```

The test is cheap, specific, and outlives any well-meaning future
"cleanup" that would replace `!is_safe()` with `< 1.0`.

## When to apply

- Any predicate-based branch extraction across module boundaries.
- Any refactor that preserves an `Option<T>.filter(...)` chain.
- Any prose in planning or review documents that paraphrases a comparison
  operator with a boundary.

Not needed for: strictly-less-than predicates where the boundary is
already excluded by both code and prose, or comparisons where the input
type is integer and the boundary is obvious.

## See also

- `resinsim/crates/resinsim-core/src/services/support_analyzer.rs`
  (test `assess_at_sf_exactly_one_fires_overload`)
- `resinsim/crates/resinsim-core/src/values/force.rs`
  (`SafetyFactor::is_safe` definition at `self.0 > 1.0`)
- Plan-review finding A-MED1 in the `supportanalyzer-restoration` issue
  history (v1 rejection reason)
