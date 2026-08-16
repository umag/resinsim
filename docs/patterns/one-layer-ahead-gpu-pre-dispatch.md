---
issue: t2f5-gpu-crosstalk-async-pipeline
date: 2026-08-16
---

# Pattern: one-layer-ahead GPU pre-dispatch

When a per-layer GPU compute step is followed by CPU-bound work, the GPU
idles during the CPU phase. Pre-dispatching the next layer's GPU work
AFTER downloading the current layer's result — but BEFORE running the
current layer's CPU work — fills the GPU idle time.

## Shape

1. **Prologue:** first layer runs sequential (no previous to overlap with).
2. **Loop body:** `download(K) → dispatch(K+1) → process(K)`.
3. **Last layer:** no `dispatch(K+1)`, just download and process.

## Constraints

- `poll(Wait)` must only see K's submission (see anti-pattern:
  `wgpu-poll-wait-serialises-all-submissions`).
- The next layer's GPU input must be independent of the current layer's
  CPU output (e.g. intensity grid depends on mask + LED power, not on
  PI/cure state that the CPU is updating).
- Per-layer ordering (cure → thermal → strain) must be preserved within
  each iteration.
- Graceful degradation: if GPU finishes before CPU work, pipelining
  degrades to sequential with no performance penalty.

## Applied in

ADR-0025 Stage F: crosstalk XY convolution pre-dispatch. The outer loop
pre-dispatches layer K+1's GPU conv between cure(K) and thermal(K), so
GPU conv(K+1) overlaps CPU thermal(K) + strain(K).
