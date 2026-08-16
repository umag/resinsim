---
issue: t2f5-gpu-acceleration-wgpu
date: 2026-08-15
---

# ADR-0025: GPU acceleration via wgpu

## Status
Accepted (Stage A: thermal FTCS, 2026-08-15; Stage B: cure + PI column
march, 2026-08-15; Stage C: XY crosstalk convolution, 2026-08-15).

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

## Stage B: GPU-accelerated cure + PI column march

### (viii) WGSL cure column-march shader

A single WGSL compute shader (`voxel_cure_gpu.rs`) implements the same
Beer-Lambert column march as the CPU `VoxelCureCalculator` (ADR-0017).
Each thread owns one (ix, iy) column and marches Z sequentially from
`iz_top` to `nz`. No cross-thread data dependency — no ping-pong needed.
Workgroup size is `@workgroup_size(8, 8)`, dispatched as
`ceil(nx/8) × ceil(ny/8)` workgroups per layer. Regime AA only (no
crosstalk); the crosstalk regimes (BA/BB/CB/DD) remain CPU-only.

The shader replicates the CPU path's constants exactly:
- `C_THRESHOLD = 0.01` (KB-160 numerical floor)
- `DP_LOCAL_MAX_FACTOR = 10.0` (extrapolation cap)
- `NEGLIGIBLE_DOSE_FLOOR = 1e-6` (early-exit threshold)
- PI depletion: `clamp(exp(-k_d * dose), 0, 1)` then
  `clamp(c_old * multiplier, 0, c_old)` (double-clamp from
  `PhotoinitiatorField::deplete`)
- Depth uses `layer_height_um` NOT `voxel_size_mm * 1000`

### (ix) GpuCureBuffers

`GpuCureBuffers` manages four GPU storage buffers (cure, PI, intensity,
params) plus two staging buffers for download. Cure and PI are uploaded
once at init and persist on GPU across layers. Per layer: the host
builds a 2D intensity grid (mask × uniformity × LED power), uploads it,
dispatches, then downloads cure + PI back to host (shrinkage pass reads
`dose_at` per layer).

Post-download sanity checks: `max_dose().is_finite()` and
`min_concentration() >= 0`.

### (x) GPU/CPU parity for cure + PI

Same tolerance-based contract as Stage A (§v). Cure dose tolerance:
max |dose_gpu − dose_cpu| < 1e-3 mJ/cm². PI concentration tolerance:
max |pi_gpu − pi_cpu| < 1e-5. Four parity tests: basic 4×4×8 grid,
zero-intensity noop, depleted column (C near C_THRESHOLD), single-voxel
nz=1.

### (xi) SimulationRunner dispatch

`VoxelState` gains `gpu_cure: Option<GpuCureBuffers>`, initialized
alongside `gpu_thermal` when `--gpu` is passed.
`apply_voxel_cure_for_layer` checks `gpu_cure` first; when `Some`,
builds the intensity grid on CPU, dispatches the shader, downloads,
sanity-checks, and returns. When `None`, the existing CPU loop runs
unchanged. Crosstalk regimes always take the CPU path.

---

## Stage C: GPU-accelerated XY light crosstalk convolution

### Context

ADR-0018 defines a separable 2D Gaussian convolution (X-pass then Y-pass)
on the per-layer intensity grid as the XY pre-attenuation stage of light
crosstalk. The CPU implementation (`LightCrosstalkCalculator::apply_separable_2d`)
is O(nx × ny × kernel_len × 2) per layer. For Mars 5 Ultra (3840 × 2400 =
9.2M pixels) this is the dominant per-layer cost in the crosstalk path.

### Decision

#### (viii) XY-only GPU, Z stays CPU

Two WGSL compute shaders (`conv_x`, `conv_y`) implement the same 1D
separable convolution as the CPU path. The Z post-attenuation convolution
stays on CPU because:

- Dose columns are computed per-pixel AFTER the sequential Beer-Lambert
  column march (`compute_column_exposure`), which reads PI field state
  and cannot move to GPU.
- Batching all pixels' dose columns for GPU dispatch requires
  nx × ny × nz floats (Mars 5 Ultra: 3840 × 2400 × 4492 = 41G floats
  = 165 GB) — exceeds any GPU's memory.
