---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# Pattern: is_empty as const-false when len is statically non-zero

## Context

clippy's `len_without_is_empty` lint flags `pub fn len(&self) -> usize`
without a paired `pub fn is_empty(&self) -> bool`. For a value object
that rejects empty input on construction, the type invariant
guarantees `len() >= 1`, but the lint doesn't know that.

`ctb-layer-height-authority` ran into this with `LayerHeightSeq`: the
type rejects empty `Vec<f32>` in `try_from_vec` and
`from_layer_inputs`, so `len()` is statically `>= 1`. Clippy still
wanted the paired API.

## The pattern

```rust
pub struct NonEmptySeq(Vec<f32>);

impl NonEmptySeq {
    pub fn try_from_vec(v: Vec<f32>) -> Result<Self, String> {
        if v.is_empty() {
            return Err("must be non-empty".to_string());
        }
        Ok(Self(v))
    }

    /// Number of elements. Always >= 1 — type invariant.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false` — type invariant. Provided for clippy's
    /// `len_without_is_empty` lint and as a paired API; the return
    /// value is statically known.
    pub fn is_empty(&self) -> bool {
        false
    }
}
```

The docstring on `is_empty()` names the type invariant explicitly so
a reader doesn't wonder why it's a constant.

## Trade-offs

- + Satisfies clippy without `#[allow]`
- + Documents the invariant at the API surface
- − Adds one method that callers theoretically don't need

`#[allow(clippy::len_without_is_empty)]` is a valid alternative, but
the const-false fn is more self-documenting.

## When NOT to use

- The type does in fact have a meaningful empty state (then implement
  `is_empty` properly).
- The invariant might change later (then the const becomes a lie).
  Audit during refactors that touch the constructor's validation.

## See also

- `crates/resinsim-core/src/values/layer_height_seq.rs` — reference
  implementation
