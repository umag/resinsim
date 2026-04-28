---
issue: 15-extract-resinsim-run
date: 2026-04-28
---

# Anti-pattern: serde_json silently coerces `f32::INFINITY` (and NaN) to `null`, breaking round-trips

## Context

When a struct field is `f32` and its value can legitimately be non-finite
(e.g. `f32::INFINITY` from `SafetyFactor::compute` for zero-load layers per
`failure_predictor.rs:279` + UAT-1 of `safety-factor-zero-force.md`),
`serde_json::to_string`/`to_string_pretty` writes that field as JSON `null`
because JSON has no Infinity/NaN literal. **The default deserializer then
fails on `null → f32`** with `invalid type: null, expected f32`.

This is a silent producer/consumer asymmetry: the `Serialize` impl accepts
the value, the round-trip from disk fails. Tests using purely-finite
synthetic fixtures don't catch it.

## Why this is wrong

1. **Round-trip should be lossless** for any value the in-memory aggregate
   holds. INFINITY is intentional state (zero-load semantics), not corruption.
2. **The failure mode is far from the cause.** Producer writes happily, consumer
   crashes hours/runs later when the file is consumed.
3. **Tests miss it** unless they explicitly construct non-finite field values.
   Synthetic test fixtures naturally use plausible-finite numbers.

## What to do instead

Wrap `f32` fields that can be non-finite with a custom serde adapter:

```rust
#[derive(Serialize, Deserialize)]
pub struct LayerResult {
    /// `f32::INFINITY` for zero-load layers per failure_predictor + UAT-1.
    #[serde(with = "f32_with_infinity")]
    pub safety_factor: f32,
    // ...
}

mod f32_with_infinity {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            s.serialize_f32(*v)
        } else {
            s.serialize_none() // null in JSON
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        let opt: Option<f32> = Option::deserialize(d)?;
        Ok(opt.unwrap_or(f32::INFINITY))
    }
}
```

The wire shape becomes `number | null`; the JSON Schema must allow both:

```json
"safety_factor": { "type": ["number", "null"] }
```

zod equivalent: `z.number().nullable()`.

## How to grep for the smell

When reviewing a serde-derived struct with `f32`/`f64` fields, ask: **can
this field ever be non-finite at runtime?** Look at the producers — any
`map_or(f32::INFINITY, ...)`, any `0.0_f32 / 0.0`, any external API that
might return `NaN`. If yes, the round-trip is broken without an adapter.

## Test discipline

Round-trip tests must explicitly include a non-finite case if the field
can be non-finite. Example regression test:
`simulation_repo.rs::save_to_path_round_trips_infinity_safety_factor_via_null`.
The "synthetic finite-values everywhere" test pattern is the trap.

## How issue 15 surfaced this

The bug existed in the codebase before ADR-0015 but couldn't surface
because the producer (in-process simulation builder) and consumer
(`ReportGenerator`) ran in the same process — the JSON round-trip never
happened. ADR-0015's canonical-interchange split forces the round-trip
through disk and immediately exposes the asymmetry.

The bug evaded all three review passes (review-code, review-adversarial,
review-ux) because they reasoned about code structure, not runtime data.
The user uncovered it by running the new pipeline against a real CTB
(Lilith Torso, 4492 layers, Mars 5 Ultra) — the final layer at area = 0
triggered the zero-force `INFINITY` path.

## See also

- `docs/patterns/nan-two-layer-defence.md` — defending against NaN at the
  domain value-type construction boundary (different concern: in-memory
  validation vs serialisation round-trip).
- `docs/patterns/anti/rust-nan-positive-validation-gap.md` — NaN slipping
  through positive-value validators.
- `docs/patterns/null-as-sentinel-for-non-finite-float-serde.md` — the
  fix-template companion to this anti-pattern.
- ADR-0015 — sim.json canonical interchange contract that surfaced this bug.
