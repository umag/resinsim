---
issue: uat-gherkin-coverage-guard-panic
date: 2026-07-27
---

# Anti-pattern: a guard that cannot observe its own failure mode

## What goes wrong

A test harness asserts on the signal it *can* reach rather than the one that
matters. The assertion looks strict — often it is `== 0` — so it reads as
airtight. But the failure it is meant to prevent arrives through a channel the
assertion never inspects, and the guard reports success while the thing it
guards is broken.

This is worse than having no guard. No guard invites suspicion. A confident
green guard actively suppresses it.

## Concrete example (uat_gherkin, 2026-04 → 2026-07)

`crates/resinsim-core/tests/uat_gherkin.rs` asserted:

```rust
assert_eq!(writer.skipped_steps(), 0, "coverage guard (a) failed: ...");
```

Reasonable on its face: cucumber reports an undefined step as *skipped*, not
*failed*, so without this a missing step definition would slip past
`execution_has_failed`.

But a spec file whose Gherkin is **malformed** does not produce skipped steps.
It produces nothing at all. Cucumber counts it in a separate
`parsing_errors()` tally, prints `20 parsing errors` in a summary block, and
drops every scenario in the file. `skipped_steps()` cannot see them, because
they never became steps.

Measured on main at 2026-07-27:

```
150 scenarios authored
 -54 lost to 20 parse errors   <- guard structurally blind to these
 -63 skipped (no step defs)    <- guard asserted on these, so suite was red
 = 36 actually executing (24%)
```

The suite had been red for months on the *visible* half, which meant nobody
was looking at the output closely enough to notice the invisible half. The
guard's own noise hid the gap in the guard.

## Why it persisted

Three reinforcing factors, all worth checking for elsewhere:

1. **The blind channel was a counter, not a failure.** `20 parsing errors` is
   a line of summary text. Nothing exits non-zero on it.
2. **The visible half was permanently red**, so "the suite fails" carried no
   information. A guard that always fails is equivalent to no guard, and it
   trains readers to skip the output.
3. **The suite was excluded from the mandated gate.** `.config/nextest.toml`
   excludes `uat_*` binaries because cucumber's `harness = false` aborts
   nextest enumeration, so a green four-config matrix said nothing about it.

## The rule

**Ask what the guard is blind to, and assert on that channel too.**

Concretely, when writing a harness assertion:

- Enumerate every way the thing you care about can fail to happen. Not fail —
  *fail to happen*. Absence and error are different channels.
- For each, name the accessor that observes it. If there is no accessor, the
  guard cannot cover it and you must say so in the comment.
- Prefer a guard that reports a *shrinking debt* over one that is binary. A
  `== 0` assertion on a condition nobody intends to reach today is a guard
  that will be red forever, and a permanently red guard is unread.

The repair here added the missing channel and split the intent:

```rust
// runtime backstop: the channel the old guard could not see
assert_eq!(writer.parsing_errors(), 0, "...");

// debt register that must only shrink, rather than == 0 forever
assert_unstepped_specs_match_allowlist(&spec_uat);
```

plus an authoring-time check in a **nextest-visible** target
(`tests/spec_gherkin_wellformed.rs`) so the mandated matrix catches malformed
Gherkin before it reaches the harness at all.

## Related trap: the exemption list that only grows

An allowlist replacing a `== 0` assertion has its own failure mode — it
becomes a place to hide work. Make it fail in **both** directions:

- an item that is not on the list and should be → fail (new debt smuggled in)
- an item on the list that no longer needs to be → fail (stale excuse)

`SPECS_WITHOUT_STEP_DEFS` and `SPECS_WITH_NO_EXECUTABLE_SCENARIOS` both do
this. Without the second assertion the register rots into decoration.

## See also

- `docs/patterns/anti/markdown-bullets-in-gherkin-step.md` — the authoring
  mistake that fed this; it went unenforced for three months and recurred 20
  times, which is why the fix had to be executable rather than documentary.
- `docs/patterns/cucumber-in-nextest-workspace.md` — why the suite is outside
  the nextest gate, i.e. factor 3 above.
- `docs/patterns/anti/cfg-test-silent-typos.md` — sibling shape: test code
  that never compiles under the configuration you run.
- `docs/patterns/anti/fixture-copy-of-shared-builder.md` — sibling shape: a
  fixture that rots because nothing links it to its source of truth.
