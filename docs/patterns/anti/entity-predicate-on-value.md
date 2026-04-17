---
issue: t1f1
date: 2026-04-17
---

# Anti-pattern: Entity predicate on value type

## What it looks like

```rust
// Values layer (BAD)
impl VatTemperature {
    pub fn is_degradation_risk(&self, profile: &ResinProfile) -> bool {
        self.0 > profile.degradation_temp_c
    }
}
```

## Why it is wrong

1. The predicate's threshold (`degradation_temp_c`) lives on the entity.
   Accessing it from the value layer inverts the dependency direction.
2. It duplicates logic that belongs on the entity, creating two authoritative
   sources for the same invariant.
3. In Rust, the values layer importing the entities crate creates a circular
   module dependency that can break compilation and definitely breaks DDD layering.

## Correct form

```rust
// Entities layer (CORRECT)
impl ResinProfile {
    pub fn is_degradation_risk(&self, vat_temp: VatTemperature) -> bool {
        vat_temp.value() > self.degradation_temp_c
    }
}
```

The entity owns its threshold data and the predicate over it. The value type
is just passed in as a parameter.

## Signal that you've hit this pattern

You are writing a method on a value type that takes an entity as a parameter
to compare against some threshold owned by that entity.
