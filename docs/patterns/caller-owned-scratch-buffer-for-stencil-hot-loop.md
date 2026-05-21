---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: Caller-owned scratch buffer for stencil hot loops

## Context

Explicit FTCS-style stencil solvers need to snapshot the previous
step's state before computing the next step (the stencil reads from
the snapshot to avoid read-after-write). The naive design owns the
snapshot inside the step function:

```rust
pub fn step(field: &mut Array3<f32>, ...) {
    let t_old = field.clone();  // ALLOCATES every call
    // ... compute t_new into field from t_old ...
}
```

This allocates + frees a multi-MB Array3 on every substep call. For
Mars 5 Ultra (16 M voxels × 4 B = 64 MB at 0.5 mm voxel, ~30
substeps per layer × ~100 layers = 3000 substeps), the allocator
churn dominated wall-clock time (4000+ s observed before fix).

## Pattern

**Caller owns the scratch buffer, allocated ONCE outside the substep
loop.** The step API takes `&mut Array3<f32>` for the scratch; the
ndarray-native `assign` method reuses the existing allocation:

```rust
pub fn step(
    field: &mut ThermalField,
    scratch: &mut Array3<f32>,  // caller-owned, dims match field
    ...
) -> Result<(), Error> {
    debug_assert_eq!(scratch.dim(), field.dimensions_as_tuple());
    scratch.assign(&field.as_array_view());  // REUSES allocation
    let t_old = &*scratch;
    // ... compute new values into field from t_old ...
}
```

The caller (orchestrator) allocates once:

```rust
struct VoxelState {
    thermal: ThermalField,
    thermal_scratch: ndarray::Array3<f32>,  // allocated once at init
    // ...
}

// Per-layer loop:
for layer in layers {
    for _ in 0..substeps_per_layer {
        ThermalDiffusionSolver::step(
            &mut state.thermal,
            &mut state.thermal_scratch,
            dt, alpha, &bcs,
        )?;
    }
}
```

## Cost-benefit

Measured for ThermalDiffusionSolver::step at Mars 5 Ultra envelope,
0.5 mm voxel, 60-layer voxel-cure-thermal integration test:

| Design | Wall-clock | Reason |
|--------|-----------|--------|
| Per-step `field.clone()` | > 4000 s (killed) | Allocator thrashing |
| Caller-owned scratch + `assign` | 79 s | Single allocation, then memcpy |

The cost is API ergonomics — callers must allocate the scratch
explicitly, and the dim invariant `scratch.dim() == field.dim()`
becomes the caller's responsibility (with `debug_assert!` as
belt-and-braces). `ndarray::assign` panics on shape mismatch in
release too, so memory corruption is not a risk.

## Safety net

If the scratch buffer's dims drift from the field's (e.g. someone
resizes the field but not the scratch), `ndarray::assign` panics on
the first call. That's a clean failure mode — not silent memory
corruption.

## When this pattern does NOT apply

- Solvers that don't need a snapshot (in-place updates without
  read-after-write — rare for explicit time-stepping).
- Single-call APIs where amortising allocation across calls saves
  nothing.
- Variable-size grids — the scratch can't be reused if dimensions
  change between calls.

## See also

- ADR-0020 §Decision v — the bit-identity invariant that makes the
  scratch+Rayon design viable.
- `docs/patterns/rayon-per-voxel-stencil-determinism.md` — sibling
  pattern; the determinism argument depends on the immutable
  snapshot pattern documented here.
