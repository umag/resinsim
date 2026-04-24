---
issue: print-time-on-reportgenerator
date: 2026-04-24
---

# Anti-pattern: Proposing a replacement without checking which branch the target lives in

## Context

A planner notices a suspicious formula somewhere in a multi-branch function
and proposes replacing it with a "correct" primitive. The proposed fix
reads plausibly in isolation: "replace naive X with the real primitive Y."

The trap: the suspicious formula only runs inside a branch where the inputs
needed for the "real primitive" aren't available. The replacement is
uncallable from that code path. A reviewer who only reads the proposal
misses this; a reviewer who opens the file and reads the surrounding
context catches it immediately.

## Concrete example (from the orphan `print-time-report` lineage)

The predecessor work to this lifecycle (jj:rnvrvtprzxmt, preserved under
bookmark `print-time-report-orphan`) had a plan v1 step saying:

> Fix naive `l * (exposure + lift_cycle)` time-axis in `cmd_inspect_thermal`
> (main.rs:910, 952) — replace with `LayerTimingCalculator::
> cumulative_times_sec` indexing.

The formula was real; `LayerTimingCalculator::cumulative_times_sec` was
real. Both facts were correctly cited. The gap: lines 910 and 952 lived
inside `cmd_thermal`'s **legacy single-stage branch**, which ran *only
when no `--printer` profile was supplied*. In that code path neither
Recipe nor PrinterProfile was loaded, and `cumulative_times_sec(recipe,
printer, n)` requires both. The proposed replacement was uncallable from
the line it claimed to replace.

Round-1 adversarial review caught this as HIGH/correctness. The orphan's
v2 resolved it by proposing the harder path: delete the legacy branch
entirely and make `--printer` + `--resin` required — removing the naive
formula by removing its only call site. **Note** that the orphan itself
never landed on main; the legacy branch deletion remains an open concern
(out of scope for the `print-time-on-reportgenerator` lifecycle which
focuses narrowly on the `SimSummary` projection + `ReportGenerator`
rendering).

## Signal

You're proposing a replacement of a specific line or formula. Before
recording the proposal:

1. Open the file and read the enclosing function.
2. Identify every branch that leads to the target line. `if let`, `match`,
   early-return / continue, feature-gates — each can restrict the ambient
   state the target expects to find.
3. For each ambient binding the replacement requires, confirm it exists in
   every branch that can reach the target. Any branch where the binding
   is `None` or out of scope is a blocker.

If the replacement is uncallable from some reachable branch, the plan is
incomplete. Either:

- Restructure the caller so the required inputs are always in scope
  (possibly by deleting the offending branch, as the orphan's v2 proposed).
- Synthesise the missing inputs (usually bad — hides the invariant the
  target was supposed to assert).
- Narrow the replacement's scope so it only applies in branches where its
  inputs exist, and deal with the rest separately.

All three are valid; picking blindly is not.

## Related

- `docs/patterns/rubric-driven-audit-with-self-challenge.md` — the general
  form of this discipline: every plan claim is a falsifiable statement
  that a reviewer should try to falsify before accepting.
- ADR-0007 — the physics reason `LayerTimingCalculator` needs both Recipe
  and PrinterProfile (release mechanism branch).
