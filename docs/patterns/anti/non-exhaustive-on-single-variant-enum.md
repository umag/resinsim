---
issue: printsim-add-layer-result-api
date: 2026-04-26
---

# Anti-pattern: `#[non_exhaustive]` on single-variant error enums

## The mistake

When introducing a new typed error enum:

```rust
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]                              // <-- this
pub enum AggregateError {
    #[error("layers must be sequential: expected {expected}, got {got}")]
    NonContiguousLayer { expected: u32, got: u32 },
}
```

The reasoning sounds good: "future-proof the enum so adding a second
variant later won't break downstream `match` sites." Real Rust API
guidelines do recommend `#[non_exhaustive]` on enums that may grow.

## Why it's wrong here

1. **Workspace-novel.** No other error enum in this workspace
   carries `#[non_exhaustive]` — `MaskError`, `CavityError`, etc.
   are bare. Adding it here introduces a one-off attribute the rest
   of the codebase doesn't model.
2. **Zero immediate benefit.** The enum has one variant. There are
   zero downstream `match` sites that benefit from being forced to
   add a wildcard arm. The protection cost is paid up-front for a
   future variant that may never land.
3. **YAGNI.** When the second variant arrives, `#[non_exhaustive]`
   can be added then — it's a forward-compatible attribute change
   that doesn't break downstream code (downstream code already had
   to compile against the single-variant form).

## When to add `#[non_exhaustive]`

When BOTH:
- The enum has 2+ variants (so `match` sites already exist)
- The enum is part of a published external API (downstream code
  outside the workspace pattern-matches it)

For internal workspace error enums on single variants, match the
sibling enums' derive set byte-for-byte:

```rust
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AggregateError {
    #[error("layers must be sequential: expected {expected}, got {got}")]
    NonContiguousLayer { expected: u32, got: u32 },
}
```

## Symptom in code review

Plan reads "introduces `#[non_exhaustive]` for future-proofing" or
"forward-compatible by design" — and the reviewer finds neither a
second variant nor an external consumer. Drop the attribute, add it
back if/when the second variant materialises.

## See also

- `promoting-panic-to-typed-result-on-aggregate-mutator.md` — the
  parent pattern; Gate 2 ("match sibling error enums byte-for-byte")
  is what this anti-pattern reinforces.
