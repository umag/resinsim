---
issue: uat-unskip-campaign
date: 2026-08-01
---

# Anti-pattern: Spec text edited, step regex not — nothing fails

## The failure shape

A refactor renames a user-facing surface (ADR-0015's CLI rename). The UAT
spec .md is dutifully updated to the new wording. The cucumber step regex
in the module is not. Cucumber treats the reworded step as UNDEFINED and
skips the scenario — no failure, no warning that survives CI. Four
`cli-temperature-flag-validation` scenarios and one
`suction-detector-raft-false-positive` scenario sat dead this way for
months while both files looked individually healthy.

## Why it survives

- Skips are not failures, and an aggregate skip count is dominated by the
  legitimate debt register — 5 extra skips hid inside 117.
- The static "spec has a module" guard sees a stepped spec and asks
  nothing further.
- Regex-vs-prose is unlinked text: no compiler, no grep, no rename tool
  crosses that boundary.

## Detection and repair

- Detection: per-spec runtime skip attribution
  (`docs/patterns/per-spec-runtime-skip-attribution.md`) — a stepped,
  off-register spec with a nonzero skip count fails loudly.
- Repair rule: prefer moving the REGEX to the current spec text; edit the
  spec only where its prose is factually stale.
- **Drift repair can uncover production defects**: the fifth scenario's
  assertion ("the warning surfaces in resinsim sim") describes behavior
  the binary never implemented. The no-weaken rule applies — file the
  production issue (`kb153-warning-missing-from-resinsim-sim`), register
  the scenario as declared debt naming it, and never re-point the
  assertion at what happens to pass.

## See also

- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md`
- `docs/patterns/anti/cucumber-step-regex-ambiguity.md` — the sibling
  regex hazard (collision rather than drift)
