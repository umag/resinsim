---
issue: viz-lib-main-split
date: 2026-08-16
---

# lib.rs + thin main.rs split for testable Bevy crates

## Context

A Bevy application structured as a binary-only crate (`main.rs`, no
`lib.rs`) cannot be imported by its own integration test harness for
in-process driving. The test binary cannot access the crate's systems,
resources, or types — only subprocess-based testing is possible.

## Pattern

Split the binary into `lib.rs` + thin `main.rs`:

1. **lib.rs** owns all modules, systems, resources, components, and a
   `pub fn run(args: Args) -> AppExit` that encapsulates `App` construction
   and `app.run()`.

2. **main.rs** (~20-30 lines) parses CLI args, performs any pre-`App`
   validation (e.g. `--screenshot` path validation that uses `eprintln`
   because `LogPlugin` isn't initialized yet), calls `run(args)`, and
   translates `AppExit::Error` to `process::exit()`.

3. **run() returns `AppExit`, never calls `process::exit()`**. This is the
   load-bearing invariant: the UAT harness inspects the returned `AppExit`
   without process termination. `Result<(), Box<dyn Error>>` loses the exit
   code; `process::exit()` inside `run()` kills the test process.

4. **Visibility widening is compile-driven**: promote `pub(crate)` to `pub`
   only when `cargo check` errors. System fns stay `pub(crate)` since
   `run()` encapsulates them — only `Args`, resources/components the harness
   queries, and exit-code consts the binary uses need `pub`.

## Why not `Result`?

`AppExit::Error` carries a `NonZero<u8>` exit code. Mapping to
`Result<(), Box<dyn Error>>` loses the code — the caller can't distinguish
exit code 2 from exit code 6. The binary needs the exact code for
`process::exit()`; the harness needs it for assertions.

## Pre-App validation stays in main.rs

Anything that runs before `App::new()` — like `--screenshot` path
validation that uses `eprintln!` because Bevy's `LogPlugin` isn't
initialized — must stay in `main.rs`. `run()` receives already-validated
args.
