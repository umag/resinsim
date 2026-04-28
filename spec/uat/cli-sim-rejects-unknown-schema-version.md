---
issue: 15-sim-json-canonical-interchange
date: 2026-04-28
---

# UAT: `report health --in` rejects unknown `schema_version` cleanly

## Rationale

ADR-0015's schema_version discriminant lets old loaders refuse future
envelopes rather than parsing them as if they were the current shape. The
load-bearing acceptance: a v1 loader sees `schema_version: 999` and
returns a typed error mentioning the rejected version. The user sees a
non-zero exit and an actionable message, NOT a panic, NOT a confusing
parse failure deep inside serde.

## UAT-1: `report health --in` against schema_version=999 rejects with typed error

```gherkin
Scenario: UAT-1 unknown schema_version triggers typed rejection
  Given a sim.json envelope where schema_version has been tampered to 999
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with non-zero code
  And stderr mentions "unknown schema_version"
  And stderr mentions "999"
  And the process does not panic (no "thread 'main' panicked" in stderr)
```

## UAT-2: `report health --in` against missing file mentions the path

```gherkin
Scenario: UAT-2 missing input file surfaces the failing path
  When the user invokes `resinsim report health --in /no/such/file.sim.json`
  Then the process exits with non-zero code
  And stderr mentions "/no/such/file.sim.json"
  And stderr contains "failed to read"
```

## UAT-3: `report health --in` against malformed JSON surfaces parse error

```gherkin
Scenario: UAT-3 garbage JSON triggers parse error not panic
  Given a file at /tmp/garbage.sim.json containing the bytes "this is not json"
  When the user invokes `resinsim report health --in /tmp/garbage.sim.json`
  Then the process exits with non-zero code
  And stderr contains "failed to parse"
  And the process does not panic
```

## UAT-4: Tampered aggregate (envelope intact, validate fails) surfaces typed error

**Rationale.** Per ADR-0009 the deserialise-bypass guard re-runs
`PrintSimulation::validate()` after parsing so a child-entity violation
(e.g. negative `layer_height_um`) cannot silently land in a downstream
consumer. ADR-0015 inherits this guard.

```gherkin
Scenario: UAT-4 tampered child entity surfaces "invalid simulation"
  Given a sim.json with valid schema_version=1 but recipe.layer_height_um set to -1.0
  When the user invokes `resinsim report health --in <PATH>`
  Then the process exits with non-zero code
  And stderr contains "invalid simulation"
  And stderr contains "layer_height_um"
```
