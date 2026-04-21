---
issue: phase1-verification-audit
date: 2026-04-22
kind: anti-pattern
---

# Anti-pattern: single-agent audit without self-challenge

## The anti-pattern

A single agent produces an audit (or refactor, or implementation) AND runs
the matrix reviewers on its own output, without any compensating control.
The reviewers share context with the producer; findings contradicting the
producer's initial impression do not surface.

## Symptom

Clean reviews with zero findings on substantive work — especially when the
audit is broad (many items × many dimensions). Reviewer output reads as
endorsement rather than challenge.

## Why it happens

Matrix fan-out (code + adversarial + security + ux + skill) *looks* like
structured independence but is not. All reviewers are functions of the same
agent's context. The agent that wrote the plan has already reasoned through
the objections — re-running that reasoning under a "reviewer" label
reproduces the same conclusions.

## The fix

Use the **rubric-driven audit with self-challenge** pattern
(`docs/patterns/rubric-driven-audit-with-self-challenge.md`):

- Commit to a rubric before examining evidence.
- Force one counter-question per PASS verdict.
- Resolve or downgrade; retain both as an audit trail.
- Record §Audit Scope Limits explicitly.

## History

Raised as a HIGH finding in `phase1-verification-audit` plan-review round 1
(`swamp data get phase1-verification-audit current --json`,
`reviewHistory[0].reviews[1].findings[1]`). Resolved by plan v2 step 18.
