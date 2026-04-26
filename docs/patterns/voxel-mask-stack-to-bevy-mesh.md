---
issue: 09-ctb-slice-rendering
date: 2026-04-26
---

# Pattern: voxel mask stack → Bevy mesh (CTB sliced-file rendering)

## Context

`resinsim-core::io::ctb::parse_ctb` returns
`(SlicedFileInfo, Vec<LayerInput>)` where each `LayerInput` carries an
`Option<LayerMask>` — a bit-packed binary occupancy grid at a fixed
physical resolution (`voxel_size_mm`, default 0.5 mm). To render that
sliced volume in `resinsim-viz` (Bevy 0.18) we need a single
`bevy::mesh::Mesh` carrying positions, normals, and indices in the
formats the GPU expects — the same shape that
`mesh::triangles_to_bevy_mesh` produces for STL geometry, so the
existing `LoadedStlMesh` rendering pipeline can host the new content
without per-format render plumbing.

ADR-0010 forbids any `bevy::*` dependency in `resinsim-core`, so the
conversion has to live in viz. The bbox helper reuses
`resinsim_core::io::stl::BoundingBox` rather than introducing a viz-
local equivalent — this keeps `mesh::fit_panorbit_to_bbox` reusable
unchanged for both geometry sources.

## The load-bearing semantic — None = void

`LayerInput.mask: Option<LayerMask>` is documented to mean two distinct
things depending on producer:

- **Mask-producing parsers** (extended CTB) populate `Some(mask)`.
- **Area-only parsers / test fixtures** leave `mask: None`.
  Downstream simulation consumers
  (`SimulationRunner::run_from_areas`) synthesise a fully-solid mask
  in this case, per the `mask-synthesising-adapter.md` pattern.

The pattern is explicit that the simulation runner does this synth;
**viz must not**. A None mask is not "we don't know, render it
solid" — it's "no mask was produced; the layer's voxel content is
unobservable here". Synthesising solid would lie about voxel content.

For the viz cross-layer face-culling rule this means an
**immediate-neighbour** check, not a "nearest mask-bearing neighbour"
walk:

```rust
// -Z face exposed iff i == 0 OR layers[i-1].mask is None
//                    OR layers[i-1] voxel at (cx, cy) is empty.
let neg_z_exposed = i == 0
    || layers[i - 1]
        .mask
        .as_ref()
        .is_none_or(|m| !m.is_solid(cx, cy));
```

The earlier reviews of this issue first proposed a "nearest mask-
bearing neighbour" precompute as an O(N) optimisation over a perceived
O(N²) scan. That framing was wrong twice over: the immediate-neighbour
check is O(1) anyway, and the "skip None layers transparently" reading
effectively synthesised solid for None — exactly the prohibition this
pattern names.

## The two-pass algorithm

```
Pass 1 — Validate dims:
  - first_mask_dims(layers) → (w, h, voxel_size_mm) or None.
  - None → return empty Mesh (covers all-None and empty-input).
  - Mismatch on any Some(mask) → warn! and return empty Mesh.
  - No vertex emission can occur after this point on the failure path.

Pass 2 — Z prefix sum (f64) + emission with immediate-neighbour culling:
  - z_prefix: Vec<f64> of length n+1, accumulating layer_height_um/1000
    in f64. Cast to f32 only at vertex emission.
  - For each layer i with Some(mask):
      For each (cx, cy) where mask.is_solid(cx, cy):
        Emit each of the six axis-aligned faces only where the
        immediate neighbour is empty:
          -X: cx == 0 || !mask.is_solid(cx-1, cy)
          +X: cx+1 == w || !mask.is_solid(cx+1, cy)
          -Y: cy == 0 || !mask.is_solid(cx, cy-1)
          +Y: cy+1 == h || !mask.is_solid(cx, cy+1)
          -Z: i == 0     || layers[i-1].mask.as_ref().is_none_or(...)
          +Z: i+1 == n   || layers[i+1].mask.as_ref().is_none_or(...)
        Each exposed face emits 2 triangles = 6 unique vertices with
        the face's outward normal (axis-aligned ±X/±Y/±Z), indices
        sequential [0..6n]. Same vertex-tripled flat-shaded shape as
        `mesh::triangles_to_bevy_mesh`.
```

## Why f64 for the Z prefix sum

f32 has 23 mantissa bits. At magnitude 90 mm cumulative-sum scale (a
4500-layer × 20 µm print) the step size is ~5 µm — about a quarter of
a layer height — and naive cumulative summation amplifies the error
to tens of micrometres at the top. Voxels would visibly mis-stack at
the top of large prints.

f64 has 52 mantissa bits and pre-empts the drift entirely; the cost is
one f64 per layer (≤36 KB at 4500 layers). Cast to f32 only when
populating vertex positions — the GPU's vertex format is f32 so
keeping f64 inside the algorithm is purely about the prefix-sum
accuracy.

The unit test `f64_z_prefix_sum_resists_drift_over_5000_layers` runs
5000 × 20 µm and asserts `bbox.max[2] == 100.0 mm ± 1e-3`.

## Mutual exclusion: one geometry source visible at a time

