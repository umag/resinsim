# Implementation conventions for resinsim

These conventions extend the issue-lifecycle skill's Phase 4 (implementation)
defaults. The skill reads this file automatically when present; updates here
take effect on the next lifecycle.

## Linear-history rule (load-bearing)

resinsim keeps a **single linear history on `main`**. Every feature is a
small chain of commits stacked directly on the current tip of `main`; no
parallel feature branches accumulate.

This rule exists because issue-15 surfaced a real divergence cost: while
issue 15 was in progress on the viz-v2 redesign chain, an earlier
feature (`feat/05-layer-timeline-chart`) had been completed on a sibling
branch off `feat/12`. Two heads on the graph required a non-trivial
rebase + ADR renumbering + import-conflict resolution to merge back into
linear shape. The procedural fix below prevents that.

### Detecting divergence at session start

Before starting any new lifecycle, run:

```bash
jj log -r 'heads(all()) & ~description(glob:"*WIP*") & ~empty()' --limit 10
```

If more than one non-trunk head appears, **stop and rebase before starting
new work**. Multiple heads mean a previous lifecycle was completed without
advancing `main`, OR a parallel branch is in flight. Either way, fix the
shape first:

```bash
# Pick the canonical line and rebase the others onto its tip:
jj rebase -s <other-head> -d <canonical-tip>
# Then advance main:
jj bookmark set main -r <canonical-tip>
```

### Starting a new feature

The using-jj-workspaces skill says "jj new before jj workspace add for a
clean shared base." Tighten that: **the base must be `main`**, not the
current `@` (which may be a previous feature still in the working copy):

```bash
# WRONG — inherits the previous feature's @ as the new base:
jj workspace add ../resinsim-N-newfeature

# WRONG — uses jj new from current @, which may not be main:
jj new
jj workspace add ../resinsim-N-newfeature

# RIGHT — explicitly base on main:
jj new main
jj workspace add ../resinsim-N-newfeature
```

The `jj new main` ensures the new workspace's `@` descends directly from
`main`, regardless of where the default workspace's `@` was sitting. Even
if the previous lifecycle forgot to advance `main` (the bug below), this
catches the divergence at the new-feature boundary.

### Completing a feature

At Phase 5 → `resolved` (or after harvest → `complete`), **always advance
`main` before forgetting the workspace**:

```bash
# When the lifecycle is at `complete` (or `resolved` if skipping harvest):
jj bookmark set main -r @
# If main was on a divergent line and refuses, override:
jj bookmark set main -r @ --allow-backwards
```

If you forget, the next `jj new main` for the following feature will
branch from STALE main, recreating the divergence problem. The
issue-lifecycle skill's complete handler should run this.

### When divergence is unavoidable

Two cases legitimately need parallel heads, briefly:

1. **Hotfix on stable while a feature is in flight.** Branch the hotfix
   from `main` directly (`jj new main`), ship it, advance `main`, then
   rebase the in-flight feature onto the new `main`.
2. **Spike or experiment that may be abandoned.** Tag with
   `experimental:` prefix in the bookmark name and don't promote to
   `main` until the spike clears.

For everything else: linear stack, advance main on completion.

## Build + verification commands

- `cargo build --workspace` — fast sanity-check
- `cargo nextest run --workspace` — full test suite (pinned via memory: always `cargo nextest run`, never `cargo test`)
- `cargo fmt --all`
- `cargo clippy -p resinsim-core -p resinsim-inspect --all-targets -- -D warnings` — clippy clean on core + inspect (resinsim-viz has pre-existing warnings unrelated to issue 15; not blocking)

Tests must pass before `review_code`.

## PR convention

Per project memory: PRs target `dev`, not `main`. `main` is reserved for
stable releases. The linear-history rule above governs `main` topology;
`dev` topology is governed by your usual git/jj remote workflow.

## Acceptance gate hand-off

The issue-lifecycle skill is the source of truth for state transitions.
This file does not override the sacred rules:

- Never auto-call `approve_plan`, `resolve_findings`, or `complete`
  without an explicit human trigger phrase.
- Run `tessl__review-*` skills inline, not as subagents (per project
  memory `feedback_inline_reviews.md`).
- For resinsim/, jj commits stay inside the resinsim/ workspace tree;
  ora-root changes are curated by Mag (per `feedback_no_ora_commits.md`).

## See also

- `agent-constraints/uat-conventions.md` — UAT format + location
- `agent-constraints/knowledge-base.md` — KB layout (docs/patterns/, docs/patterns/anti/, docs/adr/)
- `agent-constraints/iteration-limits.md` — autonomous loop caps
- `using-jj-workspaces` skill — sibling-workspace mechanics
- ADR-0015 (this issue) — example of a clean linear-history feature commit
