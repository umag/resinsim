# Iteration limits for resinsim lifecycles

resinsim does **not** override the skill's autonomous-loop defaults. This
file exists to make that explicit and to record what happens at the cap,
so no lifecycle has to re-derive it from the issue-lifecycle skill's
`references/autonomous-loop.md`.

## The caps

| Phase | Env var | Default | Hydrate counter |
| --- | --- | --- | --- |
| 3, plan review | `MAX_PLAN_ITERATIONS` | 5 | `hydrate.planIterationsThisVersion` |
| 5, code review | `MAX_CODE_ITERATIONS` | 5 | `hydrate.codeReviewIteration` |

Caps are skill-enforced against these hydrate counters — the model reads
no env vars itself. Phase 3's counter is **per plan version**
(`planIterationsThisVersion`, and the review-history table's `Version`
column), so a human `reject_plan` that produces a new plan version
restarts that phase's budget. This is inferred from the counter's name
and the history table's shape — no prose in `autonomous-loop.md` states
the reset outright, so treat it as "the counter is scoped per version,"
not as a guaranteed reset behaviour, until you've checked it against a
live `hydrate` call.

Only two phases run the autonomous loop today (Phase 3 and Phase 5) — the
skill's `references/autonomous-loop.md` phase-mapping table has exactly
these two columns.

## What "clean" means

`blocking.total == 0 AND coverage.complete` — zero CRITICAL and zero HIGH
open findings, and every active review-matrix reviewer has recorded a
result for the current round. MEDIUM and LOW findings never block and are
never a reason to burn an iteration; they're carried into the presented
history as noted warnings. Both Phase 3 and Phase 5 present the clean
result to the human and **wait** for an explicit trigger phrase (`approve`,
`approved`, `looks good`, `ship it`, `go`, `LGTM`) before calling
`approve_plan` or `resolve_findings` — a clean exit ends the *autonomous*
loop, it does not skip the approval gate.

## At the cap

The loop exits to handover — it never accepts on your behalf. Present the
blocking findings grouped by reviewer, plus the full iteration history,
and ask for **direction**, not approval. The four documented options:

1. Reduce scope — accept a narrower fix and re-plan
2. Different approach — pivot the strategy
3. Accept remaining findings — mark specific findings accepted/wontfix
4. Manual intervention — take over the plan or code directly

Never call the acceptance method in handover mode, even if the human says
"looks fine" — the safeguard fired for a reason. There is no
input-parameter escape hatch that bypasses re-review: after handover, the
human either gives direction that resolves the safeguard (e.g. marks
findings wontfix) so the loop restarts from the top, or says "force
approve" — which still does not auto-call anything; it re-fans-out
reviewers, confirms matrix coverage is complete, and then waits for the
normal trigger phrase.

## Two safeguards that usually fire first

- **Loop detection.** `hydrate.signature` (a stable hash over open
  CRIT/HIGH findings) identical to the previous round's signature means
  the current approach cannot address the finding — handover immediately
  rather than spending the remaining iterations on repeats.
- **Pivot required.** Any open CRITICAL/HIGH finding with
  `category: pivot-required`, or a description starting `FUNDAMENTAL:`,
  exits to handover immediately regardless of the iteration count.

## resinsim-specific notes

- Run `tessl__review-*` skills inline, never as subagents — a subagent has
  no context to answer the permission prompts a review can trigger, and
  the review blocks.
- Never chain through an approval gate. Pause before every human-gated
  transition, even mid-autonomous-loop.
- Budget the wall-clock, not the iteration count: one Phase 5 iteration on
  a code change costs the full ADR-0017 four-config matrix plus both
  `cargo uat` configs (see `implementation-conventions.md`). Five
  iterations is a long afternoon on this machine — an argument for a
  tighter first plan, not for asking to raise the cap.

## See also

- `agent-constraints/implementation-conventions.md` — build/verify
  commands the code-review loop actually runs each iteration
- issue-lifecycle skill, `references/autonomous-loop.md` — full loop
  control flow, hand-off formats, loop-bug recovery (a skill reference
  file, not a path inside this repo)
