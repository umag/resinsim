---
issue: t2f1-voxelized-cure-distribution
date: 2026-05-19
---

# Pattern: Forward-compat reservation for descoped runtime infrastructure

## Context

A plan promised feature X. Implementation revealed X is too invasive
for this PR. Two options:

1. **Delete the code** that was scaffolding feature X. Public API stays
   consistent with shipped behaviour.
2. **Keep the code** but document it as forward-compat reservation.
   Public API anticipates the future feature; current behaviour ignores
   the reservation.

## Pattern

Use option 2 when:

- The reservation is in the public API surface (struct fields, CLI
  flags, validation paths) — deletion would force the future ticket to
  re-introduce the same shape and risk drift.
- The reservation has its own validation that's correct (the runtime
  just doesn't consume it).
- A specific future ticket / ADR is named as the activator.

Mark with:

- Struct doc comment explaining "v1 implementation: not consulted at
  runtime; reserved for `<next ticket name>`."
- Field-level `#[serde(default)]` (so legacy TOMLs continue to parse).
- Help-text or CLI doc that says "parsed and validated; v1 behaviour
  uses `<fallback>`".
- Tests retained (they prove the reservation is well-shaped).

Do NOT mark with:

- `#[allow(dead_code)]` — there are usually tests using it, so it's
  not dead at the workspace level.
- `#[deprecated]` — it's not deprecated, it's not yet active.

## Examples in resinsim

- `ResinProfile.cure_kinetics_ea_kj_mol: Option<f32>` with KB-153
  literature midpoint warn-when-None — KB-153 promised per-resin
  calibration; v1 doesn't measure per-resin so the field is reserved.
- `PrinterProfile.voxel_cure_resolution_mm: Option<f32>` —
  ADR-0017 promised CLI > profile > default precedence; v1 uses mask
  resolution; t2f5 (GPU acceleration) activates the chain.

## When to NOT use this

- When the reservation is internal API only (no struct field, no CLI
  flag, no doc commitment) — deletion is cleaner.
- When the reservation has unsound validation — fix or delete; never
  reserve broken code.
- When the activator ticket isn't filed — reservation without a
  successor is "decoration", not "preparation".

## See also

- ADR-0017 §2 "Variable voxel resolution — v1 scope cut" — exemplar
  of the pattern post-Phase-5
- ADR-0017 "Legacy compatibility scope cut" — sibling descope with
  delete-not-reserve choice