- Z conv is O(nz × kernel_len) per pixel with a typical 7-tap kernel —
  fast enough on CPU.

#### (ix) Two-buffer swap, not ping-pong

The XY conv is exactly two passes (X then Y), not N substeps. Buffer A
holds the uploaded intensity grid. X-pass reads A, writes B. Y-pass reads
B, writes A. Result is in A. No `current_is_a` tracking needed.

#### (x) Post-download NaN guard

WGSL shaders cannot signal errors. The CPU path has explicit `is_finite()`
checks (`CrosstalkError::NonFiniteInput`). The GPU path validates the
downloaded intensity grid with `iter().all(is_finite)` before the CPU
per-pixel Beer-Lambert loop. Returns `Err` on any non-finite value.

#### (xi) Per-run buffer allocation

`GpuCrosstalkBuffers` is allocated once at `VoxelState` construction
(alongside `GpuThermalBuffers`) and reused across layers. Only the
intensity data is uploaded per layer; kernel weights and params are
written per-dispatch via `queue.write_buffer`.

#### (xii) 2D workgroup dispatch

Same pattern as the thermal solver: `wg_x = min(total_workgroups, 65535)`,
`wg_y = ceil(total_workgroups / wg_x)`. Required for Mars 5 Ultra
(9.2M pixels / 64 = 143,750 workgroups > 65535 limit).

#### (xiii) Clamp-to-zero edge policy

Both WGSL shaders implement the SKIP policy from ADR-0018 §2:
out-of-bounds samples contribute zero. The `if src_ix >= 0 && src_ix <
i32(params.nx)` guard matches the CPU `continue` on out-of-bounds.

### Consequences

- The CPU `LightCrosstalkCalculator` is unchanged.
- GPU crosstalk dispatch only fires when σ_xy is `Some` (regimes BA/BB/DD).
  Regime CB (σ_xy None, σ_z Some) uses CPU-only Z conv, no GPU dispatch.
- Regime AA (both None) takes the t2f1 fast path — no crosstalk code at all.
- GPU parity tests follow the `gpu_thermal_parity.rs` pattern: tolerance-based,
  graceful skip when no adapter.

### Rejected alternatives

(d) **GPU Z convolution.** Rejected: dose columns are per-pixel
post-Beer-Lambert; batching requires nx × ny × nz memory (prohibitive).

(e) **Full 3D tensor convolution on GPU.** Rejected for v1 per ADR-0018
§4(a): layers are exposed at different print steps, so no natural
full-print intensity tensor exists.

---

## Stage E: Batched command encoder + async layer pipelining

### Context

Stages A–D introduced four GPU compute shaders (thermal, cure, crosstalk,
strain/stress), each with its own `dispatch()` method that creates a
`CommandEncoder`, records a compute pass, and calls `queue.submit()`
independently. In the per-layer loop this produces up to 7 separate
`submit()` calls per layer. The cure and strain/stress dispatches have a
data dependency (strain reads cure dose) but no cross-dispatch GPU data
transfer — the CPU round-trips through `upload_dose()`.

### Decision

#### (xiv) Combined cure+strain encoder (regime AA only)

`GpuCureBuffers` and `GpuStrainStressBuffers` gain `encode_*_pass()`
methods that record compute passes onto an external `CommandEncoder`
without submitting. The runner creates one encoder per layer:

1. `encode_cure_pass` — cure column march
2. `encoder.copy_buffer_to_buffer(cure_buf, dose_buf)` — on-GPU dose copy
3. `encode_strain_stress_pass` — strain/stress compute

All three operations submit in a single `queue.submit()`. wgpu guarantees
sequential execution of compute passes within a single command buffer,
satisfying the cure→strain data dependency.

This applies only to regime AA (no crosstalk). GPU crosstalk (regimes
BA/BB/DD) and GPU cure are mutually exclusive code branches — the
crosstalk path remains a separate submission.

#### (xv) On-GPU dose buffer copy

