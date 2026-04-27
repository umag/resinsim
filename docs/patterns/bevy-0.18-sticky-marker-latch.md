---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# Pattern: sticky-marker latch for one-tick Bevy markers

## Problem

Bevy 0.18's screenshot pipeline (and similar one-shot-event systems)
emit a marker component (`Captured` for screenshots, similar shape
for window-close events) and despawn it one tick later via a
`First`-schedule cleanup system:

```rust
// bevy_render-0.18.1/src/view/window/screenshot.rs:410-414
.add_systems(
    First,
    clear_screenshots
        .after(message_update_system)
        .before(ApplyDeferred),
)
```

A consumer system in `Update` queries `(With<Captured>, ...)` and
sees the marker for ONE tick only. The next frame's query is empty
because `First` already despawned the entity.

## Anti-pattern

Naively gating multi-frame state on the live query:

```rust
// BAD — Phase 3 → Phase 2 regression after auto-despawn
if !captured_query.is_empty() {
    // Phase 3 logic — sees marker on tick N
} else if spawn_fired {
    // Phase 2 logic — re-entered on tick N+1 because query is empty
}
```

State machine regresses; downstream logic mis-classifies the case.

## Pattern

Latch the observation in a `Local<bool>` (or a Resource if multiple
systems need to coordinate). Flip true on first observation; never
reset (per-run lifecycle):

```rust
fn capture_system(
    captured_q: Query<(), With<Captured>>,
    mut captured_observed: Local<bool>,
    mut spawn_fired: Local<bool>,
    // ... other state ...
) {
    if *spawn_fired && !captured_q.is_empty() {
        *captured_observed = true;
    }

    // Phase 3 logic gates on the LATCHED flag, not the live query
    if *spawn_fired && *captured_observed {
        // ... persistent Phase 3 work ...
    }
}
```

## Defensive companion: file-landed-before-marker check

`save_to_disk`'s observer fires from `ScreenshotCaptured` event;
the `Captured` marker insertion happens on a different tick.
Theoretical race: file lands BEFORE we observe the marker. Defensive
Phase-2 check: also accept Phase 2 → ExitSuccess if
`file_present_on_disk`, even without `captured_observed`:

```rust
if *spawn_fired {
    if file_present_on_disk {
        // Defensive: file landed before we observed Captured
        return ExitSuccess;
    }
    if *frames_since_spawn > MAX_RENDER_FRAMES {
        return ExitTimeoutRenderHung;
    }
}
```

## Detection

Tests for one-tick markers MUST drive the marker through three frames
to catch the regression:

```rust
#[test]
fn capture_inner_phase_3_sticky_after_auto_despawn() {
    // Frame N: Captured observed
    let d1 = drive(&mut s, &args, true, /*captured*/ true, /*file*/ false);
    // Frame N+1: Captured query empty (clear_screenshots ran)
    let d2 = drive(&mut s, &args, true, /*captured*/ false, /*file*/ false);
    // Frame N+2: file lands → ExitSuccess (NOT ExitTimeoutRenderHung)
    let d3 = drive(&mut s, &args, true, /*captured*/ false, /*file*/ true);
    assert_eq!(d3, CaptureDecision::ExitSuccess);
}
```

Single-frame tests miss this entirely.

## See also

- `crates/resinsim-viz/src/screenshot.rs` `capture_inner` (issue 12 —
  the canonical instance of this pattern)
- Bevy source: `bevy_render-0.18.1/src/view/window/screenshot.rs:78-84`
  (Capturing / Captured component definitions)
- `docs/patterns/anti/bevy-app-exit-return-discarded.md` (sibling KB
  — same family of "Bevy semantics needed at the runner level")
- `docs/patterns/capture-and-exit-for-ai-feedback-loop.md` (the
  end-to-end pattern that depends on this latch working correctly)
