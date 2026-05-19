---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# Pattern: behaviour-on-value-object (DDD)

## Context

When a function derives its result purely from a value object's state
+ a small amount of extra context (a name, a flag), the function
belongs on the value object as a method — not on an application
service. The application service should route, not own the logic.

The `ctb-layer-height-authority` round-1 code review caught this:
`format_layer_height_warning` and `render_text_summary` originally
lived on `SimulationRunner` because they were called from the
runner. They moved to `LayerHeightProvenance::format_warning` /
`render_text_summary` once the review noted both functions read only
from the provenance + a profile name.

## The pattern

If you find yourself writing:

```rust
impl SomeRunner {
    pub fn format_warning_for(value: &MyValue, name: &str) -> Option<String> {
        let detail = value.mismatch.as_ref()?;
        // ... wording logic derived entirely from `value` + `name` ...
        Some(format!(...))
    }
}
```

... move it to:

```rust
impl MyValue {
    pub fn format_warning(&self, name: &str) -> Option<String> {
        let detail = self.mismatch.as_ref()?;
        // ... wording logic ...
        Some(format!(...))
    }
}
```

The runner becomes:

```rust
impl SomeRunner {
    fn emit_warning_if_present(value: &MyValue, name: &str) {
        if let Some(text) = value.format_warning(name) {
            eprintln!("{text}");
        }
    }
}
```

## Why

- **Tests live near the data.** Unit tests for `format_warning` go
  in the value object's module, not the runner's. Both surfaces
  (the data invariants and the wording invariants) are exercised
  in one place.
- **I/O stays at the boundary.** The pure formatter returns
  `Option<String>`; the runner's job is to decide whether and where
  to emit (stderr, log, JSON). Pure-vs-impure separation falls out
  of this naturally.
- **Refactors don't cross modules.** Adding a new variant to the
  value object's discriminator (e.g. `MismatchKind::Variable`)
  changes one file — the value object's module. The runner doesn't
  notice.
- **DDD consistency.** Behaviour belongs with the entity / value
  object it acts on. ADR-0001 (DDD layer dependency rule) implies
  this; this pattern is the concrete shape of "implies" for
  formatter / projector functions.

## When NOT to use

- The behaviour needs OTHER value objects to compute its result —
  then it's a domain service, not a method on one of them.
- The behaviour is I/O (talks to disk, network, the simulation
  runner). Keep I/O at the application boundary.
- The behaviour is a CLI surface convention (CLI message styling,
  exit-code mapping). That stays in the CLI module.

## See also

- ADR-0001 — DDD layer dependency rule
- `docs/patterns/phase-boundaries-for-ddd-refactors.md`
- `crates/resinsim-core/src/values/layer_height_provenance.rs` —
  `format_warning` + `render_text_summary` methods are the reference
  shape
