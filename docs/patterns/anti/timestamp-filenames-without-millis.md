---
issue: 12-viz-screenshot-flag
date: 2026-04-27
---

# Anti-pattern: auto-generated filenames with second-resolution timestamps

## Symptom

A tool generates filenames like `<prefix>-<unix-secs>.<ext>` and
treats them as "unique enough". Under typical interactive use this
works for most users most of the time, but rapid back-to-back
invocations (a button clicked twice within a second; a test loop;
a CI matrix running parallel jobs in `/tmp`) produce identical
filenames → silent destructive overwrite.

## Cause

Unix time at second resolution wraps around per-second; collision
probability rises sharply for any workflow that rapidly invokes the
generator. For a UI button (issue 12's Capture-screenshot), "click
two screenshots and compare" is a NORMAL workflow.

## Fix

Append millisecond resolution OR a counter:

```rust
// Millisecond suffix — collision becomes essentially impossible
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| (d.as_secs(), d.subsec_millis()))
    .unwrap_or((0, 0));
let filename = format!("<prefix>-{}-{:03}.<ext>", now.0, now.1);

// Counter alternative — detect existing file, append -2, -3, ...
let mut path = base.clone();
let mut n = 2;
while path.exists() {
    path = base.with_file_name(format!(
        "{stem}-{n}.{ext}"
    ));
    n += 1;
}
```

## When seconds-only IS fine

- One-time per process invocation (CLI tool that exits immediately).
- Filenames carry an additional disambiguator (PID, hostname, hash
  of inputs).
- The collision case has been explicitly accepted as "overwrite is
  the intended behaviour" (e.g., a "current state" snapshot).

## Detection

Manual test: invoke the generator twice within the same second.
Assert distinct filenames OR explicit-overwrite warning.

## See also

- `crates/resinsim-viz/src/screenshot.rs` `default_screenshot_path`
  (issue 12 — known limitation; tracked as a follow-up issue)
- `docs/patterns/capture-and-exit-for-ai-feedback-loop.md` (the
  pattern this anti-pattern hides inside)
