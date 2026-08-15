---
issue: t2f5-gpu-acceleration-wgpu
date: 2026-08-15
---

# ADR-0025: GPU acceleration of the thermal FTCS solver via wgpu

## Status
Accepted (Stage A of issue `t2f5-gpu-acceleration-wgpu`, 2026-08-15).

## Context

ADR-0020 chose explicit FTCS for forward-compatibility with GPU
acceleration: "Explicit stencils are trivially GPU-portable (one kernel
per substep, no thread dependencies)." The CPU solver
(`thermal_diffusion_solver.rs`) is Rayon-parallel and deterministic but
becomes the dominant wall-clock cost at sub-mm voxel resolutions on
Mars-class envelopes. The t2f5 follow-on filed in ADR-0020 §Consequences
lands here.

## Decision

### (i) wgpu + pollster, feature-gated behind `gpu`

wgpu provides a cross-platform compute-shader API (Vulkan / Metal / DX12)
without a display dependency. The `gpu` Cargo feature implies `field-sim`
(ThermalField requires ndarray). `pollster` bridges wgpu's async API
(`request_adapter`, `request_device`, `map_async`) in the synchronous
simulation runner via `pollster::block_on()`.

### (ii) GpuContext domain service

`GpuContext` owns the wgpu `Device`, `Queue`, and a pre-allocated buffer
pool. Construction: `Instance::new(Backends::all())` →
`request_adapter(PowerPreference::HighPerformance, compatible_surface: None)`
→ `request_device`. Headless (no display / no GPU) environments return
`None` from `request_adapter`; the caller falls back to CPU.

### (iii) WGSL compute shader — 7-point FTCS stencil

A single WGSL compute shader implements the same stencil as the CPU
solver (ADR-0020 §Decision i):

```
T_new[i,j,k] = T_old[i,j,k] + dt · α · (Laplacian_6_neighbours − 6·T_old)
```

Boundary conditions (Dirichlet bottom, Robin sides/top) are encoded as
uniforms. The shader reads from one storage buffer and writes to another.

### (iv) Ping-pong double-buffer pattern

Two `wgpu::Buffer` storage buffers (A and B) swap read/write roles each
substep. A bool `current_is_a` tracks which buffer holds the latest
state. After N substeps the host reads back from whichever buffer is
current. This avoids per-substep allocation and keeps the GPU pipeline
fed.

### (v) GPU/CPU numerical parity — tolerance-based

GPU ALU rounding differs from x86 FPU. GPU results are NOT expected to
be byte-identical to CPU results. The parity invariant is:

> max |T_gpu[i,j,k] − T_cpu[i,j,k]| < 1e-3 °C

after N substeps on a reference field. The sidecar sha256 is
per-device-class: a run on GPU produces a different hash than the same
run on CPU. Cross-device comparison must use the tolerance, not the hash.

### (vi) SimulationRunner dispatch

`run_from_layer_inputs_with_voxel` gains an optional `gpu_context`
parameter. When `Some`, `VoxelState` uploads the thermal field to GPU
once per layer, dispatches all substeps on GPU, then downloads once.
When `None`, the existing CPU path runs unchanged. The `--gpu` CLI flag
constructs a `GpuContext` in `cmd_sim` and threads it through.

### (vii) Provenance compute_device field

`Provenance` gains an optional `compute_device: Option<String>` field
(`serde(default)`). Values: `"cpu"` or the wgpu adapter name (e.g.
`"Apple M1 Pro"`). Additive — `schema_version` stays 2.

## Consequences

- The CPU solver (`thermal_diffusion_solver.rs`) is unchanged.
- GPU dispatch is only available via the voxel-mode entry point
  (`run_from_layer_inputs_with_voxel`). STL/area-only paths do not
  create VoxelState and have no thermal field to accelerate.
- Feature-config matrix grows to 5 configs: default, field-sim,
  field-sim+gpu (build), field-sim+gpu (test), plus the existing UAT
  configs.
- Tests that require a GPU adapter use a `try_create_gpu_context()`
  helper and early-return when no adapter is available — never
  `#[ignore]`.

## Rejected alternatives

(a) **CUDA / OpenCL.** Vendor-locked or has a weaker ecosystem. wgpu
covers Vulkan + Metal + DX12 from one API.

(b) **Making the whole runner async.** Invasive; the simulation pipeline
is synchronous end-to-end. `pollster::block_on` at the three wgpu async
points is minimal and sufficient.

(c) **Per-substep GPU↔CPU transfer.** Defeats the point. Upload once,
run N substeps, download once.
