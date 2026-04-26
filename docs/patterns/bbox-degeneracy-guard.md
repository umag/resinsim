---
issue: 02-stl-mesh-rendering
date: 2026-04-26
---

# Pattern: defensive bbox-degeneracy guard with `is_finite`

## Context

`resinsim_core::io::stl::bounding_box` returns `min: [INFINITY; 3]`
and `max: [NEG_INFINITY; 3]` when the input triangle slice is empty
(the loop never runs, so the initial sentinel values stay). Any
helper that consumes a `BoundingBox` and computes a centre or
diagonal must defend against three degeneracy classes:

1. **Zero-volume**: `min == max` for every axis. Diagonal = 0.
   Common cause: a single-vertex "triangle list", a collapsed
   pancake mesh, a single-layer slice.
2. **Empty input**: `min = INF`, `max = NEG_INF`. Diagonal =
   `(NEG_INF - INF, ...).length() = INF`. Centre =
   `(INF + NEG_INF, ...) * 0.5 = NaN`.
3. **Partially-non-finite**: rare in practice, but a single NaN/INF
   vertex in the input can produce `min` or `max` with mixed
   finite/non-finite components. Same hazard as (2).

A single `if diagonal < 1e-6` check catches only (1).

## Don't do this

```rust
fn compute_centre_and_distance(bbox: &BoundingBox) -> (Vec3, f32) {
    let min = Vec3::from(bbox.min);
    let max = Vec3::from(bbox.max);
    let diagonal = (max - min).length();
    if diagonal < 1e-6 {                  // misses INF and NaN
        return (Vec3::ZERO, 10.0);
    }
    ((min + max) * 0.5, 1.5 * diagonal)   // NaN centre on empty bbox
}
```

## Do this

```rust
fn compute_centre_and_distance(bbox: &BoundingBox) -> (Vec3, f32) {
    let min = Vec3::from(bbox.min);
    let max = Vec3::from(bbox.max);
    let diagonal = (max - min).length();
    let centre = (min + max) * 0.5;

    let degenerate = !diagonal.is_finite()
        || diagonal < 1e-6
        || !centre.is_finite();

    if degenerate {
        return (Vec3::ZERO, 10.0);
    }
    (centre, 1.5 * diagonal)
}
```

`Vec3::is_finite()` returns `true` only when all three components are
finite. Computing `centre` outside the guard (rather than after) lets
the guard inspect it, at the cost of computing a NaN you'll discard —
acceptable.

## Lock it in with a regression test

```rust
#[test]
fn empty_bbox_uses_fallback() {
    let empty_bbox = resinsim_core::io::stl::bounding_box(&[]);
    let (centre, distance) = compute_centre_and_distance(&empty_bbox);
    assert_eq!(centre, Vec3::ZERO);
    assert_eq!(distance, 10.0);
}
```

The test constructs the bug-triggering input via the actual
production bbox builder, so a future change to `bounding_box`'s
sentinel values is caught here too.

## See also

- `crates/resinsim-viz/src/mesh.rs::fit_panorbit_to_bbox` — first
  instance of this pattern.
- `docs/patterns/stl-to-bevy-mesh-flat-shaded.md` — viz-specific
  context.
