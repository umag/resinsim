---
issue: repos-placement-cleanup
date: 2026-04-25
---

# Anti-pattern: External validation of an aggregate's children

## What it looks like

```rust
// In SimulationRepository::load — outside the aggregate
let sim: PrintSimulation = serde_json::from_str(&contents)?;
sim.recipe().validate()?;       // <-- reaches into aggregate
sim.printer().validate()?;      // <-- reaches into aggregate
// What about sim.layers()? sim.failures()? Is order checked? Maybe?
Ok(sim)
```

## Why it is wrong

The repository becomes responsible for knowing **which** of the aggregate's
children have invariants and **how** to call them. Two specific failure
modes:

1. **Silent gaps when the aggregate gains a child.** If a future
   refactor adds `material_lot: MaterialLot` (with its own validate())
   to PrintSimulation, the repository keeps compiling and silently fails
   to validate the new child. The bug surfaces only when a tampered file
   slips through.

2. **Aggregate encapsulation broken.** The repository's `load` is now a
   second source of truth for "what valid PrintSimulation means". Every
   other deserializing caller (Bevy viz, snapshot tools, maybe a future
   IPC layer) must replicate the same external validation. Drift is
   inevitable.

3. **Invariants the children don't own get skipped.** Layer-index
   sequentiality is an aggregate-level invariant (between layers, not
   within one layer). External per-child validation has no place to put
   it without smuggling aggregate-aware logic into the repository.

## Correct form

The aggregate exposes **one** method that re-runs every invariant the
aggregate would enforce at construction:

```rust
// On PrintSimulation:
pub fn validate(&self) -> Result<(), String> { ... }
```

The repository calls it and propagates the error:

```rust
let sim: PrintSimulation = serde_json::from_str(&contents)?;
sim.validate()
    .map_err(|e| format!("invalid simulation {}: {e}", path.display()))?;
Ok(sim)
```

When a new child gets added to the aggregate, the aggregate's `validate()`
gains the corresponding check. The repository code does not change. Every
deserializing caller automatically inherits the new check.

## Signal that you've hit this pattern

- A repository / loader reads private aggregate state through
  `pub fn child_a()` / `pub fn child_b()` accessors **only** to call
  `child.validate()`
- Two or more places call `aggregate.child_x().validate()` from outside
- Changes to the aggregate's composition silently leave one consumer
  un-updated
- Code review keeps surfacing "should we also validate the new child
  here?" — the friction is the smell

## How this surfaces in plan review

The plan-review v1 round of `repos-placement-cleanup` proposed exactly
this anti-pattern: "load() will call recipe.validate() and
printer.validate()" — without an aggregate-level validate(). The HIGH
adversarial finding caught the implementability problem (private fields)
which surfaced the structural issue. The pivot to a single
`PrintSimulation::validate()` was Phase A additive (no caller signature
changes) and resolved both concerns.

## Related

- `aggregate-validate-as-deserialize-bypass-guard.md` — the positive
  pattern this anti-pattern points at.
- `aggregate-shape-matches-docstring-contract.md` — when the aggregate's
  composition is wrong, no amount of external validation papers over it.
- ADR-0009 — the ADR that codified the placement rule and introduced
  PrintSimulation::validate() as the single source of truth.
