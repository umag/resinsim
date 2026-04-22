---
issue: suction-detector-raft-false-positive
date: 2026-04-21
---

# ADR-0006: Ambient boundary policy for 3D cavity detection

## Status

Accepted.

## Context

`CavityDetector` walks a stack of per-layer `LayerMask`s and identifies
topologically-sealed void pockets. Each void voxel connects to its
4-neighbours within a layer and to the same (x, y) voxel in adjacent
layers. A void pocket is "exterior" (drains freely to the vat) if it has
a path to the ambient.

But what counts as "ambient"? The bounding box has 6 faces. The detector
must classify each as `exterior` (void at face = drain to vat) or
`barrier` (void at face treated as sealed).

## Decision

- **Lateral bbox faces** (x=0, x=width-1, y=0, y=height-1 at any z)
  = **exterior**. Vat fluid surrounds the print laterally and can freely
  enter/leave any void voxel touching these faces during peel.
- **z=0 face** (layer 0, physically on the build-plate side during peel)
  = **barrier**. The build plate is a rigid solid that caps the print
  from above during peel; any void sealed on top by layer-0 solid is
  genuinely sealed.
- **z=N-1 face** (last layer, physically on the FEP side during peel)
  = **barrier**. Pockets extending to the last layer without closure are
  "open at FEP" — each peel of a wall-layer releases cleanly without
  concentrated vacuum. These produce no events.

## Consequences

- A raft with fluid-permeable supports generates no false positive
  because the inter-column void touches the lateral face and is
  classified exterior.
- A hollow cup with a solid cap at its last wall-layer generates exactly
  one event at the cap layer (when the cavity transitions from "open at
  FEP" to "fully sealed").
- An open-topped cup (walls never close) generates no event — correct
  per physics (ring-layer peel doesn't concentrate vacuum).

## Alternate policies considered

- **z=0 as exterior (via through-hole escape).** Could model prints
  where the build plate has a relief hole. Rejected — no commodity MSLA
  printer has this; the build plate is a flat rigid surface.
- **z=N-1 as exterior (FEP-side drains at peel).** Physically, the FEP
  peels away and fluid CAN flow in through the peeling edge. But the
  concentrated vacuum that drives suction-cup failure occurs in the
  instant before the FEP releases; at that moment, the void is still
  sealed on all sides. Treating z=N-1 as exterior would miss real
  suction events.

## When to revisit

- If a new failure mode surfaces that needs a different topology
  classification.
- If a printer with a build-plate relief hole ships.
- If Athena II E4 calibration reveals suction forces outside the range
  predicted by the current policy.

## See also

- `resinsim-core/src/services/cavity_detector.rs` — implementation.
- `docs/patterns/anti/area-drop-for-3d-topology.md` — the previous
  approach.
- `spec/EXPERIMENT-PLAN-v1.1.md §E4` — future suction calibration.
