---
issue: t2f5-gpu-provenance
date: 2026-08-15
---

# Pattern: GPU adapter selection by name substring

## Context

Multi-GPU machines (e.g. discrete + integrated, or multiple discrete
cards) expose several wgpu adapters. The default `request_adapter` with
`PowerPreference::HighPerformance` picks one heuristically, but the user
may need a specific adapter — for reproducibility, for targeting a
particular device class, or because the heuristic picked wrong.

Exact-name matching is fragile: adapter names include driver versions
and vendor strings that change across OS updates. Index-based selection
is opaque and order-dependent.

## Pattern

`GpuContext::try_new_with_adapter_substring(substring)` enumerates all
wgpu adapters via `Instance::enumerate_adapters(Backends::all())` and
selects the first whose `AdapterInfo.name` contains the given substring
(case-insensitive). When no adapter matches, returns `None`; the caller
falls back to CPU and logs available adapter names so the user can pick
a valid substring.

The CLI flag `--gpu-adapter <SUBSTRING>` (requires `--gpu`) surfaces
this to the user. The flag uses clap's `requires = "gpu"` so it cannot
be passed without `--gpu`.

## When to use

Any wgpu-based CLI that supports multi-GPU selection. The substring
approach trades precision for resilience to name churn.

## See also

- `docs/adr/0025-gpu-acceleration-wgpu.md` — §vii, provenance
  `compute_device` field
- `docs/patterns/pollster-bridge-for-sync-wgpu.md` — the async bridge
  pattern used by `try_new_with_adapter_substring`
- `crates/resinsim-core/src/services/gpu_context.rs` — implementation
