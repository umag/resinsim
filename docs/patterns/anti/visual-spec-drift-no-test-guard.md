---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
kind: anti-pattern
---

# Anti-pattern: visual spec drifts past plan when no test asserts the visual property

## The anti-pattern

A plan specifies a visual property ("emissive translucent",
"high-contrast outline", "wireframe") that the implementation
silently drops or substitutes (plain colour, default opacity).
Reviewers (code + adversarial + UX) checking the diff against the
plan see the intended class of object (a Plane3d, a
StandardMaterial) and tick the box. The visual property has no
automated test — Bevy tests do not render a frame and inspect
pixels by default — so the drift only surfaces at human
verification.

## Symptom

Lifecycle exits with all matrix reviewers PASS, autonomous loop
clean, and the human reports "I cannot see X" on first interactive
run. The plan said X was specified; the diff shows X was added
structurally; the visual reality lacks X.

## The fix

For visual properties that are load-bearing for the user
(visibility, contrast, motion cue), assert them in tests at the
component level even if rendering is not exercised:

```rust
let mat = app.world().resource::<Assets<StandardMaterial>>().get(&handle).unwrap();
assert!(mat.emissive.red > 0.0 || mat.emissive.green > 0.0 || mat.emissive.blue > 0.0,
    "cursor material must be emissive for visibility against any layer colour");
assert!(mat.double_sided, "cursor must be visible from any orbit angle");
```

Component-level assertions catch spec drift even without a rendering
harness. They are not as good as a pixel-perfect snapshot test, but
they are cheap, deterministic, and CI-friendly.

## Lifecycle hygiene

When a human-verification round catches a defect that no reviewer
flagged, it is a signal that the **review matrix was missing a
dimension** for that class of property. Update planning conventions
to require a test assertion for any plan step that specifies a
visual / sensory / accessibility property.
