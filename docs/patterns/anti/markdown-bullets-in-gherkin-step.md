---
date: 2026-04-23
issue: uat-gherkin-runner-rollout
---

# Anti-pattern: markdown bullet lists inside Gherkin step text

## The trap

Authoring a Given/When/Then step that "continues" onto multiple
bullet items — natural in prose — isn't valid Gherkin:

```gherkin
Scenario: X
  Given a LayerInput stack comprising:
    - a solid raft for layers 0-22
    - a discrete-column layer at layer 23
    - a solid body above
  When …
```

The gherkin parser sees `- a solid raft` as an unknown step
keyword and fails to parse the scenario. cucumber reports
`1 parsing error` and skips that scenario. Because skipped ≠ failed,
silent-green harness guards can let this slip through unnoticed.

## The rule

Compound step inputs in Gherkin are expressed as EITHER of:

**DocString** — triple-double-quote block for free-form text:

```gherkin
Given a stack comprising:
  """
  solid raft for layers 0-22
  discrete-column layer at layer 23
  solid body above
  """
When …
```

Step def access: `step.docstring.as_deref()`.

**DataTable** — pipe-delimited rows for structured key/value data:

```gherkin
Given a printer with ranges:
  | range                 | min   | max   |
  | layer_height_range_um | 100.0 | 150.0 |
  | exposure_range_sec    | 10.0  | 60.0  |
```

Step def access: `step.table.as_ref().map(|t| &t.rows)`.

Bullet-list markdown has no Gherkin equivalent.

## Detection

A cucumber harness should assert `writer.skipped_steps() == 0` after
every run (coverage guard (a)) — otherwise a parse-error scenario
looks identical to a passing one at the exit-code level.

## Where this was learned

`spec/uat/suction-detector-raft-false-positive.md` originally
authored UAT-1 / UAT-2 Givens with markdown bullet continuations.
Step 4's harness refactor surfaced the parse error at
`$CARGO_TARGET_TMPDIR/spec-uat-features/suction-detector-raft-false-positive.feature`
failing to parse; migration to DocString landed inside the step 4
commit.

## Related

- `docs/patterns/extracting-gherkin-from-markdown.md` — the source
  conventions.
- `docs/adr/0008-bdd-uat-spike-notes.md` — the rollout where this
  was learned.
