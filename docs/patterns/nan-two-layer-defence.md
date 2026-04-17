---
issue: t1f4
date: 2026-04-17
---

# Pattern: Two-layer NaN defence for domain value types

## Context

Domain value types in resinsim-core (e.g. `Energy`, `PenetrationDepth`) wrap
`f32` and enforce invariants via `new()`. However, additional factory methods
(like `scale()`, `from_exposure()`) can produce instances that bypass `new()`.
If these methods contain only `debug_assert!` guards, release builds are
unprotected.

## Pattern

Apply two layers of defence:

**Layer 1 — Constructor** (`values/` layer): reject NaN and infinity in every
`new()` AND in every method that produces a new instance without going through
`new()`:

```rust
impl Energy {
    pub fn new(mj_cm2: f32) -> Result<Self, &'static str> {
        if !mj_cm2.is_finite() || mj_cm2 <= 0.0 {
            return Err("energy must be positive and finite");
        }
        Ok(Self(mj_cm2))
    }

    pub fn scale(&self, factor: f32) -> Self {
        assert!(factor > 0.0 && factor.is_finite(),  // runtime, not debug_assert!
                "scale factor must be positive and finite, got {factor}");
        Self(self.0 * factor)
    }
}
```

**Layer 2 — Service entry guard** (`services/` layer): assert at the entry
point of any service method that depends on the invariant:

```rust
pub fn cure_depth(dp: PenetrationDepth, energy: Energy, critical_energy: Energy) -> CureDepth {
    assert!(critical_energy.value() > 0.0 && critical_energy.value().is_finite(),
            "cure_depth: critical_energy must be positive and finite, got {}",
            critical_energy.value());
    assert!(energy.value() > 0.0 && energy.value().is_finite(), ...);
    // ... computation
}
```

## When to use

- Any service method that calls `.ln()`, `/`, or other operations that
  produce NaN from NaN inputs
- Any value type with a bypass factory method that doesn't go through `new()`
- When the consequence of a silent NaN is a misdiagnosed physics result
  (wrong value looks like valid domain state, e.g. `is_sufficient → false`)

## Testing

- Constructor rejection: `Energy::new(NaN) → Err`, `Energy::new(Inf) → Err`
- Bypass-method guard: `energy.scale(NaN)` → `#[should_panic]`
- Service guard via `unsafe { transmute }` to bypass the type system
  (documents the invariant; use sparingly and with a clear comment)

## See also

- T1-F4: full implementation of this pattern across Energy, PenetrationDepth, CureCalculator
- `docs/patterns/anti/rust-nan-positive-validation-gap.md`
- `docs/patterns/anti/debug-assert-as-release-guard.md`
