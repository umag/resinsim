---
issue: t1f6
date: 2026-04-18
---

# UAT: Legacy resin TOML loads with thermal defaults and enforces ordering

**Scope note (2026-04-21, ADR-0005).** "Legacy" in this UAT means *KB-150-era*: resins
written before the `degradation_temp_c` / `min_safe_temp_c` fields were added. Such TOMLs
must still include the `[recipe]` table introduced in ADR-0005 to deserialise — the
"missing thermal thresholds" allowance is orthogonal to the "missing recipe" refusal.
For the ADR-0005 migration contract (pre-refactor TOMLs without `[recipe]` fail loudly),
see [legacy-resin-toml-without-recipe.md](legacy-resin-toml-without-recipe.md).

## UAT-1: Legacy TOML defaulting

**Rationale.** T1-F6 identified that `degradation_temp_c` and
`min_safe_temp_c` — added post-hoc via `#[serde(default)]` in KB-150 —
had no regression test. A legacy TOML missing either field silently
deserialised with the module defaults (50.0 / 15.0), and no coverage
anchored that contract or the failure mode where an explicit legacy
value combined with a serde-applied default violates the ordering
invariant. This is user-facing behaviour: loading an old resin profile
must produce either a valid profile or a clear error, never a silently
misaligned one.

```gherkin
Scenario: UAT-1 legacy TOML defaulting — missing thermal fields apply documented defaults
  Given a resin TOML file written before KB-150 that is missing both "degradation_temp_c" and "min_safe_temp_c"
  When the resin profile is deserialised and validated
  Then "degradation_temp_c" takes the documented default of 50.0 °C
  And "min_safe_temp_c" takes the documented default of 15.0 °C
  And validate() returns Ok because the defaulted pair satisfies min_safe_temp_c < degradation_temp_c
```

## UAT-2: Invariant-crossing via serde default

**Rationale.** The highest-risk failure mode at the parse seam: a legacy
TOML with one explicit value and the other absent, where serde fills the
absent field with a default that — combined with the explicit value —
crosses the ordering invariant. Without coverage, a future change to
`default_degradation_temp_c()` could silently reject old files at load
time in production.

```gherkin
Scenario: UAT-2 invariant-crossing via serde default — explicit + defaulted fields cross ordering
  Given a resin TOML file with "min_safe_temp_c" explicitly set to 55.0 °C but with "degradation_temp_c" absent
  When the resin profile is deserialised and validated
  Then serde applies the default "degradation_temp_c" of 50.0 °C
  And validate() returns Err citing BOTH fields by name because 55.0 > 50.0 violates the strict-less-than ordering invariant
  And the profile is NOT silently accepted with misaligned thermal thresholds
```
