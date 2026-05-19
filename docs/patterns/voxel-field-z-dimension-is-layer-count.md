---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# Pattern: Voxel field Z dimension equals print layer count

## Context

resinsim's Tier-2 voxel fields (`CureField`, `PhotoinitiatorField` —
t2f1; planned `StrainField` — t2f3; `ThermalField` — t2f4) store
spatial state on a 3D grid spanning the printed part. Two natural
choices for the Z axis:

1. **Z = num_layers, one voxel per layer slab.** Each Z-voxel represents
   the cure / strain / temperature state of one layer at one (x, y)
   pixel.
2. **Z = physical mm at uniform voxel_size_mm.** The grid is cubic;
   layers map to multiple Z-voxels each.

## Decision

t2f1 picks **Z = num_layers**.

## Rationale

- **Aligns with the layer-by-layer process.** Each layer's exposure
  pass writes to one Z-slab; the slab's cure state is what the
  simulation reports per layer. A finer Z grid would have to be
  re-binned at every per-layer aggregation.
- **Memory.** For a 4492-layer Mars 5 Ultra print at 0.5 mm mask voxel
  and 50 µm layer height, option 1 stores 4492 Z-voxels; option 2 with
  cubic 0.5 mm voxels stores 4492 × (500/50) = 44 920 Z-voxels — 10×
  larger for no information gain (Beer-Lambert within a layer is
  exactly recovered by the analytical formula already implemented in
  `apply_column_exposure`).
- **Anisotropy is intrinsic.** The MSLA process is anisotropic
  (X-Y resolution = LCD pixel grid; Z resolution = print layer
  thickness). Pretending it's isotropic distorts the model.

## Consequences

- **Z-step is the print layer height, NOT `voxel_size_mm × 1000`.** See
  the related anti-pattern
  `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md`.
- **CureField.voxel_size_mm describes X-Y only.** Despite the name,
  this field is the LCD pixel pitch, not a cube edge length. Docstring
  on CureField calls this out.
- **world_at_voxel_center maps `(ix, iy, iz)` to world coordinates by
  treating Z as ALSO using voxel_size_mm**, which is intentionally
  WRONG for use as a physical-Z lookup. The method exists for the viz
  heatmap's per-pixel world position (X-Y only). Voxel-mode physics
  inside the simulator never calls it for Z. Future viz code that
  wants a physical-Z world position should compute it from
  `bbox_min_z + iz × layer_height_um` — t2f4 / viz overlay work.

## Source of layer_height_um

In t2f1 the runner reads `recipe.layer_height_um()`. A follow-on
ticket `ctb-layer-height-authority` realigns this with the CTB's own
layer_height (the slicer's authoritative value), warning the user
when the resin profile's recipe disagrees.

## Out of scope for t2f1

- Sub-layer Z resolution. Multi-Z-voxels-per-layer would refine
  Beer-Lambert depth within a single layer — already captured
  analytically inside `apply_column_exposure`, no extra storage needed.
- Cross-layer ray tracing. Modelling photons from layer N exposing
  layer N+1 (which is physical and real for shallow MSLA prints) is
  out of scope for t2f1; it's a t2f2 / t2f3 concern.

## See also

- ADR-0017 §3 "Dense Array3<f32> over part bbox" — dimension choice
- ADR-0017 §6 "Coordinates" — bbox-anchor convention
- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` —
  sibling anti-pattern for the Z-step source
- Ticket `ctb-layer-height-authority` — recipe-vs-CTB authority
  refinement
