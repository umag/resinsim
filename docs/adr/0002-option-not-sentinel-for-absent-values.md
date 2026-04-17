---
issue: t1f2
date: 2026-04-17
---

# ADR-0002: Use Option<T>, not sentinel values, for absent domain values

## Status
Accepted

## Context
`SafetyFactor::compute()` returned `f32::INFINITY` when peel force was zero (no load).
`SafetyFactor::new()` rejected `f32::INFINITY`. This created a split invariant: a value
could only be constructed via `compute()` — not round-tripped through `new()`.

## Decision
When a domain value may be absent (not applicable, not yet computed, undefined input),
represent it as `Option<DomainType>` rather than a magic sentinel (INFINITY, -1, NaN).

At output boundaries (data structs, JSON, serialised `LayerResult`), convert `Option`
to a concrete `f32` using a documented sentinel only if the downstream consumer already
handles that sentinel (see LayerResult exception below).

## Consequences
- `SafetyFactor` is now always finite when constructed — type invariants are self-consistent.
- Callers that previously checked for INFINITY must now pattern-match on `Option`.
- `compute()` returning `None` for zero force is explicit and unsurprising.

## LayerResult exception
`LayerResult.safety_factor` remains `f32`. Zero-force layers store `f32::INFINITY` at
the output boundary because `print_simulation.rs` already uses `f32::INFINITY` as its
min-SF accumulator seed. This is a deliberate boundary decision, not a violation of
this ADR.
