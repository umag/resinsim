---
issue: t1f4
date: 2026-04-17
---

# Anti-pattern: Using debug_assert! as the only release-mode guard

## Problem

`debug_assert!` is compiled out in release builds (`cfg(not(debug_assertions))`).
If it's the only protection against an invalid input, production code is
unguarded. Tests pass (debug mode) while production can silently receive and
propagate invalid values.

```rust
// Only fires in debug builds — production is unguarded
pub fn scale(&self, factor: f32) -> Self {
    debug_assert!(factor > 0.0 && factor.is_finite(), "...");
    Self(self.0 * factor)  // In release: silently returns NaN if factor is NaN
}
```

This is especially dangerous when the method bypasses a validated constructor
(like `Energy::new`), since there's no other opportunity to catch the invalid
value before it enters the domain.

The live production path in T1-F4:
`uniformity_calculator::cure_depth_at_position → nominal_energy.scale(factor)`

## Fix

Use `assert!` for invariants that must hold in production:

```rust
pub fn scale(&self, factor: f32) -> Self {
    assert!(factor > 0.0 && factor.is_finite(),
            "scale factor must be positive and finite, got {factor}");
    Self(self.0 * factor)
}
```

Reserve `debug_assert!` for:
- Performance-sensitive inner loops where the invariant is guaranteed by the
  calling code and the check would be measurably expensive in release
- Cross-cutting consistency checks (e.g. sorted-order invariants) where a
  violation causes a crash anyway

## Rule of thumb

If a `debug_assert!` failure in production would produce a silent wrong result
(NaN, garbage data, missed event) rather than a crash, use `assert!` instead.

## See also

- T1-F4: `Energy::scale` fixed from `debug_assert!` to `assert!`
- `docs/patterns/anti/rust-nan-positive-validation-gap.md` (root NaN entry point)
