---
issue: t1f4
date: 2026-04-17
---

# Anti-pattern: Rust NaN passes <= 0 validation

## Problem

In Rust, all comparisons involving NaN return `false`:

```rust
f32::NAN <= 0.0  // false — NaN "passes" a positive-only check
f32::NAN > 0.0   // false — NaN is also not positive
```

A constructor that only checks `if value <= 0.0 { return Err }` silently
accepts NaN. If that NaN propagates to a mathematical function (`ln`, `/`,
`*`), the result is also NaN, and any downstream `>= x` or `is_sufficient`
check then silently fails (returning false) instead of erroring.

In resinsim, this masked a correctness failure as a physics failure:
`cure_depth.is_sufficient()` returned `false` for NaN depth —
indistinguishable from a genuinely undercured layer.

## Fix

Pair the range check with an `is_finite()` check:

```rust
// Wrong — NaN passes
if value <= 0.0 { return Err("must be positive") }

// Correct — NaN, INFINITY, NEG_INFINITY all rejected
if !value.is_finite() || value <= 0.0 {
    return Err("must be positive and finite")
}
```

## Where this bites

Any domain value type with a positive-only invariant:
- `Energy::new` (fixed T1-F4)
- `PenetrationDepth::new` (fixed T1-F4)
- Any future `new(f32)` constructor with a range check

## See also

- T1-F4: Energy and PenetrationDepth fixed
- `docs/patterns/anti/debug-assert-as-release-guard.md` (related bypass path)
