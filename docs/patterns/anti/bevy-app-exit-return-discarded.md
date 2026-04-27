---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# Anti-pattern: discarding `app.run()`'s `AppExit` return value

## Symptom

Calls to `MessageWriter<AppExit>::write(AppExit::Error(_))` (directly
or via a helper like `fatal_exit`) appear to fire correctly — unit
tests asserting on the `Messages<AppExit>` buffer pass — but the
binary exits with code 0 in production. CI scripts that branch on
`$?` see "success" for every load failure.

## Cause

Bevy 0.18 changed `App::run()` to return the final `AppExit`. The
runner's `should_exit()` correctly picks the first error (per
`bevy_app-0.18.1/src/app.rs:1342`), but `main()` must do something
with the return value — it's not auto-honoured by the process.

```rust
// Anti-pattern: silently exits 0 regardless of fatal_exit calls
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    // ... systems queue AppExit::Error(NonZero::new(6)) on bad CTB ...
    app.run();   // <-- AppExit dropped on the floor
    Ok(())
}
```

## Fix

Pattern-match the return and route `AppExit::Error` through
`std::process::exit`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    // ...
    let exit = app.run();
    if let AppExit::Error(code) = exit {
        // SAFETY: by the time app.run() returns, Bevy's runner has
        // fully cleaned up (windows closed, GPU device dropped,
        // render thread joined). Only leak-safe stdlib locals
        // remain. If you add an RAII handle here in the future
        // (tracing subscriber guard, profiler scope), drop it
        // BEFORE process::exit.
        std::process::exit(code.get() as i32);
    }
    Ok(())
}
```

Alternative: change `main()`'s signature to `Result<ExitCode, _>`
and `Ok(ExitCode::from(code.get()))` — preserves the destructor
chain but loses `?` ergonomics on the `Box<dyn Error>` arm.

## Why unit tests didn't catch it

`load_ctb_into_world_emits_exit_6_when_smoke_exit_and_ctb_unreadable`
asserts on the `Messages<AppExit>` buffer:

```rust
let messages = app.world().resource::<Messages<AppExit>>();
let exits: Vec<&AppExit> = cursor.read(messages).collect();
assert!(exits.iter().any(|e| matches!(e, AppExit::Error(c) if c.get() == 6)));
```

This passes because the message IS written. The runner-level race
(AppExit::Success from `smoke_exit_after_one_frame` queued on the
SAME frame) AND the missing main()-return-handling were both
invisible to unit tests. **End-to-end exit-code testing requires
actually running the binary and reading `$?`.**

## Detection

Add a manual-gate check for any new exit-code surface:

```bash
./target/debug/<binary> --flag-that-should-fail; echo "exit: $?"
```

If `$?` is 0 when it should be non-zero, the exit-code contract is
broken — most likely either main() discards `app.run()` or a
competing AppExit::Success arrived on the same frame.

## See also

- `docs/patterns/bevy-0.18-sticky-marker-latch.md` (sibling KB —
  Bevy 0.18 schedule precedence requires equally-careful handling
  for one-tick markers)
- `docs/patterns/capture-and-exit-for-ai-feedback-loop.md` (the
  end-to-end pattern that motivated discovery of this bug)
- `docs/adr/0013-screenshot-exit-code-disjunction.md` (the
  exit-code contract this anti-pattern would have silently broken)
- Bevy source: `bevy_app-0.18.1/src/app.rs:1342` (should_exit)
- Bevy source: `bevy_winit-0.18.1/src/state.rs:734` (winit_runner
  consumes the return value)
