---
issue: 15-extract-resinsim-run
date: 2026-04-28
---

# Pattern: `null` as the JSON sentinel for non-finite floats (serde adapter)

## Context

JSON has no Infinity/NaN literal. A struct field with `Serialize + Deserialize`
on `f32` (or `f64`) silently corrupts round-trips when the value is
non-finite — `serde_json` writes `null`, then fails to read it back.

Per the anti-pattern at
`docs/patterns/anti/serde-json-non-finite-f32-null-coercion.md`, the
solution is a serde adapter that maps the non-finite value to JSON `null`
on serialise and back on deserialise.

## Implementation

```rust
mod f32_with_infinity {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f32, s: S) -> Result<S::Ok, S::Error> {
        if value.is_finite() {
            s.serialize_f32(*value)
        } else {
            s.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        let opt: Option<f32> = Option::deserialize(d)?;
        Ok(opt.unwrap_or(f32::INFINITY))
    }
}
```

Apply via `#[serde(with = "f32_with_infinity")]` on the field:

```rust
#[serde(with = "f32_with_infinity")]
pub safety_factor: f32,
```

## When to use this vs `Option<f32>`

- **Use `Option<f32>` directly** when the in-memory caller would benefit
  from branching on `Some`/`None` (e.g. "no value computed" is meaningful
  to read-paths).
- **Use this adapter** when the in-memory representation is canonically
  `f32::INFINITY` (zero-load = infinite safety factor in the physics
  model) and refactoring every read site to handle Option would be
  invasive without semantic gain.

The adapter preserves the in-memory `f32` shape — every existing
`safety_factor.is_finite()` / `safety_factor < threshold` call continues
to work without changes.

## On-disk shape

The wire shape becomes `number | null`. Schemas must allow both:

- JSON Schema: `{"type": ["number", "null"]}`
- zod 4: `z.number().nullable()`

## Asymmetric NaN handling

The adapter promotes `null → f32::INFINITY` on read. If a NaN somehow leaks
into the field (it shouldn't — domain value-types catch NaN at construction
per `nan-two-layer-defence.md`), the round-trip lossily recovers it as
INFINITY. This is strictly safer than crashing on `null → f32` deserialise,
but a strict-mode adapter could refuse `null` from a non-INFINITY producer.
Default to lossy promotion until a regression demands strictness.

## Test discipline

Always add an explicit round-trip test for the non-finite case. Example:
`simulation_repo.rs::save_to_path_round_trips_infinity_safety_factor_via_null`
constructs a layer with `safety_factor: f32::INFINITY`, saves, asserts the
on-disk JSON has `null` for that field, reloads, and asserts the f32 is
INFINITY again.

## See also

- `docs/patterns/anti/serde-json-non-finite-f32-null-coercion.md` — the
  bug this pattern fixes.
- `docs/patterns/nan-two-layer-defence.md` — domain-value NaN guards.
- ADR-0015 — sim.json canonical interchange.
