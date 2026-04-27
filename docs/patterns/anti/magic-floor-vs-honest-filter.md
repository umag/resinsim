---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# Anti-pattern: magic floor for non-physical values

## What

When a math operation is undefined for some inputs (log10 of zero or
negative; division by zero; inverse of singular matrix), the
tempting "fix" is to clamp the input to a tiny positive number so
the operation succeeds:

```rust
fn log10_safe(x: f32) -> f32 {
    x.max(0.001).log10()  // floor at log10(0.001) = -3.0
}
```

The chart now plots a continuous line instead of a gap. Easy. Done.

## Why this is wrong

The clamp converts "no defined value here" into "value is roughly
0.001" — a lie. The reader of the chart has no way to distinguish
"this is a real near-zero measurement" from "this measurement is
undefined." Both render as the same point on the line.

In issue 05's context: `safety_factor = ∞` for zero-force layers
(documented in `spec/uat/safety-factor-zero-force.md`).
`log10(∞)` is not a number — the layer has no defined log-safety
value. Plotting it at y = -3 says "this layer is *very* unsafe"
when the truth is "this layer has no peel force, so safety is
undefined / irrelevant." The reader dispatches to the wrong mental
model.

Plan v1 of issue 05 included exactly this: `safety_factor_log10`
with a `-3.0` floor. Adversarial review caught it as a HIGH finding:
"-3.0 is a magic number with no domain justification — the smallest
safety_factor a printer would ever surface in practice is around
0.1 (log10 = -1) and zero / negative are physically meaningless.
Clamping non-physical values to a fake finite floor makes the chart
lie."

## What to do instead

Filter at projection time. Drop the undefined samples before they
hit the chart. The resulting series has gaps where the value is
undefined, which the line renderer naturally interpolates across or
breaks at — both are honest.

```rust
for layer in layers {
    let sf = layer.safety_factor;
    if sf.is_finite() && sf > 0.0 {
        out.push((layer_idx, sf.log10()));
    }
    // sf <= 0 or non-finite → not in `out` → no point on chart
}
```

If the upstream chart user *needs* to see "this layer is special
somehow", surface it via a different visual channel: a marker, a
shaded background band, an annotation. Don't conflate "special" with
"value = 0.001".

## Where this lives in code

- `crates/resinsim-viz/src/ui/plots.rs::build_layer_chart_data` —
  issue 05's projection.
- Test
  `build_layer_chart_data_log_safety_omits_non_positive_and_non_finite`
  pins the filter contract.

## See also

- Pattern `nan-two-layer-defence.md` — adjacent pattern about
  defending downstream code from upstream NaN/Inf escapes. This
  anti-pattern is about not LYING when you can't compute; that
  pattern is about not CRASHING when you receive a lie. Same family,
  different perspective.
- ADR-0014 — issue 05's log-via-transform decision; this anti-pattern
  is the rationale for the "filter, not clamp" half of that
  decision.
