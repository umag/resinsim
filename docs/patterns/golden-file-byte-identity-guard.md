---
issue: reportgenerator-extraction
date: 2026-04-24
---

# Pattern: Golden-file byte-identity guard for behaviour-preserving refactors

## Context

When refactoring a function that produces user-visible output (CLI text,
JSON payloads, log lines, file contents), the contract often is "the output
must not change at all." Existing tests usually grep for one or two
specific fields — they would pass even if the output drifted in ways that
break downstream consumers who do positional or full-string parsing.

Without an explicit byte-identity guard, the reviewer has no way to
distinguish "the refactor is safe" from "the refactor happens not to break
the existing assertions." Adversarial review on this codebase has flagged
this gap on multiple plans.

## Pattern

Land the refactor in two commits:

### Commit 1 — Phase pre-A (golden capture, no production change)

1. Build the unmodified binary / call the unmodified function.
2. Capture its full output for one or more representative inputs into a
   fixture file in the test crate (e.g.
   `tests/fixtures/<feature>.<format>.golden`). Where the inputs include
   per-run state (tmpdir paths, timestamps), use a placeholder like
   `__STL_PATH__` and have the test substitute the actual value before
   comparing.
3. Add an integration test that spawns the binary / calls the function
   with the same inputs and asserts `stdout == fixture` (after
   substitution).
4. **Verify the new test passes against the unmodified production code.**
   This is the baseline: the fixture is correct iff the test passes here.
5. Commit the fixture + test together. The repo is now strictly stricter
   than it was before.

### Commit 2 — Phase A + B (the refactor itself)

Make the production change. The byte-identity tests must stay green; if
they fail, fix the refactor (never the fixture, never the test).

## When to apply

- Extractions that move presentation logic between modules
- Renames that touch logging or error message strings
- Switchovers between equivalent serialization paths (e.g. `json!` macro to
  `Serialize`-derived struct — see anti-pattern note in
  `docs/patterns/anti/serde-json-derived-struct-breaks-field-order.md`)
- Any plan whose acceptance criterion includes "no behaviour change" or
  "byte-identical output"

## When NOT to apply

- Refactors that intentionally change output (then the right artefact is a
  schema test or a snapshot review, not a byte-identity guard)
- Output that legitimately includes non-deterministic content (timestamps,
  random IDs, per-run paths) — first try placeholder substitution; if the
  non-determinism is structural, fall back to format-identity (assert key
  set + line count + label spellings) and call out the downgrade in the
  commit message
- Pure-internal refactors with no user-visible output (use unit tests
  directly on the changed functions)

## Determinism prerequisite

Before capturing the fixture, run the binary twice with identical inputs
and `diff` the outputs. If they differ, the simulation/computation has
non-determinism that the byte-identity guard would surface as flakes.
Either fix the non-determinism (preferred) or downgrade to
format-identity (next-best).

## See also

- `docs/patterns/phase-boundaries-for-ddd-refactors.md` — Phase A/B
  bundling rules for type-coupled refactors. Phase pre-A from the byte-
  identity pattern naturally precedes Phase A.
- `crates/resinsim-inspect/tests/fixtures/report_health_athena_ii.{text,json}.golden`
  and the byte-identity tests in `profile_loader_cli.rs` — the worked
  example from `reportgenerator-extraction`.
