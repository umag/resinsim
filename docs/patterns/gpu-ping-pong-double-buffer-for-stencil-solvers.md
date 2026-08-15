---
issue: t2f5-gpu-acceleration-wgpu
date: 2026-08-15
---

# Pattern: GPU ping-pong double-buffer for stencil solvers

## Context

Explicit finite-difference stencils (FTCS, Lax-Wendroff, etc.) read from
an immutable snapshot of the prior state and write the updated state. On
CPU, `thermal_diffusion_solver.rs` uses a caller-owned scratch buffer for
the snapshot and writes back in-place. On GPU, in-place read-write from
the same buffer produces race conditions — threads in one workgroup may
read a cell another workgroup has already overwritten.

## Pattern

Allocate two `wgpu::Buffer` storage buffers (A and B) at init time. Each
substep's compute pass binds one as `read` and the other as
`read_write`, then swaps. A `bool current_is_a` tracks which buffer
holds the latest state. After N substeps, the host reads back from
whichever buffer is current.

Key implementation points:

- **Two bind groups, not one.** Create `bind_group_a_to_b` (read A,
  write B) and `bind_group_b_to_a` (read B, write A) at init. Switching
  bind groups per substep is a single `set_bind_group` call — no buffer
  rebinding or reallocation.
- **Batched command submission.** All N substeps go into a single
  `CommandEncoder` with N compute passes, submitted once at the end.
  This eliminates per-substep CPU-GPU synchronization — measured 15x
  improvement over per-substep submission on AMD Radeon Pro 5500M.
- **Odd/even invariant.** After an odd number of substeps the result is
  in buffer B (not A where it started). The download must read from the
  current buffer. A parity test (`gpu_cpu_parity_odd_substep_count`)
  pins this invariant.
- **Upload once per layer, not per substep.** BCs are constant within a
  layer's substep loop (the LED temperature is evaluated at the layer's
  end time). Upload the thermal field and the BC uniform buffer once
  before the dispatch loop, download once after.

## When to use

Any explicit stencil solver ported to GPU compute. The same pattern
applies to the cure calculator's per-pixel Beer-Lambert pass (future
Stage B) and the light crosstalk convolution (future Stage C).

## When NOT to use

Implicit solvers (ADI, Crank-Nicolson) with tridiagonal solves — these
have sequential dependencies along each line and do not map to the
independent-cell dispatch model that makes ping-pong work.

## See also

- `docs/adr/0025-gpu-acceleration-wgpu.md` — decision record
- `docs/adr/0020-spatial-thermal-diffusion.md` — FTCS chosen for GPU
  portability (§Decision i)
- `crates/resinsim-core/src/services/thermal_diffusion_gpu.rs` — the
  implementation
- `crates/resinsim-core/tests/gpu_thermal_parity.rs` — parity and
  invariant tests
