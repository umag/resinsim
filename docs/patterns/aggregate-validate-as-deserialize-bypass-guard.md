---
issue: repos-placement-cleanup
date: 2026-04-25
---

# Pattern: Aggregate-level `validate()` as deserialize-bypass guard

## Context

`#[derive(Serialize, Deserialize)]` is the path of least resistance for
persisting aggregates. The cost: deserialization reconstructs the struct
field-by-field, completely bypassing `pub fn new()`, `pub fn add_X()`,
and any other constructor / mutator that owns the invariants.

For an aggregate like `PrintSimulation`:

```rust
pub fn new(recipe: Recipe, printer: PrinterProfile) -> Self { ... }

pub fn add_layer(&mut self, result: LayerResult, ...) {
    let expected = self.layers.len() as u32;
    assert_eq!(result.index, expected, "layers must be sequential");
    ...
}
```

`PrintSimulation::new` doesn't run `Recipe::validate()` /
`PrinterProfile::validate()` — those are checked at TOML-load time by
the entity repos. `add_layer` enforces sequentiality, but only when
called. After `serde_json::from_str(&untrusted_json)`:

- Child entity invariants are untrusted (no validate() called)
- Layer indices are untrusted (no add_layer called; raw `Vec` deserialized)
- The aggregate looks valid to the type system but its semantics are not
  checked

## Pattern

Add `pub fn validate(&self) -> Result<(), String>` on the aggregate.
Implement it as the union of every invariant the aggregate's constructors
+ mutators would have enforced:

```rust
pub fn validate(&self) -> Result<(), String> {
    self.recipe
        .validate()
        .map_err(|e| format!("recipe: {e}"))?;
    self.printer
        .validate()
        .map_err(|e| format!("printer: {e}"))?;
    for (i, layer) in self.layers.iter().enumerate() {
        let expected = i as u32;
        if layer.index != expected {
            return Err(format!(
                "layer index mismatch at position {i}: expected {expected}, got {}",
                layer.index
            ));
        }
    }
    Ok(())
}
```

The repository calls validate() once after deserialize and rejects any
aggregate that fails:

```rust
pub fn load(&self, name: &str) -> Result<PrintSimulation, String> {
    // ... read + parse ...
    sim.validate()
        .map_err(|e| format!("invalid simulation {}: {e}", path.display()))?;
    Ok(sim)
}
```

## Why this beats the alternatives

- **Public child accessors** — `pub fn recipe(&self)` etc. — would let
  the repository call `sim.recipe().validate()` directly, but the
  aggregate then leaks its internals to every external caller, not just
  the repository. DDD encapsulation broken. See
  `anti/external-aggregate-child-validation.md`.
- **`#[serde(try_from = "Raw")]`** — feasible but introduces a separate
  `Raw` type that mirrors the aggregate exactly. More moving parts,
  same end state.
- **Validate at every aggregate read** — performance cost, doesn't compose
  with non-deserialize construction paths.

## When to use it

Apply this pattern when ALL of:

1. The aggregate derives `Deserialize` (or otherwise has a non-`new()` path
   to reach a populated state)
2. The aggregate's children have their own `validate()` and the aggregate
   constructor doesn't call them
3. The aggregate's mutators enforce invariants that `Deserialize` bypasses
   (sequential indices, monotonic timestamps, parent-child link consistency)

If only (1) holds, child entities have no validate() of their own, and the
constructor doesn't enforce anything beyond field types — the pattern is
overkill. Type checks are sufficient.

## Test coverage

For each invariant validate() enforces, write one unit test that:

1. Builds a clean aggregate
2. Round-trips through `serde_json::to_value` / `from_value`
3. Mutates the JSON Value to violate the specific invariant
4. Deserializes (must succeed — schema is intact)
5. Calls validate() and asserts Err with a message identifying the
   violating field/position

This is what `print_simulation.rs::tests::validate_*` cover for
PrintSimulation. The repository's own tests can then assume validate()
works and just check load() actually calls it (one test per repository).

## See also

- `entity-validate-on-mutation.md` — the entity-level analogue. Aggregates
  compose entities; this pattern is the next layer up.
- `aggregate-shape-matches-docstring-contract.md` — when the aggregate
  doesn't own the entities it claims to own, validate() can't enforce
  what's missing. Fix the shape first.
- `anti/external-aggregate-child-validation.md` — the negative form.
- ADR-0009 — the ADR that introduced this guard for `PrintSimulation`
  alongside `SimulationRepository`.
