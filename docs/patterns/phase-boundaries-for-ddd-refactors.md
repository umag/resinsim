---
issue: resin-recipe-model
date: 2026-04-21
---

# Pattern: Phase A (additive) / Phase B (switchover) boundaries for type-forced DDD refactors

## Context

DDD refactors on strongly-typed codebases (Rust, Kotlin, TypeScript strict) often require
moving a field from one aggregate to another. The type system then forces every caller to
be updated simultaneously: splitting the work across commits leaves intermediate states
where the workspace does not compile.

Example from resin-recipe-model (ADR-0005): `layer_height_um` moved from `PrinterProfile`
to `Recipe` (nested in `ResinProfile`). The removal + addition + every `predict_layer` /
`slice_areas` / CLI caller update are atomically coupled.

## Pattern

Label the refactor's steps explicitly with two phase markers:

### Phase A — Additive

Every Phase A step introduces new types (value objects, domain services) without removing
anything. The workspace compiles between every step. Tests can land alongside each type.

In resin-recipe-model, Phase A delivered: ADR-0005 draft, `FloatRange` + `IntRange` value
objects, `Recipe` value object, `PairingValidator` domain service. Each landing was
independently reviewable and left the repo in a releasable state.

### Phase B — Switchover

Phase B contains **one or more atomic commits**. Each atomic commit bundles:

- The field removal (from the old aggregate)
- The field addition (to the new aggregate)
- Every caller / test / data migration that the type system forces together

In resin-recipe-model, Phase B had two atomic landings:

- **Landing #1** (plan steps 5+6+7+8): `ResinProfile.recipe` required field +
  `SimulationRunner` pairing-before-slice + `FailurePredictor` signature change +
  `PrinterProfile` field removal / range addition + 3 printer TOMLs + 4 resin TOMLs +
  CLI updates. Single ~15-file commit.
- **Landing #2** (plan steps 9-13): `SlicedFileInfo` nested Recipe + migration
  completeness test + ADR finalize + UATs + regression sweep. Several commits but no
  type-forced atomicity.

## When to apply

- Refactors that move a field between aggregates
- Refactors that change a widely-used function signature (e.g. adding a parameter)
- Refactors that rename a type read by many callers

Not needed for: purely additive features, single-file changes, refactors where the old
API can live alongside the new with a deprecation period.

## Counter-indication: over-bundling

Phase B atomic commits can become huge (15+ files). If splitting is possible via a
transitional shim (e.g. an adapter that presents the old signature temporarily), prefer
the shim and two commits. The atomic commit is for cases where the shim costs more than
the atomicity.

## How to signal in a plan

Label each plan step with its phase. Flag Phase B atomic bundling in
`potentialChallenges`. Plan reviewers should check: "does this step force an atomic
commit with the next step? If yes, label both as Phase B landing #N."

## See also

- `docs/adr/0005-three-axis-printer-resin-recipe.md` — the refactor that introduced this pattern
- The `resin-recipe-model` swamp model's plan v1→v2→v3 iteration history for the HIGH-severity
  TDD-coupling finding that became this labelling convention
- `docs/patterns/entity-validate-on-mutation.md` — the pattern Phase A typically preserves
