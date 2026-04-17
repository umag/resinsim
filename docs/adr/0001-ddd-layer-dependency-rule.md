---
issue: t1f1
date: 2026-04-17
---

# ADR-0001: Values layer must not import Entities

## Status
Accepted

## Context
resinsim-core is structured in three layers: Values, Entities, Services.
The intended dependency direction is Services → Entities → Values (Values at the bottom).

During T1 implementation, `VatTemperature` (values) was given methods that took
`&ResinProfile` (entity) as a parameter, importing `crate::entities::ResinProfile`
inline. This created a values→entities circular dependency.

## Decision
The values layer (`crates/resinsim-core/src/values/`) must never import from
`crate::entities`. Domain predicates that require entity-owned threshold data
belong on the entity, not the value type.

## Consequences
- `VatTemperature::is_degradation_risk` and `is_too_cold` removed in T1-F1.
- Canonical versions live on `ResinProfile::is_degradation_risk(vat_temp)` and
  `ResinProfile::is_too_cold(vat_temp)`.
- CI will catch any future violation at compile time (circular imports = build error).
