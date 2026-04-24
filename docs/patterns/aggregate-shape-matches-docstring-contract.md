---
issue: print-time-on-reportgenerator
date: 2026-04-25
---

# Pattern: Aggregate shape should match its docstring contract

## Context

A DDD aggregate root's docstring documents what it owns — typically a
phrase like "Aggregate root: a complete X for one Y, Z, and W". When the
struct's fields don't match those promises, projections on the aggregate
end up taking the missing entities as parameters every call. The
discrepancy is usually invisible at planning time because the docstring
reads correctly and the struct compiles correctly, but it surfaces as
ergonomic friction every time a new projection is added.

## Concrete example

`PrintSimulation` was originally:

```rust
/// Aggregate root: a complete simulation run for one geometry + resin + printer.
pub struct PrintSimulation {
    layers: Vec<LayerResult>,
    failures: Vec<FailureEvent>,
}
```

The docstring promises ownership of "geometry + resin + printer" but the
struct only holds `layers + failures`. When the `print-time-on-reportgenerator`
lifecycle needed to add a time projection that depends on Recipe +
PrinterProfile, the v3 plan proposed parameter injection:

```rust
pub fn summary(&self, recipe: &Recipe, printer: &PrinterProfile) -> SimSummary
```

The v3 plan-review autonomous loop exited clean with no blocking findings,
but the architectural concession was visible in `potentialChallenges` —
"follow-up ADR consideration on PrintSimulation aggregate shape". The
human pivoted to v4 which reshaped the aggregate:

```rust
pub struct PrintSimulation {
    recipe: Recipe,            // <- now owned
    printer: PrinterProfile,   // <- now owned
    layers: Vec<LayerResult>,
    failures: Vec<FailureEvent>,
}

pub fn new(recipe: Recipe, printer: PrinterProfile) -> Self { ... }
pub fn summary(&self) -> SimSummary { /* reads self.recipe + self.printer */ }
```

`impl Default` removed (no sensible default). Nine call sites updated by
the compiler. `summary()` reverted to arg-less. Future projections on
`PrintSimulation` (force-profile stats, temperature-history stats) stay
arg-less and don't replay the parameter-threading burden.

## Signal

When proposing a projection on an aggregate that depends on domain
entities the aggregate doesn't currently hold, ask:

1. Does the aggregate's docstring promise ownership of those entities?
2. If yes — is the struct's field list consistent with the docstring?
3. If the docstring is right but the struct lags behind, **fix the
   struct** (smaller blast radius today than every future projection's
   tax).

If the docstring is wrong (the aggregate genuinely shouldn't own those
entities), update the docstring AND keep parameter injection as the
projection's input mechanism.

The trap to avoid: accepting parameter injection as a "local fix" that
papers over the aggregate-shape gap. Each subsequent projection
reinforces the wrong shape until the cost of the pivot becomes
prohibitive.

## When to defer the pivot

Sometimes the aggregate-reshape work is genuinely out of scope for the
current lifecycle (e.g., the project is mid-migration, or the
aggregate's identity is in flux). In that case:

- File a follow-up ADR explicitly proposing the reshape.
- Accept parameter injection for THIS lifecycle.
- Surface the deferred decision in the plan's potentialChallenges so
  reviewers see it.

The `print-time-on-reportgenerator` lifecycle initially took this path
in v3 then pivoted in v4 — the pivot was cheap because the lifecycle
was still pre-implementation. A post-implementation pivot would have
cost more (every adapted call site, every test fixture).

## Related

- ADR-0001 — values must not import entities. Note this is NOT a
  "projections live in the domain" rule; it's narrower. Don't cite
  ADR-0001 to justify aggregate-shape decisions.
- [phase-boundaries-for-ddd-refactors](phase-boundaries-for-ddd-refactors.md)
  — Phase A (additive) vs Phase B (switchover) framing for aggregate
  reshapes.
- `crates/resinsim-core/src/simulation/print_simulation.rs` — the
  reshaped aggregate.
