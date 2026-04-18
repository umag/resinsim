---
issue: t1f5
date: 2026-04-18
---

# Pattern: Entity field encapsulation with validate-on-mutation contract

## Context

Domain entities in resinsim-core (e.g. `ResinProfile`) define a
`validate() -> Result<(), String>` method that enforces physical invariants
(NaN/inf rejection, range checks, cross-field consistency). Construction
paths (factory methods, TOML deserialisation via repositories) call
`validate()` before returning, guaranteeing every freshly-constructed
profile is trusted.

If the entity's fields are `pub`, external code can mutate a previously-valid
profile to invalid state without re-running `validate()`. Downstream services
that rely on "every `ResinProfile` instance has passed validate()" then
silently consume invalid state, producing misdiagnosed physics output.

## Pattern

Apply two layers of encapsulation:

**Layer 1 — Restrict the mutation surface.** Make every field `pub(crate)`
so external code cannot construct via struct literal or mutate via field
assignment. External read access (where needed) is exposed via getters.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResinProfile {
    pub(crate) name: String,
    pub(crate) penetration_depth_um: f32,
    // ... 10 more pub(crate) fields
}

impl ResinProfile {
    /// Resin profile identity (used for display + matching by name).
    pub fn name(&self) -> &str {
        &self.name
    }
    // factory methods, predicates, validate()
}
```

**Layer 2 — Document the validate-on-mutation contract.** Add a `///` doc
comment on the struct AND on `validate()` that explicitly states: any
intra-crate code mutating a field of a previously-validated profile MUST
re-call `validate()` before passing the profile to a downstream service.

```rust
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` — external code cannot construct or mutate.
/// Construction is restricted to the factory methods on this type and to
/// TOML deserialisation via the repository, both of which run `validate()`
/// before returning. After any field mutation by intra-crate code
/// (typically tests), `validate()` MUST be re-called before treating the
/// profile as trusted by downstream services.
pub struct ResinProfile { /* ... */ }
```

**Layer 3 (defence-in-depth) — Re-validate at service entry.** Existing
service entry points (e.g. `simulation_runner::run_*`) already call
`profile.validate()` at the start of execution. This catches any contract
violation at runtime even if intra-crate code forgets to re-validate.

## When to use

- Any entity with a non-trivial `validate()` method enforcing cross-field
  invariants
- Any entity whose fields are read by external crates (workspace siblings)
  but should never be mutated externally
- Any entity that is loaded from untrusted external data (TOML, JSON, user
  input) where construction-time validation is the trust boundary

## Testing

- Add a contract-demonstration test that mutates a previously-valid profile
  to invalid state (using a mutation shape distinct from "construct invalid"
  tests) and asserts `validate()` returns `Err`. Include a comment
  cross-referencing the struct's doc comment so the test is not deleted as
  apparent duplicate.
- Verify all existing intra-crate callers compile (most just read fields —
  unaffected by `pub(crate)`).
- Verify external crate consumers compile after swapping `entity.field` →
  `entity.field()` accessor calls.

## See also

- T1-F5: full implementation on `ResinProfile`
- T1-F7: sibling refactor for `PrinterProfile` (filed during T1-F5 plan review)
- `docs/patterns/nan-two-layer-defence.md` — the value-layer counterpart
  (constructor + service-entry guard for value types)
- `docs/adr/0001-ddd-layer-dependency-rule.md` — entities sit between
  values and services; entities own their invariants
