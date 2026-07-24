---
issue: peel-corrections-s3-perimeter-shape
date: 2026-07-24
---

# Pattern: Discriminate synthetic placeholder masks before deriving a per-layer geometry factor

## Context

`SimulationRunner` always hands `predict_layer` a `LayerMask` per layer, but not
every mask describes real geometry. Two paths synthesise **placeholder** masks:

- `run_from_areas` emits a **1×1 all-solid** mask per layer — it carries a scalar
  area only, no shape.
- `run_from_layer_inputs` fills any layer missing a mask with an all-solid
  **W×H** mask sized to the build grid (the fallback at
  `simulation_runner.rs`), so cavity/suction topology stays consistent.

ADR-0022 Stage 3 derives an area/perimeter (A/L) peel shape factor from the
mask. Naively applying it to a placeholder is a **silent correctness bug**: a
non-square all-solid W×H fallback has A/L ≈ 0.94 of a square, so an active shape
factor would quietly reduce peel force on layers whose real geometry is unknown.
This was caught as a HIGH in plan review, before any code was written.

## Pattern

Before computing a geometry-derived per-layer factor, **discriminate real
geometry from a filled bounding box** and treat the placeholder as neutral:

1. Add a cheap predicate on the value object:
   `LayerMask::is_fully_solid() == (solid_cell_count == width*height)`.
2. In the runner's per-layer factor builder, branch on it:
   `if mask.is_fully_solid() { NEUTRAL } else { factor_from(mask) }`, where
   `NEUTRAL` is the identity for the factor (here `1.0`).
3. Gate the whole pass on the opt-in being active (empty map ⇒ `None` ⇒ neutral)
   so the default path allocates nothing and stays byte-identical.

A fully-solid mask is *exactly* the set of "no interior shape signal" cases: the
1×1 placeholder, the W×H fallback, AND any genuine grid-filling solid part
(which is compact anyway, so neutral is the right answer). Real parts leave void
margins around themselves, so they are never fully-solid and always get a real
factor. The predicate is the crisp, testable boundary between "measured
geometry" and "synthesised placeholder."

## Why

- **The default path is provably neutral.** No placeholder can perturb the
  physics, so behaviour-preservation is structural, not incidental.
- **The bug is impossible by construction**, not merely untested — the reviewer's
  "does this apply real physics to fake geometry?" question is answered in the
  branch.
- **The predicate is reusable.** Any future per-layer geometry analysis
  (perimeter, convexity, hole count) faces the same placeholder hazard and can
  reuse `is_fully_solid()` as the guard.

## When NOT to

If a future consumer legitimately wants to treat a grid-filling solid rectangle
as a *real* elongated part (rather than flooring it to neutral), it must first
distinguish "genuine full-bbox part" from "synthesised placeholder" by a
stronger signal than fully-solid — e.g. threading a `mask_is_real: bool` down
from the layer source. Until such a case exists, flooring full-bbox masks to
neutral is the correct, conservative default (documented trade-off).

## See also

- ADR-0022 Stage 3; `crates/resinsim-core/src/app/simulation_runner.rs`
  (`build_shape_factor_map`), `values/layer_mask.rs` (`is_fully_solid`).
- KB-185 — the A/L peel shape factor this guard protects.
- The optional-field + empty-map default template:
  `docs/patterns/parametrize-const-through-canonical-method.md` (S2).
