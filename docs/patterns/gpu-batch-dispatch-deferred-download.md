---
issue: t2f5-gpu-cure-stage-b
date: 2026-08-15
---

# Pattern: Batch GPU dispatches with deferred download

## Context

A per-layer GPU dispatch + download cycle pays the PCIe/Metal DMA
round-trip on every layer. On AMD Radeon Pro 5500M, the round-trip
(`copy_buffer_to_buffer` → `submit` → `map_async` → `poll(Wait)` →
`get_mapped_range` → `unmap`) dominates wall-clock: a 500x500 cure grid
at 50 layers spent 8.7 s on GPU with per-layer download vs 1.1 s with
batch=32 (two downloads total). The dispatch itself is nearly free once
the command is queued.

## Pattern

Accumulate N GPU dispatches before downloading. The GPU processes them
in queue order, accumulating results in the same storage buffers. One
download at the batch boundary brings all N layers' data to the host.
Downstream CPU work (shrinkage, stress, layer caches) is deferred into
a batch and runs after the single download.

Key constraint: any per-layer state that the deferred work needs must be
**snapshotted at dispatch time**, not read from the field at flush time.
In the cure path, `thermal.volume_mean_c()` evolves between layers
(thermal solver runs independently per layer), so each deferred layer
carries its own `mean_t_c` snapshot for the Ec(T) computation.

## Measured impact (AMD Radeon Pro 5500M, 500x500x50)

| Batch | Downloads | GPU time | Speedup vs CPU |
|-------|-----------|----------|----------------|
| 1     | 50        | 8.7 s    | 7.8x           |
| 8     | 7         | 1.8 s    | 37x            |
| 32    | 2         | 1.1 s    | 44x            |
| 50    | 1         | 1.0 s    | 47x            |

## When NOT to use

When downstream CPU work has a cross-layer dependency that requires
intermediate GPU state — e.g. a solver whose layer N+1 input depends on
layer N's GPU-computed output. In that case, download after every layer
(batch=1). The cure path has no cross-layer dependency in the cure
dispatch itself; only shrinkage reads the cure field, and shrinkage at
layer L reads only layer L's own dose.

## See also

- `docs/patterns/gpu-cure-column-march-no-pingpong.md`
- `docs/patterns/gpu-ping-pong-double-buffer-for-stencil-solvers.md`
- `docs/adr/0025-gpu-acceleration-wgpu.md` §(viii)–(xi)
