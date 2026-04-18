---
issue: t1f6
date: 2026-04-18
---

# Anti-pattern: Tuple construction bypasses value-object validation

## What it looks like

```rust
#[test]
fn safety_factor_below_one_is_failure() {
    let sf = SafetyFactor::compute(SupportCapacity(37.7), PeelForce(50.0)).unwrap();
    // ^^^^^^^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^^^^^^
    // tuple construction — bypasses PeelForce::new() and SupportCapacity::new()
    assert!(!sf.is_safe());
}
```

## Why it is wrong

`PeelForce::new(50.0)` runs finite + non-negative checks; `PeelForce(50.0)`
runs nothing. Tests encoded the latter — the bypassed form — as the
canonical contract. If `PeelForce::new` ever tightens (e.g. requires
strictly positive, rejects zero, adds a realistic upper bound), production
starts rejecting values that tests still produce. Tests pass; production
breaks on the next load.

The fact that the tuple form is syntactically legal within the defining
crate (because the field is module-private, not pub) is coincidental. It
is not a second constructor; it is the absence of a constructor.

## Signal that you've hit this pattern

- A value object has `pub fn new(x) -> Result<Self, _>` with validation
- Tests in the same module construct it as `TypeName(x)` to skip setup
- A separate suite of `_rejects_*` tests confirms `new()` validates,
  but those tests are ISOLATED from the semantic/display/physics tests
  that use tuple construction
- Grep shows display/Display/behaviour tests using `TypeName(literal)`
  and dedicated validation tests using `TypeName::new(literal)`

## Correct form

```rust
#[test]
fn safety_factor_below_one_is_failure() {
    let sf = SafetyFactor::compute(
        SupportCapacity::new(37.7).unwrap(),
        PeelForce::new(50.0).unwrap(),
    )
    .unwrap();
    assert!(!sf.is_safe());
}
```

Every fixture routes through `new()`. Any future tightening of `new()`
forces the test to be updated explicitly — either the fixture needs a
different value, or the test encodes a now-impossible scenario and must
be removed or refactored. The compiler becomes an ally in keeping tests
honest.

## Why `pub(crate)` fields are not a fix

Post-T1-F5/F7, most value-object fields are `pub(crate)` — external code
cannot do tuple construction, but intra-crate tests still can. The fix is
**discipline**, not a stronger visibility modifier: extending
`pub(crate)` to the struct's fields does not close this gap for unit
tests in the same module. Enforce by convention, verify by grep.

## Related

- [split-constructor-invariant](split-constructor-invariant.md) (T1-F2) —
  the case where two constructors disagree on validity
- [entity-validate-on-mutation](../entity-validate-on-mutation.md) (T1-F5) —
  the parallel pattern at the entity layer
