---
issue: agent-constraints-dangling-see-also
date: 2026-08-04
---

# Anti-pattern: Doc audit scoped to one authority

## What goes wrong

A documentation accuracy audit adopts a falsification rule — "any claim
whose check fails is deleted" — but greps only ONE normative source. A
claim that is false against that source and true against another gets
deleted as confidently as a genuine error, and the deletion ships wearing
the audit's own credibility.

Concrete instance (2026-08-04): while writing
`agent-constraints/iteration-limits.md`, the audit grepped the
issue-lifecycle skill's `references/` directory for `tests_approved`,
`override_reason` and `testReviewIteration`, found zero hits, and deleted
the test-review-round paragraph — concluding "there is no input-parameter
escape hatch." The `@magistr/issue-lifecycle` MODEL schema
(`swamp model type describe`) defines all three: the
`writing_tests`/`reviewing_tests` round, the `hydrate.testReviewIteration`
counter, and `tests_approved`'s documented `override_reason` human
override. The skill's reference files and the model's method schemas are
BOTH normative, and they lag each other.

## Why it survives

The falsification rule feels rigorous — it is the cure for the opposite
failure (asserting unverified claims), so applying it aggressively reads
as discipline. Deletions also leave no artifact to review: a reviewer sees
accurate remaining text, not the missing paragraph. And the narrower the
grep scope, the cleaner the "zero hits" evidence looks.

## Detection and repair

- Before auditing, enumerate the normative sources the claims span — for
  lifecycle-process docs in this repo that is at minimum the
  issue-lifecycle skill's `references/` directory AND the model schema
  (`swamp model type describe @magistr/issue-lifecycle`). A claim is
  killable only when it fails against EVERY source that could define it.
- Red flag at review time: a deleted claim that names a mechanism the
  current lifecycle is itself using (this one was caught because the
  reviewing lifecycle was inside a `reviewing_tests` round at that very
  moment).
- Repair shape: rescope rather than restore — state which authority
  defines what, and name both so the next audit checks both.

## See also

- `agent-constraints/iteration-limits.md` — the corrected file, which now
  names both authorities
- `docs/patterns/anti/guard-that-cannot-observe-its-own-failure-mode.md` —
  the same blindness one level down: a check whose scope excludes its own
  failure mode
- `crates/resinsim-core/tests/agent_constraints_links.rs` — the guard
  that names this doc, per this repo's own rule that an anti-pattern doc
  must name the guard that catches it
