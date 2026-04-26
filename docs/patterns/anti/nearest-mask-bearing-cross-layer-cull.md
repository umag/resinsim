---
issue: 09-ctb-slice-rendering
date: 2026-04-26
---

# Anti-pattern: "Nearest mask-bearing neighbour" cross-layer face culling

## Context

Slice-stack rendering (CTB sliced files in `resinsim-viz`) converts a
stack of `LayerInput` (each carrying `Option<LayerMask>`) into a
single Bevy `Mesh` via face-culling boundary-quad emission. Per-voxel,
the algorithm decides whether each of the six axis-aligned faces is
exposed (visible) or culled (interior to the solid). For ±Z faces
this depends on the neighbour layer's voxel content.

`LayerInput.mask: Option<LayerMask>` carries two distinct semantics
depending on producer (see
`crates/resinsim-core/src/io/sliced.rs:20-23` and
`docs/patterns/mask-synthesising-adapter.md`):

- **Mask-producing parsers** populate `Some(mask)` — actual binary
  occupancy.
- **Area-only parsers / test fixtures** leave `mask: None`. The
  simulation runner synthesises a solid mask in this case; **viz must
  not**.

A None layer means "the parser produced no occupancy data here" — for
viz, that's **void**, not "render solid".

## Don't do this

```rust
// Compute prev/next mask-bearing layer indices in O(N) total and
// look up the "nearest" mask for the ±Z face-culling check.
let prev_mask_idx: Vec<Option<usize>> = build_prev_mask_idx(layers);
let next_mask_idx: Vec<Option<usize>> = build_next_mask_idx(layers);

// In the emission loop:
let neg_z_exposed = prev_mask_idx[i]
    .map_or(true, |j| !layers[j].mask.expect("...").is_solid(cx, cy));
```

The reasoning that goes with it ("we precompute neighbour indices to
avoid an O(N²) linear scan") sounds like a perf optimisation but
silently changes the semantics: when layer i+1 is None, the rule
looks past it to the next mask-bearing layer (say i+5) and culls
layer i's +Z face if layer i+5's voxel at (cx, cy) is solid. The
None layers in between act as if filled — exactly the synth this
pattern was supposed to prevent.

## Symptom

For a stack like `[Some(2×2 solid), None, None, Some(2×2 solid)]`, the
renderer produces a single closed cuboid spanning all three layer
heights, not two separate shells with a visible air gap. The
None-layer voids are invisible; viz silently lies about voxel content.

## Why it happens

- The "nearest mask-bearing neighbour" framing is intuitive when you
  think of the algorithm as "skip None layers transparently", which
  is itself a benign-sounding phrase that hides what's actually being
  done with the voids.
- A perf-driven O(N) precompute reads as an obvious win over a
  perceived O(N²) scan, so the change passes review on perf grounds
  without anyone re-checking the semantics.
- In practice the precompute solves a non-problem: the
  immediate-neighbour rule (below) is O(1) per check anyway.

## Do this instead

```rust
// Immediate-neighbour rule. None layers expose surrounding ±Z faces
// — viz never synthesises a solid mask.
let neg_z_exposed = i == 0
    || layers[i - 1]
        .mask
        .as_ref()
        .is_none_or(|m| !m.is_solid(cx, cy));
```

`is_none_or(...)` returns `true` when the option is `None`, which
exposes the face — that's the load-bearing void semantic. The check
is O(1); no precompute needed. See
`crates/resinsim-viz/src/slice.rs::z_face_void` and
`docs/patterns/voxel-mask-stack-to-bevy-mesh.md` for the full
two-pass algorithm.

## Test that locks the contract in

```rust
#[test]
fn none_mask_layer_renders_as_void_between_shells() {
    let layers = vec![
        solid_mask_layer(100.0, 2, 2, 0.5),
        no_mask_layer(100.0),                   // void
        solid_mask_layer(100.0, 2, 2, 0.5),
    ];
    let mesh = slice_stack_to_bevy_mesh(&layers);
    // Two separate 2×2×1 shells: 16 voxel-faces × 6 verts × 2 layers.
    assert_eq!(positions_of(&mesh).len(), 192);
}
```

The "nearest" rule produces a single merged cuboid with a different
position count — the assertion fails loudly, catching any future
drift back to the broken algorithm.

## Related

- `docs/patterns/mask-synthesising-adapter.md` — the load-bearing
  contract on `Option<LayerMask>` semantics.
- `docs/patterns/voxel-mask-stack-to-bevy-mesh.md` — the correct
  two-pass algorithm and the cross-references that make this trap
  hard to fall back into.
- 09-ctb-slice-rendering review history — round-2 caught this in
  both review-code and review-adversarial; round-1 missed it because
  the issue was framed as a perf concern rather than a semantic one.
