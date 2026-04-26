---
issue: 03-per-layer-heatmap-overlay
date: 2026-04-26
kind: anti-pattern
---

# Anti-pattern: `ButtonInput::clear()` then `press()` across multiple test ticks

## The anti-pattern

In a Bevy 0.18 test app that omits `InputPlugin` (to avoid window
backend dependencies), simulating multiple key presses across ticks
via:

```rust
for _ in 0..N {
    input.clear();
    input.press(KeyCode::ArrowUp);
    app.update();  // expects just_pressed to fire each iter
}
```

`ButtonInput::clear()` only clears `just_pressed` and `just_released`
— it does NOT remove the key from `pressed`. The next `press()`
checks `if !pressed.contains(input)`; the key is still pressed, so
`just_pressed` is NOT inserted. Only the first iteration fires the
handler; subsequent ticks are no-ops.

## Symptom

Single-press tests pass (handler fires once, asserted state matches).
Multi-press tests fail with the cursor / counter / state stuck at
`+1` from initial regardless of N.

## The fix

Use `reset_all()` (clears `pressed` + `just_pressed` +
`just_released`) between iterations:

```rust
for _ in 0..N {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    input.reset_all();
    input.press(KeyCode::ArrowUp);
    app.update();
}
```

Alternatively, `release()` then `press()`:

```rust
input.release(KeyCode::ArrowUp);  // removes from pressed
input.press(KeyCode::ArrowUp);    // pressed empty → just_pressed inserted
```

## Why InputPlugin would mask this

`InputPlugin` runs `clear_just_pressed()` in `PreUpdate` AND processes
`KeyboardInput` events (which release on key-up). Real input flows
through the plugin's full state machine. Tests that bypass the plugin
must simulate the full lifecycle manually.
