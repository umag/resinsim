---
issue: t1f6
date: 2026-04-18
---

# Pattern: Parse-path invariant regression (three-case shape)

## When to apply

You are adding a field via `#[serde(default = "fn_name")]` to a type that
participates in a cross-field invariant (ordering, mutual-consistency,
sum-to-something). Legacy serialised forms of the type exist and must
continue to deserialise. The invariant is enforced by a separate
`validate()` function, not by serde itself.

## The three cases

For each such field, write three unit tests exercising the parse path
(`serde::from_str` → `validate()`), not the field-mutation path:

1. **Full-missing.** All invariant-participating fields absent. Assert
   each field equals the documented default. Assert `validate()` returns
   Ok (the defaults must, by construction, satisfy the invariant).
2. **Partial-missing.** One field absent, the other explicit at a value
   that preserves the invariant against the default. Assert the default
   is applied to the absent field and the explicit value is preserved.
   Assert `validate()` returns Ok.
3. **Invariant-crossing.** One field absent, the other explicit at a
   value that VIOLATES the invariant when combined with the default.
   Assert `validate()` returns Err citing both fields by name.

## Example (T1-F6, `ResinProfile::{degradation_temp_c, min_safe_temp_c}`)

```rust
#[test]
fn legacy_toml_invariant_crossing_rejected_by_validate() {
    // min_safe_temp_c = 55.0 explicit; degradation_temp_c absent,
    // serde-applied default = 50.0. 55 > 50 violates the strict-less-than
    // ordering invariant enforced by validate().
    let toml_str = format!("{}\nmin_safe_temp_c = 55.0\n", baseline());
    let p: ResinProfile = toml::from_str(&toml_str).unwrap();
    let err = p.validate().expect_err("ordering violated");
    assert!(err.contains("min_safe_temp_c") && err.contains("degradation_temp_c"));
}
```

## Why all three matter

- **Full-missing** alone locks the default VALUE but not the invariant
  that connects defaults.
- **Partial-missing** alone covers one leg of a two-leg ordering check
  but not the cross between explicit + defaulted.
- **Invariant-crossing** is the only case that catches the specific
  regression: "someone changed default_X in a way that, combined with a
  legacy TOML's explicit Y, silently crosses an ordering boundary".

Without case 3, a future default-value tightening can ship without any
test failing, and legacy files silently start failing validation at
load time in production.

## Related

- [nan-two-layer-defence](nan-two-layer-defence.md) (T1-F4) — the
  per-field finiteness pattern this one composes with for multi-field
  invariants
- [adr/0002-option-not-sentinel-for-absent-values](../adr/0002-option-not-sentinel-for-absent-values.md)
  (T1-F2) — the Option-vs-sentinel decision for single-field absence
  semantics
