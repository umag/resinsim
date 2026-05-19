---
issue: t2f1.5-voxel-cleanup
date: 2026-05-19
---

# Pattern: Poison-as-witness for short-circuit tests

## Problem

Code has a short-circuit:

```rust
pub fn cure_depth_summary_for_resin(&self, layer_index: u32, resin: &ResinProfile) -> Option<CureDepth> {
    let layer = self.layers.get(layer_index as usize)?;
    if self.cure_field.is_none() {
        return CureDepth::new(layer.cure_depth_um).ok();
    }
    // Expensive Ec(T) compute + dispatch...
    let ec_t = Self::ec_t_for_layer(layer, resin);
    Some(layer.cure_depth_um_summary(self, resin.penetration_depth_um(), ec_t.value()))
}
```

How do you prove the short-circuit ran via a behavioural test? Counting
function calls doesn't translate naturally to Rust. Asserting the
return value alone doesn't distinguish "short-circuit returned cached"
from "Ec(T) path computed the same value".

## Technique

Arrange the fixture so the expensive path would PANIC if it ran. Then
the test passing IS the witness that the short-circuit fired.

```rust
#[test]
fn cure_depth_summary_for_resin_tier1_skips_ec_t_compute() {
    let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
    let mut layer = make_layer(0, 5.0, 3.0, 22.0);
    layer.cure_depth_um = 87.5;
    layer.vat_temperature_c = f32::NAN;  // <-- POISON: would panic Ec(T) compute
    sim.add_layer(layer, vec![]).expect("...");

    let resin = ResinProfile::generic_standard();

    let cd = sim
        .cure_depth_summary_for_resin(0, &resin)
        .expect("Tier-1 mode must return Some(cached) for in-bounds layer");
    assert_eq!(cd.value(), 87.5,
        "Tier-1 path must return the cached cure_depth_um verbatim");
}
```

If the short-circuit fires: `cure_field.is_none()` → return
`CureDepth::new(87.5)` directly. NAN is never read. Test passes.

If the short-circuit doesn't fire: `ec_t_for_layer` runs,
`VatTemperature::new(NAN).expect(...)` panics. Test FAILS loudly.

## Why this works

- The poison value is chosen to violate the downstream code's
  precondition (here: `VatTemperature::new` rejects non-finite).
- The downstream code uses `.expect` (per ADR-0003), so violating the
  precondition is a panic, not a silent failure.
- The test's assertion only passes via the short-circuit return.

## When to use

- A short-circuit exists for performance, NOT correctness.
- The skipped path has a clear "precondition would panic if violated"
  call to use as the witness.
- The poison value can be set on the fixture without changing the
  short-circuit's condition (here: `vat_temperature_c` is independent
  of `cure_field`'s None-ness).

## When not to use

- The short-circuit is for correctness — if the skipped path produces
  a different value, assert THAT directly (no poison trick needed).
- No precondition-violating value exists for the skipped path — fall
  back to a value-comparison test against a hand-computed expected.

## Documentation

The test body MUST include a docstring explaining the witness
technique — without it, a future reader sees a NAN in a test fixture
and assumes it's an unrelated edge case or a stale leftover. The
docstring is the load-bearing link between the poison value and the
short-circuit contract under test.

## Related

- ADR-0003 (unwrap-policy) — `.expect` justifications make this
  pattern possible (the panic message names the violated invariant).
- `decomposition-invariant-for-result-structs.md` — related "test
  the path, not just the result" theme.
