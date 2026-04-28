---
issue: 15-sim-json-canonical-interchange
date: 2026-04-28
---

# UAT: `resinsim sim` produces a canonical `sim.json` envelope

## Rationale

ADR-0015 establishes `sim.json` as the canonical interchange format
between simulation producer and downstream consumers (resinsim-viz
`--load-sim`, `resinsim report health --in`, future LLM tooling). The
producer side of that contract is the new `resinsim sim` subcommand. The
acceptance shape: a `{schema_version, simulation, provenance}` envelope
written atomically (tmp + rename), schema_version = 1, provenance carrying
the run-context metadata so downstream consumers can reconstruct the
report header without re-supplying CLI args.

## UAT-1: Happy path — `sim --stl ... --out` produces a valid envelope

```gherkin
Scenario: UAT-1 sim subcommand writes a valid envelope to --out
  Given a sliced or STL input file
  And shipped resin and printer profiles
  When the user invokes `resinsim sim --stl <PATH> --resin generic_standard --printer generic_msla_4k --out cube.sim.json`
  Then the process exits with code 0
  And cube.sim.json exists at the requested path
  And the file is valid JSON with top-level fields schema_version, simulation, provenance
  And schema_version equals 1
  And provenance.input_path equals the input path
  And provenance.resin_name equals "Generic Standard"
  And provenance.printer_name equals "Generic MSLA 4K"
  And the simulation block has non-empty layers array
```

## UAT-2: Default `--out` derived from input stem

```gherkin
Scenario: UAT-2 omitting --out defaults to <input-stem>.sim.json
  Given a sliced input at /tmp/work/widget.stl
  When the user invokes `resinsim sim --stl /tmp/work/widget.stl ...` without --out
  Then the process produces /tmp/work/widget.sim.json
```

## UAT-3: Existing `--out` overwritten silently (POSIX default)

**Rationale.** ADR-0015's overwrite rule: silent overwrite (POSIX default).
No `--force` flag in v1; users that want no-overwrite manage the file
lifecycle themselves.

```gherkin
Scenario: UAT-3 existing target file is silently overwritten
  Given an existing file at /tmp/cube.sim.json with arbitrary content
  When the user invokes `resinsim sim --stl <PATH> ... --out /tmp/cube.sim.json`
  Then the process exits with code 0
  And /tmp/cube.sim.json contains the freshly produced envelope (the old content is gone)
```

## UAT-4: Missing input file hard-errors with non-zero exit

```gherkin
Scenario: UAT-4 missing --stl path hard-errors
  When the user invokes `resinsim sim --stl /no/such/file.stl --out /tmp/x.sim.json`
  Then the process exits with non-zero code
  And stderr mentions "/no/such/file.stl"
```

## UAT-5: Missing profile name hard-errors with available list

**Rationale.** The unknown-profile guard surfaces in the SIM (producer)
side after ADR-0015 — `report health` is the consumer and no longer
touches profiles. The available-list error shape (per ADR-0004) is
preserved.

```gherkin
Scenario: UAT-5 unknown --resin name hard-errors with available list
  When the user invokes `resinsim sim --stl <PATH> --resin no_such_resin --printer generic_msla_4k --out /tmp/x.sim.json`
  Then the process exits with non-zero code
  And stderr contains "no_such_resin"
  And stderr contains "Available profiles"
```

## UAT-6: Atomic write — partial failures don't corrupt existing target

**Rationale.** ADR-0015's atomic-write contract: `save_to_path` stages at
`<out>.tmp` then `std::fs::rename`s. A write failure mid-flight cannot
truncate an existing `<out>` from a downstream consumer's perspective.

```gherkin
Scenario: UAT-6 unwritable parent path leaves existing files untouched
  Given an unrelated file at /tmp/safe.sim.json with content "previous"
  And a path /tmp/blocked/inner.sim.json whose parent /tmp/blocked is not a directory
  When the user invokes `resinsim sim --stl <PATH> ... --out /tmp/blocked/inner.sim.json`
  Then the process exits with non-zero code
  And /tmp/safe.sim.json still contains "previous"
```
