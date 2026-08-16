---
issue: viz-ctb-fixture-synthesis
date: 2026-08-16
---

# Pattern: Env-gated fixture with trivial-pass step functions

## Context

A cucumber step-def needs a large binary fixture (e.g. a 356 MB `.ctb`
slicer output) that cannot be committed to the repo. The fixture is
available locally via an env var (`RESINSIM_SLICED_FIXTURE`), but the
harness's debt register must be a constant (or at least deterministic)
regardless of whether the env var is set.

## Pattern

Register step functions for ALL scenario lines (Given/When/Then) so
cucumber-rs never sees an "undefined" step — the scenario is always
PASSED, never "skipped". In the When step, check the env var:

- **Set**: load the real fixture, invoke the binary, populate
  `world.last`. Subsequent Then steps fire real assertions.
- **Absent**: set `world.fixture_skipped = true`, do NOT invoke the
  binary, do NOT populate `world.last`. Subsequent Then steps check
  `fixture_skipped` and return early (trivial pass).

The debt register entry for the spec is REMOVED (0 skipped scenarios)
regardless of env var presence. The register is constant again.

## Cascade obligation

Every shared Then step that accesses `world.last.as_ref().expect(...)`
MUST check `fixture_skipped` first. Missing the guard on even one
shared step causes a panic when the env var is absent — the scenario
"passes" trivially through its own steps but panics on a shared step
that didn't get the guard.

Convergent finding: both code and adversarial reviewers independently
flagged this (viz-ctb-fixture-synthesis plan v1 → v2). The guard is
needed on ALL shared steps, not just the one the plan initially named.

## When to use

- The fixture is too large to commit (binary assets, real-world data)
- The scenario's Given/When/Then prose is not env-var-specific (unlike
  `Given a fixture .ctb file at $RESINSIM_SLICED_FIXTURE` which names
  the env var explicitly)
- The debt register must remain deterministic

## When NOT to use

- The fixture can be committed (small, text-based)
- The scenario's prose already names the env var — then the Given step
  can be truly conditional (registered only when the var is set)
- A cargo feature flag can gate the step functions at compile time
  (use `#[cfg]` instead, per core's `field-sim` pattern)

## See also

- `docs/patterns/synthesise-archive-fixture-not-committed-binary.md` —
  the complementary pattern for synthesising archive fixtures in-test
- `docs/patterns/per-spec-runtime-skip-attribution.md` — the debt
  register design this pattern preserves
