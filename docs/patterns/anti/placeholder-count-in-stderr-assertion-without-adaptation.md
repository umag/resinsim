---
issue: viz-ctb-fixture-synthesis
date: 2026-08-16
---

# Anti-pattern: Placeholder count in stderr assertion without adaptation

## Context

A spec's Then assertion checks for a literal substring in stderr that
contains fixture-dependent numbers:

```gherkin
Then stderr contains "layer count mismatch: CTB has 100 layers, sim has 50"
```

The step def substitutes a real fixture whose counts differ (e.g. 4492
and 10). The generic shared step (`then_stderr_contains`) extracts the
literal needle and checks it — assertion fails because "100 layers"
does not appear in stderr.

## Why this is an anti-pattern

The failure is SILENT at plan time — the spec prose looks steppable,
and the cucumber regex matches fine. The assertion only fails at RUNTIME
when the real fixture's output doesn't match the placeholder text.

## Resolution

The viz harness uses `expected_mismatch_counts` in VizWorld: the When
step stores the real (ctb, sim) layer counts, and `then_stderr_contains`
replaces the spec's placeholder text with the real counts before
asserting. This is a DOCUMENTED COUPLING — the replacement targets exact
placeholder text from one spec ("CTB has 100 layers", "sim has 50").

Better alternatives (not available here):
1. Use spec prose that doesn't contain fixture-dependent numbers
2. Use a dedicated Then step that doesn't overlap with the generic regex
3. Synthesise a fixture with the exact counts the spec uses

## See also

- `crates/resinsim-viz/tests/uat_viz_steps/viz_screenshot_flag.rs` —
  the doc comment on `then_stderr_contains` documents the coupling
