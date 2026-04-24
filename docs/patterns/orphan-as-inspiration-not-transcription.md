---
issue: print-time-on-reportgenerator
date: 2026-04-25
---

# Pattern: Treat an orphan branch as design inspiration, not transcription

## Context

When work was started but not landed, then main moved on, the prior
work often persists as an "orphan" commit or bookmark. Common scenarios:

- A feature branch was started before a prerequisite refactor landed.
  Post-refactor, the feature can no longer be mechanically rebased.
- A team member explored a design, abandoned it, but wanted to preserve
  the work for archaeological reference.
- A plan was authored against a previous codebase shape; the codebase
  shifted underneath.

The orphan still has value — it reflects real implementation thinking
about the problem domain, including tests, edge cases, and design
intent. The mistake is treating it as code-to-rebase.

## Signal

You've identified an orphan commit/bookmark that addresses the problem
you're working on. Before deciding to rebase:

1. **Run `jj diff -r <orphan> --stat`** (or git equivalent). Look at
   the file list. Are most of the touched files still shaped the same
   way on main, or have they been refactored, extracted, renamed?
2. **Read the orphan's commit message + the planning context.** The
   orphan's author was solving a problem in a particular codebase
   state. What does main look like NOW for the same problem?
3. **Check the issue scope.** Does the orphan's diff include
   out-of-scope material (refactors, deprecations, unrelated cleanup)?

If the answer to (1) is "files have moved", or (2) reveals a different
codebase shape, or (3) shows out-of-scope content, you're not rebasing
— you're porting with adaptation.

## Discipline

Lifecycle issues that port from an orphan should:

1. **Preserve the orphan as a bookmark** (don't `jj abandon` it before
   the port lands) so it's available for byte-level mining during
   implementation. Cleanup is the LAST step, post-merge.
2. **List the orphan's contributions in the plan** by category:
   - What ports verbatim?
   - What needs adaptation (and to what)?
   - What is out of scope and should be DROPPED?
3. **Surface adaptations explicitly** — a port without adaptation is
   suspicious unless the orphan was authored against the same
   codebase state.
4. **Rewrite docs that reference the orphan's design choices** — a
   pattern doc authored under a v3-style parameter-injection design
   reads incorrectly under v4 aggregate-owns-recipe-printer; fix it
   at port time, don't ship stale claims.
5. **Drop out-of-scope content with justification** — if the orphan
   includes a feature deprecation that the current lifecycle
   explicitly excludes, surface the omission in the PR description so
   reviewers see what was intentionally not ported.

## Cleanup

After the port lands and merges to main:

```sh
jj abandon <orphan-change-id>
jj bookmark delete <orphan-bookmark>
```

If `git remote` has the bookmark pushed, also delete remotely. The
orphan has served its purpose; keeping it around invites a future
agent to re-port it.

## Concrete example

The `print-time-on-reportgenerator` lifecycle (2026-04-24/25) ported
the orphan `jj:rnvrvtprzxmt` (bookmark `print-time-report-orphan`).

- **Verbatim port**: `format_duration_hms` helper + 11 unit tests.
- **Adapted port**: `summary()` signature reverted from v3
  parameter-injection (`&Recipe, &PrinterProfile`) to v4 arg-less; 9
  `PrintSimulation::new()` call sites adapted to take
  `(recipe, printer)`.
- **Out-of-scope drops**: UAT-4 and UAT-4b (`inspect thermal` requires
  --printer/--resin) — the legacy thermal path removal was explicitly
  excluded from the lifecycle's scope.
- **Doc rewrite**: `docs/patterns/print-time-projection-on-simsummary.md`
  was framed around parameter-injection in the orphan; rewritten for
  the v4 aggregate-owns-recipe-printer design.

The naive alternative (`jj rebase rnvrvtprzxmt onto main`) would have
produced 461 lines of textual conflict against the post-extraction
`ReportGenerator` and shipped UAT-4/4b out of scope.

## Related

- [replacement-without-branch-check](anti/replacement-without-branch-check.md)
  — adversarial counterpart on the planning side.
- ADR-0008 — UAT/spike conventions.
