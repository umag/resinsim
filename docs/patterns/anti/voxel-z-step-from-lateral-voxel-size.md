---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# Anti-pattern: Voxel Z-step inferred from lateral voxel size

## Context

resinsim's Tier-2 voxel cure path (ADR-0017) stores per-pixel-per-layer
dose in a dense `CureField` with dimensions `(nx, ny, nz)` where:

- `nx` × `ny` is the LCD pixel grid (X-Y resolution) at the slicer mask's
  `voxel_size_mm` (typically 0.5 mm for cavity-detection-class masks).
- `nz` is the layer count of the print.

Each Z-voxel represents ONE LAYER of the print, not a cube of side
`voxel_size_mm`. The physical depth between adjacent Z-voxels is the
**per-layer** CTB-authoritative layer height (typically 30–50 µm, but
adaptive slicers vary it per layer), which is INDEPENDENT of the
lateral X-Y voxel pitch. See sibling pattern note
`docs/patterns/voxel-field-z-dimension-is-layer-count.md` and the
follow-on ticket `ctb-layer-height-authority` (resolved 2026-05-19 —
the runtime now sources the Z-step **per layer** from
`LayerInput.layer_height_um`, supporting adaptive-slicing CTBs as
first-class, with a stderr warning when `recipe.layer_height_um`
disagrees) for the recipe-vs-CTB authority concern.

## The mistake

A naive implementation of Beer-Lambert depth attenuation reaches for
`voxel_size_mm × 1000` as the depth-per-step:

```rust
// WRONG — assumes cubic voxels, treats the field as a 3D grid with
// uniform side `voxel_size_mm`.
for iz in iz_top..nz {
    let depth_um = (iz - iz_top) as f32 * voxel_size_um
                 + (voxel_size_um * 0.5);
    let attenuation = (-depth_um / dp_local).exp();
    // ...
}
```

For Mars-class slicer outputs (50 µm layer height, 0.5 mm mask voxel),
this gives `voxel_size_um = 500` so the first voxel-centre depth is
250 µm. With ceramic-grey Dp = 145 µm, attenuation = exp(-250/145) ≈
0.18, i.e. ~82 % of the surface dose is lost at the very first voxel
under the LCD/FEP interface. The Tier-1 scalar primitive
`CureCalculator::cure_depth` doesn't see this because it uses a single
formula on the full layer-height; only the voxel-resolved path goes
wrong.

The bug is silent — `cargo nextest run --workspace --features field-sim`
passes (the tests use exposure values high enough to overcure even at
the buggy depth, masking the issue), and the Tier-1 path is untouched.
You only see the discrepancy when you compare a Tier-1 scalar against
the voxel field's `LayerSummary.mean` on a real CTB.

## The fix

`apply_column_exposure` takes `layer_height_um: f32` as an explicit
parameter and uses it for Z-stepping:

```rust
for iz in iz_top..nz {
    let depth_um = (iz - iz_top) as f32 * layer_height_um
                 + (layer_height_um * 0.5);
    let attenuation = (-depth_um / dp_local).exp();
    // ...
}
```

`voxel_size_mm` is now used only for X-Y world-position lookups (per-
pixel uniformity factor, viz coordinate mapping). The Z-step is a
separate physical input from the recipe.

## Lesson

Voxel fields representing "one slab per layer" of a layer-by-layer
fabrication process have **anisotropic physical voxels** — the X-Y
pitch is the LCD pixel grid and the Z pitch is the recipe layer
height. A single `voxel_size_mm` field on the field VO is convenient
but tempting-misleading: it tracks the X-Y dimension, NOT a cubic cell
side. The Z-step must be threaded as a separate input.

For future Tier-2 work (t2f3 strain, t2f4 thermal diffusion), apply
the same convention: physical Z-step = layer height for the print
(see `ctb-layer-height-authority` ticket for the CTB-vs-recipe
authority refinement).

## Regression guard

`crates/resinsim-core/src/services/voxel_cure_calculator.rs`'s
`z_step_uses_layer_height_not_lateral_voxel` test fires a single
exposure into a 1×1×2 column with mask voxel 0.5 mm and layer height
50 µm, then asserts the surface voxel dose exceeds 50 mJ/cm² (correct
formula: ~77; buggy formula: ~8). The threshold cleanly distinguishes.

## See also

- ADR-0017 §6 "Coordinates" — voxel field Z dimension convention
- KB-160 — photoinitiator depletion uses the corrected Z-step
- `voxel_cure_calculator.rs` — `apply_column_exposure` docstring
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — sibling
  decision rationale
- Ticket `ctb-layer-height-authority` — recipe-vs-CTB authority gap
  surfaced during this lifecycle's harvest (resolved 2026-05-19)

## Resolution

`ctb-layer-height-authority` shipped on 2026-05-19. The runtime
authority for layer-height is now the CTB's per-layer
`LayerInput.layer_height_um` Vec (extracted via
`LayerInput::extract_layer_heights_um`; non-finite/non-positive
values rejected, variability preserved). Each layer's physics —
`predict_layer` and `apply_voxel_cure_for_layer` — receives its own
slab thickness, so adaptive / variable-layer-height CTBs (Chitubox v2,
Lychee Pro, PrusaSlicer MSLA) run end-to-end.

On disagreement with `recipe.layer_height_um`, the runner emits a
loud user-facing warning to stderr with two branches:

- **Uniform CTB disagrees with recipe** — quotes both values, names
  the recipe's value as a GUESS, names the recipe-implied WRONG LAYER
  COUNT for the same Z-extent.
- **Variable / adaptive CTB** — quotes the min/max/mean of the per-
  layer heights, names the recipe value as a GUESS that necessarily
  produces the WRONG LAYER COUNT (because no single value can
  describe a varying stack).

Both surfaces in `sim.json` via
`PrintSimulation.layer_height_provenance: Option<LayerHeightProvenance>`
(additive, schema-stable). The
`provenance.mismatch.kind` enum (`"uniform"` / `"variable"`)
discriminates the two failure modes so downstream consumers can
render appropriate badges. See ADR-0005 Consequences "Policy: CTB as
file-axis authority" and ADR-0017 §2 "Variable voxel resolution"
update for the canonical decision record.

Follow-up reminder queued for Phase 6 harvest:
- `LayerInput::new` validation tightening — mirror the helper's
  `is_finite() && > 0.0` guard at construction time so unsafe
  `LayerInput`s can't reach the reconciler.

(The `--accept-layer-height-mismatch` suppression flag idea was
mooted but is moot now that adaptive-slicing CTBs are first-class:
the warning surfaces in two distinct flavours, and a batch user
running adaptive CTBs would want the variable-Z badge as confirmation
rather than suppression.)