`GpuStrainStressBuffers::encode_copy_dose_from(encoder, cure_buf)`
records a `copy_buffer_to_buffer` from the cure storage buffer to the
strain/stress dose buffer. Both are `nx*ny*nz` f32 arrays with matching
byte sizes. This eliminates the CPU round-trip through `upload_dose()`.

#### (xvi) write_intensity + cure_buf accessor

`GpuCureBuffers::write_intensity()` stages the per-pixel intensity grid
via `queue.write_buffer()` without dispatching. `cure_buf()` exposes the
cure storage buffer handle for the on-GPU dose copy.

#### (xvii) wgpu re-export

`lib.rs` re-exports `wgpu` under `#[cfg(feature = "gpu")]` so
integration tests can construct `CommandEncoder` instances directly.

### Consequences

- Per-layer `submit()` calls reduced from ~7 to ~2 (combined cure+strain,
  plus thermal which retains its own batched-substep encoder).
- `upload_dose()` is superseded on the combined path but retained for
  standalone use in tests and non-batched callers.
- GPU/CPU parity tests extended: 4 new tests in
  `gpu_combined_pipeline_parity.rs` (single-encoder, multi-layer 8,
  1-layer boundary, double-buffer swap isolation).
- GPU crosstalk path is unchanged — no new submission batching there.

### Rejected alternatives

(f) **Batch crosstalk+cure+strain in one encoder.** Rejected: GPU
crosstalk and GPU cure are mutually exclusive code branches (crosstalk
fires only when σ_xy is active, in which case cure uses the CPU path).
No regime exists where both GPU crosstalk and GPU cure run in the same
layer.

(g) **Async compute queues / multi-queue.** Rejected: wgpu does not
expose async compute queues or multi-queue dispatch.

---

## Stage F: Async crosstalk layer pipelining

### Context

Stage C's GPU crosstalk dispatch (`GpuCrosstalkBuffers::apply_separable_2d`)
runs upload → conv_x → conv_y → copy-to-staging → submit → poll(Wait) →
download synchronously per layer. In the per-layer loop, each layer blocks
on GPU completion before proceeding to thermal and strain. The GPU is idle
during CPU-bound thermal diffusion and strain/stress computation.

### Decision

#### (xviii) Split apply_separable_2d into begin_dispatch + finish_download

`begin_dispatch` uploads intensity, encodes X/Y passes, copies to staging,
and calls `queue.submit()` — non-blocking. `finish_download` calls
`map_async` + `poll(Wait)` + copy — blocking. `apply_separable_2d` is
retained as a convenience wrapper.

#### (xix) One-layer-ahead pre-dispatch in the outer loop

After `apply_voxel_cure_for_layer` completes for layer K, the outer loop
builds the intensity grid for layer K+1 and calls `begin_dispatch`. The
GPU runs conv(K+1) while the CPU runs thermal(K) + strain(K). On
entering `apply_voxel_cure_for_layer_crosstalk` for K+1, the function
downloads the pre-dispatched result instead of dispatching fresh.

#### (xx) download → dispatch → process ordering

`poll(Wait)` waits for ALL pending GPU submissions. To prevent
serialisation, the pre-dispatch for K+1 fires AFTER K's download
completes (within the cure step), so `poll(Wait)` for K has no other
pending work. The GPU then works on K+1 while the CPU runs thermal +
strain for K.

#### (xxi) Graceful degradation

For the first crosstalk layer (no pre-dispatch) and the last layer (no
K+1 to dispatch), the path degrades to sequential dispatch + download
with no performance loss. Single-layer prints take the one-shot path.

### Consequences

- Supersedes Stage E consequence "GPU crosstalk path is unchanged".
- Overlap window: GPU conv(K+1) during CPU thermal(K) + strain(K).
  For small grids or fast GPUs the GPU finishes before the CPU work
  starts — pipelining degrades to sequential, never slower.
- GPU parity tests extended: 3 new tests in `gpu_crosstalk_parity.rs`
  (begin/finish parity, multi-layer pipelined, single-layer degenerate).
- Benchmark extended with pipelined multi-layer throughput comparison.
- `VoxelState` gains `crosstalk_gpu_dispatched: bool` tracking whether
  a pre-dispatch is pending.
