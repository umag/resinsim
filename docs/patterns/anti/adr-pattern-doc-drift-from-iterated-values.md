---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# Anti-pattern: ADR / pattern-doc drift when values are iterated interactively after planning

## Tempting

A plan locks in initial values for a constant (e.g. default camera
angles), an ADR records the decision, and a pattern doc explains the
math. Code-review later notices the user changed the values via
interactive iteration but the supporting docs were never updated.

## Why it's wrong

ADRs are meant to be authoritative. A future contributor grepping for
the camera default angle finds `yaw=20°, pitch=45°` in
`docs/adr/0011-…md` but `yaw=45°, pitch=-120°` in
`crates/resinsim-viz/src/main.rs three_quarter_yaw_pitch()`. The
contradiction wastes time AND erodes trust in the ADR — the contributor
now has to verify every other ADR claim against the code.

## Pattern: update ADRs at every plan revision

When the lifecycle's autonomous-iterate loop bumps `planVersion`, walk
the plan diff and apply the same edits to:

1. The ADR file(s) listed in `state.plan.summary` `## Documentation impact`.
2. Pattern notes that quote specific values.
3. The commit message body if values change after the initial commit.

Issue 10's lifecycle caught the drift in code-review (round 1, MED) but
ideally it would have been caught at plan revision time. A
`reviewMatrix.documentation: true` reviewer that diffs ADR text against
plan-summary `## Documentation impact` items is a candidate enhancement
to the issue-lifecycle skill itself.

## Mitigation

Concrete, doable today: when the autonomous-iterate loop calls
`reject_plan` + `plan` for a new revision, the human iterating the plan
should also walk every documentation impact item and update it
in-place, not as a follow-up commit. The cost of doing this at plan
revision time is ~5 minutes; the cost of catching it in code review is
a full review round + iterate cycle.
