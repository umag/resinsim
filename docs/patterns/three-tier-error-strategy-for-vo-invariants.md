---
issue: t1f3
date: 2026-07-28
---

# Pattern: Three-tier error strategy for value-object invariant enforcement

## Context

Value objects in resinsim-core (`Energy`, `PenetrationDepth`, `CureDepth`,
`CrossSectionArea`, `PeelForce`, `SafetyFactor`, `VatTemperature`, …) enforce
physical invariants (finite, non-negative, range) in `new() -> Result`.
T1-F3 found 168 construction sites across 15 files bypassing `new()` via
direct tuple construction, and constructor-adjacent methods
(`Energy::from_exposure`, `CrossSectionArea::circle`) doing raw math with no
validation. Blanket-converting every interior site to `Result` would smear
error plumbing through hot loops for conditions that cannot occur after load
validation.

## Pattern

Classify every construction site into one of three tiers:

**Tier 1 — untrusted boundary: return `Result`.** Constructor-adjacent
methods that derive a value from external or computed input validate like
`new()` and return `Result<Self, String>` (e.g. `Energy::from_exposure`,
`CrossSectionArea::circle` guarding `diameter >= 0 && is_finite`).

**Tier 2 — validated interior: `::new(..).expect(..)` with pointer.**
Service-internal constructions from already-validated profile data use
`::new(x).expect("already validated at load — see ResinProfile::validate")`.
The expect message names the guarantor so the reader can verify the contract
instead of trusting the comment.

**Tier 3 — entry-point enforcement makes Tier 2 sound.** Aggregate/profile
`validate()` runs at every simulation entry point
(`simulation_runner::run_*`), so Tier-2 `.expect()` calls cannot fire in
release builds. Without Tier 3, Tier 2 is a lie (see
`docs/patterns/anti/debug-assert-as-release-guard.md` for the failure mode).

The enabling mechanism is private/`pub(crate)` fields: bypassing the tiers
becomes a compile error, not a review-time catch (see
`docs/patterns/anti/tuple-construction-bypasses-validation.md`).

Keep arithmetic ON the value object (`Energy::scale(factor)` and other typed
helpers) so derived values stay inside the validated domain instead of
round-tripping through raw floats
(`docs/patterns/behaviour-on-value-object.md`).

## When to use

- A VO invariant audit finds bypass sites in service code
- Adding any new constructor-adjacent method (`from_*`, geometry helpers)
- Reviewing a diff that adds `VO(x)` tuple construction outside `new()`

## Testing

- Tier-1 methods: one rejection test per invariant branch
- Tier-3: an entry-point test proving invalid profiles are rejected before
  any Tier-2 site executes
- Compile-time: fields private so the bypass class cannot recur

## See also

- `docs/patterns/anti/tuple-construction-bypasses-validation.md` — the anti-pattern this resolves (t1f6)
- `docs/patterns/anti/split-constructor-invariant.md` — sibling constructor-path hazard (t1f2)
- `docs/patterns/nan-two-layer-defence.md` — value-layer guard pairing (t1f4)
- `docs/patterns/entity-validate-on-mutation.md` — entity-layer counterpart (t1f5)
