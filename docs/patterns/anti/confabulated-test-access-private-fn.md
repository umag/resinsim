---
issue: viz-timeline-toggle-stepdefs
date: 2026-08-16
---

# Anti-pattern: Confabulated test-access claim on a private function

## Context

During plan v1 review for `viz-timeline-toggle-stepdefs`, the plan
claimed step defs would test Y-range behavior via `cursor_label_top_y`.
Both code and adversarial reviewers independently flagged this as HIGH:
the function is `fn` (module-private in `plots.rs`), not `pub`. Even
after making `mod ui` → `pub mod ui` in `lib.rs`, module-private
functions remain inaccessible from integration tests.

## Anti-pattern

Claiming a plan step will "test via function X" or "call function X"
without verifying that X is actually accessible from the test's
compilation unit. The claim passes surface plausibility (the function
exists, its name appears in grep output) but fails when the Rust
visibility rules are applied.

## Why it happens

- The plan author sees the function in a `grep` result and assumes
  `pub mod` transitivity makes it callable.
- `pub struct` / `pub fn` inside a `mod` (not `pub mod`) are public
  within their crate but invisible to integration tests.
- Rust's visibility model differs from languages where "public" means
  "visible everywhere" (Java, Python).

## Fix

Before claiming a plan step will call a specific function from an
integration test:

1. Check the function's own visibility (`fn` vs `pub fn` vs
   `pub(crate) fn`).
2. Check every ancestor module's visibility in `lib.rs` — a `pub fn`
   inside a `mod` (not `pub mod`) is crate-private.
3. If the function is private and the test is an integration test,
   either make it pub or drop the claim and declare debt.

## See also

- `bevy-app-test-seam.md` — the egui caveat section documents which
  functions are testable from integration tests
- `in-process-cucumber-via-pure-projection.md` — the corrected pattern
  that emerged after this anti-pattern was caught
