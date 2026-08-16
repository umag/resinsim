---
issue: viz-async-ctb-load
date: 2026-08-16
---

# Pattern: IoTaskPool for heavy file I/O in Bevy systems

## Context

Bevy 0.18 provides three task pools: `ComputeTaskPool` (CPU-bound parallel
work), `AsyncComputeTaskPool` (CPU-bound async work), and `IoTaskPool`
(I/O-bound work like file reads and network calls).

In resinsim-viz, `parse_ctb` reads and decompresses a 300+ MB binary file.
This is I/O-bound, not compute-bound.

## Pattern

1. Spawn the I/O work on `IoTaskPool::get().spawn(async move { ... })`.
2. Store the `Task<T>` handle in a Bevy `Resource` enum with
   Idle/Loading/Failed variants.
3. Poll with `block_on(poll_once(&mut task))` in an Update system.
4. When `Some(result)`, apply the result to the world on the main thread.
5. On re-trigger (e.g. drag-drop while loading), replace the resource —
   dropping the old `Task` cancels the in-flight work via Rust's drop
   semantics.

## When to use

Any file parse that takes >50 ms and blocks the render loop. The threshold
is subjective; CTB files at 300+ MB take 1–3 s, which is clearly
noticeable.

## When NOT to use

Small files (<1 MB), or files where the parse result must be available
synchronously before the next system runs (e.g. config files at startup).

## First-party example

`crates/resinsim-viz/src/lib.rs::CtbLoadTask`, `spawn_ctb_parse`,
`poll_ctb_load`.
