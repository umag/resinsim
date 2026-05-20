---
issue: t2f3-shrinkage-strain-stress
date: 2026-05-20
---

# Pattern: honest-zero predictor with documented model-gap caveat

## When to apply

A failure-prediction module passes through three layers:
1. **Physics model** that computes some stress / strain / temperature
   field.
2. **Threshold criterion** that converts the field into a binary "is
   this a problem?" answer.

You have a credible, calibrated threshold (e.g. resin tensile strength
from a measured datasheet) but the underlying physics model is known
to UNDERESTIMATE the field magnitude. The result: on real workloads
the criterion returns zero (no failure detected) even where you know
from observation real failures occur.

You have three options:

1. **Tune the threshold downward** so the model fires on real-world
   workloads. Buys signal at the cost of calibration: the criterion
   is no longer the physical yield strength, it's an arbitrary
   "matches what we see" number. Real future physics improvements
   then need re-tuning, and the criterion drifts away from a
   defensible anchor.

2. **Don't ship the criterion** until the physics is improved. Buys
   correctness at the cost of forward compat: when Tier-3 lands, all
   the infrastructure (caches, accessors, sim.json fields, viz
   integration) has to be added then.

3. **Ship the criterion at the physically-correct threshold; document
   the model gap; return honest zeros.** Buys both forward compat and
   correctness. The honest-zero answer means "the model says no failure
   under its current limitations"; when the physics improves, the same
   criterion fires correctly without recalibration.

## When to pick option 3

- The threshold has an external physical meaning (tensile strength,
  yield criterion) that won't drift as the model improves.
- The model gap is well-understood and the path to closing it is
  documented (e.g. "Tier-3 cumulative residual stress").
- A downstream consumer (UI, dashboard, viz) wants the FIELD
  populated even if the criterion is currently silent.

## The discipline

- Document the gap in the predictor doc-comment AND the relevant KB
  entry. "Model produces values of order 5 MPa; threshold is order
  35 MPa; gap closes when X."
- Add a regression-guard integration test asserting the criterion
  reads zero on a baseline workload. Future contributors who close
  the gap have to explicitly update this test, which forces them to
  re-evaluate whether the threshold is still appropriate.
- Make the gap visible in user-facing output. The
  `voxel_yield_fraction` cache populates as Option<f32> Some(0.0)
  rather than None — distinguishing "criterion ran and read zero"
  from "criterion didn't run". Consumers see the difference.

## Example: t2f3 voxel_yield_fraction

- Criterion: `count(σ_vm > tensile_strength_mpa per voxel) / cured_count`.
- Both σ_vm and tensile_strength_mpa are physically anchored (von
  Mises and KB-140 datasheet values). No safety factor multiplier.
- Current model: per-voxel free-shrinkage stress only. σ_vm peaks
  at ~5 MPa on typical photopolymer prints.
- Gap: real warpage is driven by CUMULATIVE residual stress across
  the multi-layer cure, not per-voxel snapshot. Tier-3 work.
- Honest-zero behaviour: yield_fraction = 0.0 on every layer of every
  current print. Cache field populated. WarpingRisk fires 0.
- Forward compat: when Tier-3 closes the gap, the same threshold
  fires; no rewiring needed.

## See also

- KB-162 (yield criterion derivation + model-gap caveat in the doc)
- ADR-0018 §9 (model-gap documentation)
- `crates/resinsim-core/src/services/failure_predictor.rs`
  predict_strain_failures doc-comment
- `docs/patterns/anti/hydrostatic-strain-dead-warpage-detector.md` —
  the upstream-physics fix that took σ_vm from 0 to 5.71 MPa; the
  honest-zero pattern is the response to "the criterion is still
  silent even after that fix because real warpage is cumulative"
