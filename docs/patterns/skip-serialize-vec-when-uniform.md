---
issue: ctb-layer-height-authority
date: 2026-05-19
---

# Pattern: skip-serialize Vec when uniform

## Context

Value objects that wrap a `Vec<f32>` (or any homogeneous collection)
often have a common "every entry equal" case and a rare "varying"
case. Serialising the full Vec for the common case is byte-wasteful
when the data is recoverable from a single scalar + a count.

In the `ctb-layer-height-authority` ticket, the
`LayerHeightProvenance` value object wraps a per-layer
`LayerHeightSeq` (a typed `Vec<f32>`). For Mag's lilith-torso.ctb at
4492 layers, embedding the full Vec on every uniform sim.json costs
~70 KB of duplicate noise — every entry is the same float.

## The pattern

Implement a custom `Serialize` that emits a flat scalar + count for
the uniform case and the full Vec for the variable case. Implement a
custom `Deserialize` (via a wire-shape struct with optional fields)
that accepts both. JSON consumers branch on which field is present.

```rust
impl Serialize for LayerHeightProvenance {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ctb_field_count = if self.uniform_height_um().is_some() { 2 } else { 1 };
        let mismatch_field_count = if self.mismatch.is_some() { 1 } else { 0 };
        let field_count = 1 + ctb_field_count + mismatch_field_count;
        let mut state = s.serialize_struct("LayerHeightProvenance", field_count)?;
        match self.ctb.uniform() {
            Some(u) => {
                state.serialize_field("ctb_um", &u)?;
                state.serialize_field("layer_count", &self.ctb.len())?;
            }
            None => {
                state.serialize_field("ctb_layer_heights_um", self.ctb.as_slice())?;
            }
        }
        state.serialize_field("recipe_um", &self.recipe_um)?;
        if let Some(m) = self.mismatch.as_ref() {
            state.serialize_field("mismatch", m)?;
        }
        state.end()
    }
}

#[derive(Deserialize)]
struct LayerHeightProvenanceWire {
    #[serde(default)] ctb_um: Option<f32>,
    #[serde(default)] ctb_layer_heights_um: Option<Vec<f32>>,
    #[serde(default)] layer_count: Option<u32>,
    recipe_um: f32,
    #[serde(default)] mismatch: Option<MismatchDetail>,
}

impl<'de> Deserialize<'de> for LayerHeightProvenance {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = LayerHeightProvenanceWire::deserialize(d)?;
        let ctb = match (w.ctb_layer_heights_um, w.ctb_um, w.layer_count) {
            (Some(v), _, _) => LayerHeightSeq::try_from_vec(v).map_err(D::Error::custom)?,
            (None, Some(u), Some(n)) if n > 0 => {
                LayerHeightSeq::try_from_vec(vec![u; n as usize]).map_err(D::Error::custom)?
            }
            (None, Some(u), None) => {
                LayerHeightSeq::try_from_vec(vec![u]).map_err(D::Error::custom)?
            }
            (None, None, _) => return Err(D::Error::custom("missing ctb_um or ctb_layer_heights_um")),
        };
        Ok(Self { ctb, recipe_um: w.recipe_um, mismatch: w.mismatch })
    }
}
```

## Trade-offs

- + ~70 KB saved per uniform sim.json on a 4500-layer print
  (Mars 5 Ultra typical part scale)
- + JSON consumers can branch on `ctb_um` vs `ctb_layer_heights_um`
  to identify uniform vs adaptive prints without parsing the Vec
- − Two valid JSON shapes for the same value object — Deserialise
  must be tolerant of both
- − Custom Serialize / Deserialize impls have to keep `field_count`
  in sync with the actual branches (clippy `if_same_then_else` caught
  a `1 else 1` bug during development of the resinsim impl)

## When NOT to use

- The Vec is meaningful to the consumer in BOTH cases (e.g. has
  identity beyond the values themselves — per-entry timestamps,
  ordering relevance with semantic meaning per position)
- The "uniform" case is rare (premature optimisation)
- The collection is small enough that the Vec cost is noise

## See also

- ADR-0015 — sim.json canonical interchange (the producer/consumer
  contract this fits into)
- `crates/resinsim-core/src/values/layer_height_provenance.rs` —
  reference implementation
