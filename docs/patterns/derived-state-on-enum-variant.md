---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# Pattern: don't duplicate derivable state on discriminator enums

## Context

When a value object has a discriminator enum (often `Mismatch*` /
`Result*` / `Outcome*`) and an associated "always present" data
field, it's tempting to store summary fields on the enum variants
for caller convenience. If those summary values are derivable from
the always-present data, the duplication is dead weight.

The `ctb-layer-height-authority` round-2 code review caught this:
`MismatchKind::Variable { min_um, max_um, mean_um }` duplicated
values that `LayerHeightProvenance::ctb_layer_heights()` (a
`LayerHeightSeq` always present) already computes. The fix made
`Variable` a unit variant; callers read min/max/mean from the
LayerHeightSeq on demand.

## The pattern

```rust
// BAD — duplicates derivable state on the enum.
pub struct Provenance {
    pub data: AlwaysPresentSeq,
    pub recipe: f32,
    pub mismatch: Option<MismatchDetail>,
}
pub enum MismatchKind {
    Uniform { value: f32 },
    Variable { min: f32, max: f32, mean: f32 },  // derivable from `data`
}

// GOOD — Variable is a unit variant; min/max/mean read from `data`
// on demand.
pub enum MismatchKind {
    Uniform { value: f32 },
    Variable,
}

impl Provenance {
    fn render_summary(&self) -> String {
        match &self.mismatch.as_ref().map(|m| &m.kind) {
            Some(MismatchKind::Uniform { value }) => format!("uniform {value}"),
            Some(MismatchKind::Variable) => format!(
                "variable {}-{} µm (mean {})",
                self.data.min(), self.data.max(), self.data.mean()
            ),
            None => "agreed".to_string(),
        }
    }
}
```

## Trade-offs

- + Single source of truth
- + Smaller enum (smaller stack values, smaller JSON when serialised)
- + Future changes to the summary calc happen in one place
- − Slight runtime cost: every read computes min/max/mean from the
  Vec. Linear in the Vec length but cached at the call site if needed.
- − Slight ergonomic cost: callers must reach back to `provenance.data`
  to get the summary values (one extra dereference).

For µm-scale per-print values where N rarely exceeds 5000, the
runtime cost is noise; the ergonomic cost is a method call. Worth it.

## When NOT to use

- The summary value is expensive to compute (linear in a multi-GB
  collection, or requires async I/O). Caching as an enum field then
  makes sense.
- The summary value can change after construction (mutation —
  unusual for value objects but possible for entities). Then the
  cached enum field is the only consistent shape.

## Counterpart

The "duplicate state on enum" smell is often introduced when
serialising for an external consumer that wants the summary
pre-computed. If the consumer is a CLI / report, it can compute on
read just like internal callers. If the consumer is a network API
contract, document why the dup exists (see also:
`docs/patterns/skip-serialize-vec-when-uniform.md`).

## See also

- `docs/patterns/skip-serialize-vec-when-uniform.md`
- `crates/resinsim-core/src/values/layer_height_provenance.rs` —
  `MismatchKind::Variable` reference shape
