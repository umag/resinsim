---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: ThermalField Z dimension is spatial mm (NOT layer count)

## Context

ResinSim's Tier-2 voxel fields are a family:

- `CureField`, `PhotoinitiatorField` (t2f1) — store per-voxel cumulative
  cure dose / photoinitiator concentration.
- `StrainField`, `StressField` (t2f3) — store per-voxel shrinkage-strain
  / cumulative-stress tensors.
- `ThermalField` (t2f4, this work) — stores per-voxel temperature.

The first four use **Z = layer count, one voxel per layer slab** per the
`voxel-field-z-dimension-is-layer-count.md` pattern. ThermalField
intentionally breaks this. This document explains why.

## Decision

`ThermalField`'s Z axis is **spatial mm divided by `voxel_size_mm`**
(e.g. 165 mm vat at 0.5 mm voxel → 330 Z-voxels), NOT the print layer
count. Z extents are anchored to `PrinterProfile.build_envelope_mm`,
NOT to the printed-part bbox.

## Rationale

### CureField/StrainField/StressField are layer-stacked phenomena

For these fields, each Z-voxel represents one cured layer's accumulated
physics output:

- CureField at `iz` = cumulative dose received by layer `iz`'s resin.
- StrainField at `iz` = shrinkage strain locked when layer `iz` cured.
- StressField at `iz` = cumulative residual stress at layer `iz`'s
  cured-polymer position.

The Z axis literally is "layer index" — Z=N means "the polymer that was
created during the print's N-th layer exposure", which lives at world
height `bbox_min_z + Σᵢ layer_height_um(i)` (variable across layers per
adaptive slicing). Each Z-slab is written ONCE during its cure layer
and (for strain/stress) locked to prevent overwrite.

### ThermalField is a spatial-temporal diffusion state

Temperature obeys the heat equation. It's:

- **Continuous in space.** A temperature at vat Z = 100 mm exists whether
  or not a layer was cured there at any particular point. The resin in the
  middle of the vat has a temperature; so does the still-liquid resin
  above it; so does the printed-part stack below it.
- **Continuous in time.** Temperature at every voxel updates every CFL
  substep, not just on layer boundaries. There's no "write once on this
  layer's cure" event.
- **Defined over the full vat volume, not the part.** Heat enters the
  vat at the LCD/FEP interface (the bottom Dirichlet BC) and leaves
  through the vat walls and resin surface (convective BCs). Cropping the
  domain to the part bbox eliminates the BC zones, leaving the diffusion
  solve with synthetic ghost boundaries that mean nothing physical.

Forcing layer-count Z onto a diffusion field would mean either:

- (a) Storing per-layer-end snapshots in Z — turns the 3D problem into a
  4D one (3D × time), N× memory.
- (b) Z = layer-index with coordinate translation back to spatial-Z for
  the solver — does the part-bbox ↔ vat-envelope translation twice and
  obscures the natural domain.

### Vat envelope, not part bbox

The diffusion solve needs the wall + resin surface BCs. Those BCs live
at the vat envelope's outer faces. The printed part inside the vat is a
subdomain of the simulation domain. Anchoring ThermalField to
`PrinterProfile.build_envelope_mm` keeps the BCs at natural surfaces.

Consumers that read per-voxel temperature at part voxels (e.g.
`VoxelCureCalculator::ec_at_temp_field`) translate part-bbox indices into
vat-envelope world coordinates via
`ThermalField::temperature_at_world(x_mm, y_mm, z_mm)`. Trilinear
interpolation inside the field; `Err(OutOfEnvelope)` at the vat boundary
(no clamping — clamping silently produces wrong answers; per
`clamp-onto-boundary-convolution` anti-pattern from t2f2).

## Consequences

- **Aggregate invariant divergence.** `PrintSimulation` carries cure /
  photoinitiator / strain / stress fields anchored to the part bbox, AND
  `thermal_field` anchored to the vat envelope. The aggregate invariant
  is *"each field has its own self-consistent bbox + voxel_size; thermal
  matches the printer envelope; the rest match the part bbox"*, not the
  earlier "all fields share one bbox" invariant.
- **Sidecar format supports per-field bbox already.** Verified at
  `crates/resinsim-core/src/repositories/sidecar/encoder.rs` lines
  230-388: each `FieldDescriptor` stores its own `bbox_origin` +
  `voxel_size_mm`. No format change beyond the new `FieldKind::Thermal`
  variant required.
- **No `write_layer` accessor on ThermalField.** Diffusion is not
  written-once-per-layer. Accessors are `temperature_at(ix, iy, iz)`,
  `temperature_at_world(x, y, z)`, `as_array_view`, `as_array_mut` (for
  solver), reductions (`volume_mean_c`, `volume_max_c`).
- **CureField/etc.'s `voxel-field-z-dim-is-layer-count` pattern is
  unchanged.** It documents the t2f1 / t2f2 / t2f3 / t2f3.5 family;
  t2f4 deliberately departs. The original pattern's mention of
  "ThermalField — t2f4" in its family list is forward-looking
  vocabulary, not a claim about Z semantics.

## Memory footprint comparison

For Mars 5 Ultra envelope 153 × 78 × 165 mm at 0.5 mm voxel:

| Field | Z axis | Z extent | Total voxels | Memory (f32) |
|---|---|---|---|---|
| CureField (part bbox 50×50×100 mm typical) | layer count | ~200 layers | 100 × 100 × 200 = 2 M | 8 MB |
| ThermalField (full vat envelope) | spatial mm | 165/0.5 = 330 | 306 × 156 × 330 = 15.8 M | 63 MB |

Tier-2 ThermalField is ~8× the size of part-bbox CureField for a typical
print, but well within `DEFAULT_MAX_FIELD_ALLOCATION_BYTES = 4 GB`. The
trade-off — full-vat coverage vs minimal memory — is the correct call
because the diffusion solve needs the BC zones at the envelope edges.

## See also

- ADR-0020 §Decision ii (full-vat envelope anchor), §Decision iii (Z =
  spatial mm)
- `docs/patterns/voxel-field-z-dimension-is-layer-count.md` — the
  layer-count Z pattern this one contrasts against
- `docs/patterns/anti/voxel-z-step-from-lateral-voxel-size.md` —
  anti-pattern affecting layer-count Z fields; ThermalField is not
  affected because Z IS the lateral voxel pitch in spatial Z
- `docs/patterns/anti/clamp-onto-boundary-convolution.md` — why
  `temperature_at_world` returns `Err(OutOfEnvelope)` instead of
  clamping at the vat boundary
