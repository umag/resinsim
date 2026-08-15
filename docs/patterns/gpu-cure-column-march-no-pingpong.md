---
issue: t2f5-gpu-cure-stage-b
date: 2026-08-15
---

# Pattern: GPU cure column march needs no ping-pong

## Context

ADR-0025 Stage A's thermal FTCS solver uses a ping-pong double-buffer
pattern: two storage buffers swap read/write roles each substep because
every voxel reads its six neighbours from the previous substep's state.
Without the swap, a thread would read a neighbour that another thread
already overwrote in the same pass — a race condition.

Stage B's cure + PI column march has no such dependency. Each (ix, iy)
thread owns its Z column exclusively and never reads from another
thread's column. The march is sequential within the column (top to
bottom), and each voxel's cure dose and PI depletion are written
in-place.

## Pattern

When a compute shader has no cross-thread data dependency — each thread
reads and writes only its own slice of the output buffer — use a single
read-write storage buffer instead of ping-pong. This halves the GPU
buffer count and eliminates the bind-group swap overhead.

The cure column march dispatches as `@workgroup_size(8, 8)` over
`ceil(nx/8) x ceil(ny/8)` workgroups. Each thread:
1. Reads its pixel intensity from a per-layer input buffer
2. Marches Z from `iz_top` to `nz`, reading/writing `cure[]` and `pi[]`
   at its own `(ix, iy, iz)` index only

No barrier, no swap, no second buffer.

## When NOT to use

Stencil solvers (thermal diffusion, Laplacian, convolution) where each
output voxel depends on its neighbours' previous-step values. Those need
ping-pong or a scratch copy to avoid read-after-write races.

## See also

- `docs/patterns/gpu-ping-pong-double-buffer-for-stencil-solvers.md`
- `docs/adr/0025-gpu-acceleration-wgpu.md` §(viii)
