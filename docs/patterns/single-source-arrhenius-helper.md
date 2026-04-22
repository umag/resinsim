---
issue: recipe-aware-time-and-thermal
date: 2026-04-22
---

# Pattern: Single-source helper for formulas consumed by service + CLI

## Context

The Arrhenius Ec(T) correction (KB-153) is consumed by two call sites:

1. `CureCalculator::cure_depth_at_temp` — the simulator's per-layer cure-depth
   path inside `FailurePredictor::predict_layer`.
2. `cmd_thermal` in `resinsim-inspect` — renders the Ec(T) column of the
   user-facing thermal sample table.

In the initial step-10 implementation, both sites independently computed
`Ec(T) = Ec_ref × exp((Ea/R) × (1/T - 1/T_ref))` — one in
`cure_calculator.rs:72-77`, the other as hand-rolled math at
`main.rs:779-784`. Round-1 code review flagged the duplication as MEDIUM: a
future change to the formula (sign convention, R precision, unit conversion)
must land in both places or the CLI diverges from the simulator — the exact
failure mode the v7 plan review caught in a sign-flip scenario at a different
callsite.

## Pattern

Extract a public helper on the service type that returns the intermediate
value. Use it from both the downstream service method and any
presentation-layer renderer that needs the same number.

```rust
// services/cure_calculator.rs
impl CureCalculator {
    pub fn ec_at_temp(
        ec_ref: Energy,
        ref_temp_c: f32,
        vat_temp: VatTemperature,
        ea_cure_kj_mol: f32,
    ) -> Energy {
        // Single source of truth for the Arrhenius formula.
    }

    pub fn cure_depth_at_temp(dp: PenetrationDepth, energy: Energy, /* ... */) -> CureDepth {
        let ec = Self::ec_at_temp(ec_ref, ref_temp_c, vat_temp, ea_cure_kj_mol);
        Self::cure_depth(dp, energy, ec)  // delegate to the base formula
    }
}
```

```rust
// resinsim-inspect/src/main.rs cmd_thermal
let ec_t = CureCalculator::ec_at_temp(ec_ref, ref_temp_c, vat, ea_cure);
```

A formula sign flip, unit-conversion bug, or constant refinement now lands
once.

## When to use

- A formula appears in 2+ callsites, even if one is a "display-only"
  presentation path.
- The formula embeds physical constants or convention choices (sign, unit)
  whose drift between sites would be a silent correctness bug rather than a
  compile error.
- The intermediate value (here: `Ec(T)`) has standalone semantic meaning —
  not just an anonymous sub-expression.

## When NOT to use

- One-off inlined math that isn't used elsewhere — extracting a helper adds
  indirection without dedup.
- Formula bodies that diverge subtly between sites by design (e.g.
  approximation in the hot path, exact in the batch path); naming them
  distinctly is clearer than one helper with branching.

## Consequences

- The extracted helper must be public (not `pub(crate)`) if the CLI crate
  consumes it — be explicit about the cross-crate surface.
- Signature choice matters: accept the typed value at the boundary
  (`VatTemperature`), not raw `f32`, so argument-order swaps with sibling
  temperatures are compile errors (see
  `docs/patterns/typed-temperature-boundary.md`).

## Testing

One unit test at the helper asserting the formula's self-consistency
(e.g. `ec_at_ref_temp == ec_ref`), plus property tests that exercise the
helper via the delegating service method so regressions in either layer
surface (see `tests/cure_properties.rs::ec_arrhenius_symmetric_in_inverse_temp`
which inverts `Cd = Dp × ln(E/Ec)` to extract `Ec(T1) × Ec(T2) = Ec_ref²` —
a cross-layer integration test that would fail if either the helper or the
delegation drifted).

## See also

- `crates/resinsim-core/src/services/cure_calculator.rs` — `ec_at_temp`
  + `cure_depth_at_temp` implementation.
- `crates/resinsim-inspect/src/main.rs` — `cmd_thermal` consumer.
- `kb/KB-153-cure-kinetics-temperature.md` — the physics reference.
- `docs/adr/0007-led-and-vat-as-separate-temperatures.md` — the issue
  lifecycle that drove the extraction.
