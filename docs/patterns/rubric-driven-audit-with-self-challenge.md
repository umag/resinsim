---
issue: phase1-verification-audit
date: 2026-04-22
kind: pattern
---

# Rubric-driven audit with self-challenge

## Context

Architecture / code quality audits are inherently subjective unless the
auditor commits to a verdict rubric *before* examining evidence. When the
same agent produces both the audit and runs the matrix reviewers on it,
confirmation bias is structural: the same context that produced a PASS
verdict will usually endorse it under review.

## Pattern

1. **Rubric first.** Before auditing any item, write the rubric as part of
   the plan. For each dimension, define what PASS / PARTIAL / GAP requires.
   Include an explicit N/A policy for dimensions that do not apply to all
   item types (e.g. physics correctness is N/A for orchestration services).
2. **Audit items against the rubric, one dimension at a time.** Record
   verdicts as a table. Evidence links must be concrete (file:line).
3. **Self-challenge.** For every PASS verdict, formulate one counter-question
   ("what would make this PASS wrong?"). Resolve with evidence logged in
   the audit doc (verdict stays PASS) OR downgrade to PARTIAL with the open
   question logged under §Open Questions. Resolved questions are retained
   with their resolution for audit trail.
4. **§Audit Scope Limits.** Explicitly list cells where evidence was thin
   ("rubber-stamp risk: these N cells relied on a single grep, not
   line-by-line read"). This tells the consumer where the audit ran light.
5. **Verdict tally with arithmetic check.** The executive summary tally
   must be recounted from first principles against §Findings, with column
   sums checked against row count. An inflated PASS count is a HIGH
   finding in the code-review phase.

## Applicability

- Any verification / compliance / quality audit produced by a single agent.
- Plan-vs-code reconciliation audits like `phase1-verification-audit`.
- Post-refactor audits where the same agent did both refactor and audit.

## Not applicable

- Audits with an independent second-agent review (the independent voice
  substitutes for self-challenge).
- Bug triage where the goal is reproduction, not structured judgment.

## Reference implementation

- Plan v3 of `phase1-verification-audit` (`swamp data get
  phase1-verification-audit current --json`, `plan.steps[2]` = rubric
  step, `plan.steps[18]` = self-challenge, `plan.steps[19]` = scope-limit
  synthesis).
- `projects/000-global/research/resinsim-verification-findings.md` §Rubric,
  §Findings, §Open Questions, §Audit Scope Limits.

## History

- v1 plan had subjective PASS/PARTIAL/GAP; rejected by adversarial review.
- v2 added rubric and self-challenge; still had test-baseline contradiction.
- v3 fixed baseline + rubric N/A + tightened self-challenge wording.
- Code-review round 1 caught miscounted verdict tally (HIGH) despite all
  plan safeguards — underscores the "arithmetic check" step.
