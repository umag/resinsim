---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: Rayon for per-voxel-independent stencils preserves bit-identity

## Context

Stencil-based simulation kernels (heat diffusion, light convolution,
stress propagation) iterate over a 3D voxel grid and update each
voxel from its neighbours. The naive concern with parallelising via
Rayon is the f32 non-associativity hazard: parallel reductions
(sum, mean, max) can produce different bit patterns run-to-run
depending on thread schedule.

ResinSim's sidecar sha256 invariant (ADR-0019, ADR-0020 §Decision v)
requires bit-identical bytes across runs of the same input — so a
naive `par_iter_mut` over a reduction would break it.

## Pattern

**Per-voxel writes from an immutable snapshot are deterministic under
Rayon.** When the inner loop:

1. Reads only from an immutable snapshot of the prior state (e.g. a
   pre-cloned `Array3<f32>` "scratch" buffer).
2. Writes only to its own `(ix, iy, iz)` cell in the output grid
   (no cross-thread writes).
3. Does NOT participate in any parallel reduction.

...the final field bytes are a deterministic function of the input
bytes, regardless of thread schedule. The per-voxel computation is
sequential (each voxel's update reads a fixed set of neighbours from
the snapshot and applies the same FLOPS in the same order),
producing the same f32 bit pattern.

Example from `ThermalDiffusionSolver::step` (ADR-0020):

```rust
use ndarray::parallel::prelude::*;
let data = field.as_array_mut();
ndarray::Zip::indexed(data).par_for_each(|(ix, iy, iz), out| {
    *out = update_voxel(ix, iy, iz, &t_old_snapshot, &bcs);
});
```

`t_old_snapshot` is an immutable Array3 borrowed across all threads;
`out` is a per-voxel mutable reference into the field; `update_voxel`
is a pure function. No reductions cross thread boundaries.

## What stays serial

- **Volume reductions** like `volume_mean_c` and `volume_max_c` —
  these use `f32::max` / sum-of-f64-accumulator and are NOT
  parallelised. They're read-only reports (`tier-2 thermal complete:`
  log line, postcondition checks); they do NOT enter the field state.
- **Per-step cross-voxel dependencies** — if a voxel update needed
  to read a *neighbour's NEW value* (e.g. Gauss-Seidel iteration),
  Rayon would create a read-after-write race and non-determinism.
  FTCS (forward-time centred-space) is explicitly forward-only,
  reading the prior step's state via the immutable snapshot.

## Validation

The determinism invariant is enforced by
`thermal_diffusion_solver::tests::two_runs_with_same_input_produce_byte_identical_field`
— constructs two identical fields, runs N steps on each, and asserts
`a[i].to_bits() == b[i].to_bits()` across the entire grid. Add an
equivalent test to any Rayon-ified stencil.

## When this pattern does NOT apply

- Gauss-Seidel / SOR iteration schemes (read-after-write).
- Multigrid restriction / prolongation operators (cross-cell averaging).
- Any kernel that calls a parallel reduction WITHIN the per-voxel work.
- Any kernel that uses `par_iter_mut().for_each(|cell| ...)` without
  the explicit indexed-write pattern — `Zip::par_for_each` is the
  safe API; raw `par_iter_mut` can hide non-determinism if combined
  with side effects.

## Validation for future work (t2f5 GPU dispatch)

The same per-voxel-independent-write invariant carries over to GPU
compute kernels: each thread writes to its own cell in the output
buffer, reads from the input buffer (snapshot). The byte-identity
test SHOULD be re-run on the GPU path before merging.

## See also

- ADR-0020 §Decision v (the per-cycle "single-threaded" → "parallel
  per-voxel" reversal)
- `docs/patterns/cfl-guard-on-anisotropic-stencil.md` — sibling
  pattern for the explicit-FTCS choice that enables this parallelism
- `docs/patterns/caller-owned-scratch-buffer-for-stencil-hot-loop.md`
  — companion pattern; the determinism argument depends on the
  immutable snapshot pattern documented there
- ADR-0019 sidecar sha256 invariant — the load-bearing reason why
  bit-identity matters