v1 enforces a single visible geometry: loading either STL or CTB
despawns priors of *both* kinds, and clap's `conflicts_with` rejects
`--load-stl` + `--load-ctb` at parse time. The drag-drop dispatch
respects the same invariant — a `.ctb` drop despawns any
`LoadedStlMesh` and vice versa.

Per-vertex `Mesh::ATTRIBUTE_COLOR` was the chosen carrier for per-layer
scalar overlays (issue 03's cure-depth heatmap) because it does not
introduce a new entity marker, preserving the single-source
mutual-exclusion rule. The heatmap rides on the same `LoadedSliceStack`
mesh — a sibling cursor entity (`LayerCursor`) handles the active-layer
indicator with its own thin Plane3d mesh + transparent material.

This is a v1 scope cut, not a permanent shape. The earlier "deferred-
capability trigger" anticipated issue 03 introducing a separate
coloured-mesh marker; in practice issue 03 baked the colours into the
existing `LoadedSliceStack` mesh, leaving the single-source rule
untouched. The trigger now lands when STL + CTB co-display becomes a
real requirement (e.g. comparing a re-sliced model against its source
STL) — at that point a `GeometryLayer` enum lets multiple markers
coexist and the single-source rule generalises to "one of each kind".

## Cross-format BoundingBox reuse

`slice_stack_bounding_box` returns `resinsim_core::io::stl::BoundingBox`
rather than introducing a viz-local equivalent. That couples the slice
helpers to the STL module's type, but the alternative — a viz-local
struct — would either duplicate the field shape or force a third type
on `fit_panorbit_to_bbox`'s signature.

When a third format adds a bbox path (SL1, GOO, 3MF — already
enumerated in `sliced::detect_format`), promote `BoundingBox` to
`core::values::geometry` (or rename the `stl` module to something
format-neutral). Until the third caller exists, the reuse is the
right call.

## route_drop divergence from detect_format

`sliced::detect_format` is the canonical multi-format detector for the
inspect/CLI side and matches lowercase extensions verbatim. The viz
drag-drop path needs case-insensitive matching (macOS often emits
mixed-case extensions) and only renders two formats.

Rather than mutate core, the viz crate has a thin local helper:

```rust
pub fn route_drop(path: &Path) -> DropAction {
    let lower = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    match lower.as_deref() {
        Some("ctb") => DropAction::Ctb,
        Some("stl") => DropAction::Stl,
        _ => DropAction::Skip,
    }
}
```

When a third format renderer lands, `route_drop`'s match grows by one
arm and the dispatch table stays unit-testable on a pure function
(per the `bevy-app-test-seam.md` pattern).

## load_geometry_into_world refactor trigger

`load_stl_into_world` and `load_ctb_into_world` share the despawn-
priors → parse-or-error → build-mesh → spawn → fit-camera scaffold.
With two formats this is fine; with a third the duplication grows.
The natural refactor — a `load_geometry_into_world` taking a closure
`(&Path) -> Result<(Mesh, BoundingBox), String>` — should land when
the third format renderer is added (see also the BoundingBox
promotion trigger above; both refactors fall out of the same change).

## Performance budget

| Print scale                                | Boundary voxels  | Vertices    | Mesh memory |
|---|---|---|---|
| 50 mm cube @ 0.5 mm voxel (test fixture)   | ~60 K            | ~360 K      | ~10 MB      |
| 153 × 78 mm × 4500 layers (real print)     | ~6 M             | ~36 M       | ~1 GB       |

The test fixture is tractable as raw boundary triangles. Real-print
scale is the trigger for a greedy-meshing follow-up: once a print
exceeds **>10 M vertices**, switch to `binary-greedy-meshing` 0.5.2
(MIT, no bevy runtime dep) with a 62³-chunk adapter. The vertex-count
threshold is a concrete actionable signal — replace the v1 face-
emission with greedy meshing only when prints actually hit it.

## Testing notes

ADR-0003 unwrap-policy applies in tests too. All test-side `.expect()`
references the in-test mask construction (we control all inputs); no
fixture file is required for the unit tests because mask construction
via `LayerMask::new(w, h, voxel_size_mm)` + `LayerMask::new_all_solid`
is hermetic.

The CTB smoke test `smoke_exit_with_load_ctb_flag_runs_setup_without_panic`
gates on `RESINSIM_SLICED_FIXTURE` — same convention as
`data/test_cube_10mm.ctb.README.md`. CI without the env var no-ops the
test.

## See also

- `docs/patterns/stl-to-bevy-mesh-flat-shaded.md` — the analogous
  pattern for STL geometry; same vertex-tripled flat-shaded shape.
- `docs/patterns/mask-synthesising-adapter.md` — the load-bearing
  None-vs-solid contract.
- `docs/patterns/bbox-degeneracy-guard.md` — empty-bbox INF-sentinel
  handling, delegated to `fit_panorbit_to_bbox`.
- `docs/patterns/bevy-app-test-seam.md` — pure-function helpers
  (`route_drop`, `slice_stack_to_bevy_mesh`) extracted from systems
  for testability.
- `docs/adr/0010-resinsim-viz-presentation-layer.md` — viz layering
  rule (one-way dep on core).
