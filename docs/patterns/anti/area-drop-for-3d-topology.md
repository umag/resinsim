---
issue: suction-detector-raft-false-positive
date: 2026-04-21
---

# Anti-pattern: 2D area-drop as a proxy for 3D topology

## Symptom

A detector uses per-layer cross-sectional area (`f64` scalar) to infer 3D
properties of the print — presence of sealed cavities, suction cups,
trapped fluid pockets. Ratio-based heuristics (e.g. "area drop > 50%
between consecutive layers → flag as suction cup") look defensible on
paper but collapse on common geometries.

## Why it fails

Two topologically distinct geometries produce identical area signatures:

- **Sealed cavity** (ring walls around trapped fluid, lateral exterior
  blocked): true suction cup.
- **Fluid-permeable supports** (discrete columns with inter-column gaps
  reaching the lateral bbox): no suction, fluid drains to the vat.

Both have the same "area drops sharply from the previous layer" signal.
Only 3D topology — in particular, the lateral-boundary reachability of
the void region — distinguishes them. A 2D scalar can't encode this.

## Symptoms this anti-pattern produces

- False positives on every rafted print (raft → supports is the most
  common non-trivial MSLA topology).
- Fabricated downstream failures when the false positive is fed into
  safety-factor computations (force exceeds support capacity, deflection
  overshoots layer height, etc.).
- Users distrust the tool's critical failure reports.

## Right approach

Emit per-layer **occupancy masks** (binary grid at a physical voxel size)
from the slicer, then run a topology analysis over the 3D void volume:
connected-components across layers with an explicit ambient-boundary
policy (which bbox faces count as "exterior"). Events emerge only for
topologically-sealed pockets — the signal the caller actually wants.

## In resinsim

Replaced in 2026-04-21 Phase B of the suction-detector-raft-false-positive
lifecycle:

- `SuctionDetector::detect_from_areas` (area-only) — removed.
- `LayerMask` value object + `CavityDetector` service — added.
- `SimulationRunner` entry points now always feed `LayerMask` stacks
  through `CavityDetector`.

## See also

- `resinsim-core/src/services/cavity_detector.rs` — the 3D replacement.
- `docs/adr/0006-ambient-boundary-policy-for-cavity-detection.md` —
  formalises the lateral-vs-z boundary decision.
