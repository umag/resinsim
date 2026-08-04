---
issue: uat-unskip-a3-b
date: 2026-08-04
---

# Pattern: Synthesise the archive fixture, don't commit the binary

## Context

A test needs a container-format input (here: `.nanodlp` = ZIP holding
JSON metadata + PNG slices + a gzipped CSV log) whose CONTENT must make a
scenario's premise true — e.g. "the real force peaks at a different layer
than the sim" needs a late-peaking area profile against an early-peaking
log. The committed reference fixture (`mini.nanodlp`) cannot express the
premise, and committing more opaque binaries hides what property each one
carries and rots silently when the format evolves.

## Pattern

Build the archive in-test from a parameterised builder
(`uat_steps/fixtures.rs::NanoDlpJobBuilder`):

- Parameters ARE the premises: per-layer lit-pixel counts drive the
  predicted force shape; `SupportLayerNumber`/exposures drive the recipe
  mapping; an optional tall `ID,T,V` body supplies the "real" log. The
  test that needs `|peak offset| >= 3` says so in its arguments.
- Mirror the committed reference fixture's entry names and JSON key
  spellings exactly, so the synthesised archive exercises the SAME
  production parse branches the real format does — the builder encodes no
  force model, no exposure branch, no area formula.
- Reuse committed provenance where it exists: the analytic body is
  `include_str!`-ed from the committed `synthetic_stepped_forces.csv`
  rather than re-typed, so one provenance serves both the athena module
  and the calibrate variant.
- Probe before pinning: run the real CLI against the synthesised fixture
  ONCE and record the observed numbers in the doc comment before writing
  assertions (`nanodlp_calibrate_compares_real_force.rs::late_peaking_variant`
  records offset +5, R² 0.000). If the premise does not hold, adjust the
  FIXTURE, never the assertion.
- Write only under `CARGO_TARGET_TMPDIR` with a unique per-call suffix;
  use `CompressionMethod::Stored` so no optional zip feature is assumed.

## When NOT to use

- The premise is expressible with the committed reference fixture — use
  it (calibrate UAT-1 uses `mini.nanodlp` unchanged).
- The builder's dependencies are not reachable from the test target — the
  fallback is a SMALL committed binary WITH a regeneration recipe in the
  module doc (`athena_fixture_roundtrip.rs`'s documented `gzip -9 -n -k`
  precedent), never a new dev-dependency added ad hoc.

## See also

- `docs/patterns/anti/fixture-copy-of-shared-builder.md` — the builder
  lives in ONE shared home; modules compose it, never fork it
- `docs/patterns/golden-file-byte-identity-guard.md` — the probe-then-pin
  discipline the premise probe follows
