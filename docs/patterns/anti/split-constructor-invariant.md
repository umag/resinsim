---
issue: t1f2
date: 2026-04-17
---

# Anti-pattern: Split invariant across constructors

## What it looks like

```rust
impl SafetyFactor {
    pub fn new(ratio: f32) -> Result<Self, String> {
        if !ratio.is_finite() {
            return Err(...); // rejects INFINITY
        }
        Ok(Self(ratio))
    }

    pub fn compute(capacity: SupportCapacity, force: PeelForce) -> Self {
        if force.0 <= 0.0 {
            return Self(f32::INFINITY); // produces INFINITY
        }
        Self(capacity.0 / force.0)
    }
}
```

## Why it is wrong

`new()` and `compute()` disagree on valid values. A `SafetyFactor` produced by
`compute()` cannot be round-tripped through `new()`. The type's invariant depends
on which constructor was used — impossible to reason about in isolation.

## Signal that you've hit this pattern

- One constructor rejects a value (NaN, INFINITY, negative)
- Another constructor can produce that same value as a sentinel
- The doc comment on the permissive constructor says "this is needed because..."

## Correct form

```rust
pub fn compute(capacity: SupportCapacity, force: PeelForce) -> Option<Self> {
    if force.0 <= 0.0 { return None; }
    Some(Self(capacity.0 / force.0))
}
```

Use `Option` instead of a sentinel. Both constructors agree: `SafetyFactor` is always
finite and non-negative.
