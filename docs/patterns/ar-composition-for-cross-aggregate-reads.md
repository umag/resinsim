---
issue: t2f1.5-voxel-cleanup
date: 2026-05-19
---

# Pattern: AR-level composition for cross-aggregate reads

## Context

A domain operation needs data from two (or more) aggregates. The naive
placement — a method on a child entity that takes both aggregates as
`&` parameters — compounds two Demeter violations: the child reaches
OUT to its own parent's state, AND across to a sibling aggregate.

`PrintSimulation` (aggregate root) owns `Recipe + PrinterProfile` and
a `Vec<LayerResult>` + `Option<CureField>`. `ResinProfile` is a SEPARATE
aggregate (loaded by `ResinRepo`). Computing per-layer cure depth needs:

- `LayerResult.vat_temperature_c` (child of PrintSimulation)
- `PrintSimulation.cure_field` (parent aggregate state)
- `ResinProfile.{critical_energy, penetration_depth, reference_temp_c, ...}` (other aggregate)

## Anti-shape (rejected)

```rust
impl LayerResult {  // <-- child entity
    pub fn cure_depth_summary_for_resin(
        &self,
        sim: &PrintSimulation,   // reaches OUT to parent
        resin: &ResinProfile,    // reaches across to sibling AR
    ) -> CureDepth { ... }
}
```

Awkward call site: `sim.layers()[5].cure_depth_summary_for_resin(&sim, &resin)`
— passing `sim` again after navigating from it.

## Correct shape

```rust
impl PrintSimulation {  // <-- aggregate root
    pub fn cure_depth_summary_for_resin(
        &self,
        layer_index: u32,
        resin: &ResinProfile,  // read-only query input
    ) -> Option<CureDepth> {
        let layer = self.layers.get(layer_index as usize)?;
        // ... compose CureCalculator::ec_at_temp + layer primitive
    }
}
```

Properties:

- The AR navigates to its own child (no Demeter violation).
- `ResinProfile` is consumed as a read-only query parameter (value-like
  input, not a mutation cross-aggregate).
- OOB layer index returns `None` — Option semantics, no panic.
- Tier-1 short-circuit (no voxel field) returns the cached scalar
  without composing — zero-overhead non-feature path.

## When to use

- A behaviour that's naturally phrased as "what's X for this aggregate
  given Y from another aggregate?" — make it an AR method.
- The other aggregate is read-only (a `&` borrow) — query input, not
  state transfer.
- The behaviour fundamentally requires data from both aggregates — if
  it only reads from one, place it on that aggregate's root or service.

## When not to use

- The cross-aggregate operation needs to mutate the other aggregate
  (then it's a use-case / application service, not an AR method).
- The composition is generic enough that no specific aggregate "owns"
  it — domain service (e.g. `CureCalculator::ec_at_temp`).

## Related

- `phase-boundaries-for-ddd-refactors.md` — sequencing for refactors
  that move methods between layers.
- `aggregate-shape-matches-docstring-contract.md` — whether the
  aggregate's OWNED data matches its docstring promises (different
  concern from method placement).
