---
issue: t2f5-gpu-acceleration-wgpu
date: 2026-08-15
---

# Pattern: pollster bridge for synchronous wgpu usage

## Context

wgpu's `Instance::request_adapter()`, `Adapter::request_device()`, and
`Buffer::slice().map_async()` are async. The resinsim simulation runner
is fully synchronous — no tokio runtime, no async/await. Making the
whole runner async would be invasive and unnecessary when only three call
sites need the bridge.

## Pattern

Use `pollster::block_on()` (a zero-dependency single-file blocking
executor) at the three async boundary points:

1. `GpuContext::try_new()` — adapter + device creation at startup
2. `GpuThermalBuffers::download()` — `map_async` for GPU→CPU readback

`pollster` is a runtime dependency (not dev-only) gated behind the `gpu`
Cargo feature. It adds no transitive dependencies and compiles in under
a second.

The alternative — adding tokio as a runtime dependency — was rejected
(ADR-0025 §Rejected alternatives (b)) because it would pull in a full
async runtime for three blocking calls.

## When to use

Any synchronous Rust codebase that needs wgpu compute without an async
runtime. The pattern is standard in the wgpu examples and ecosystem.

## See also

- `docs/adr/0025-gpu-acceleration-wgpu.md` — §Decision i, §Rejected
  alternatives (b)
- `crates/resinsim-core/src/services/gpu_context.rs` — the bridge sites
