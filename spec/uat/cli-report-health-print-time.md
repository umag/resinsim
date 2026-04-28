---
issue: print-time-on-reportgenerator (refactored 2026-04-28 for ADR-0015)
date: 2026-04-24
---

# UAT: `report health` surfaces total print time (+ per-phase breakdown)

## Rationale

`resinsim report health` historically reported force, safety, thermal, and
deflection extrema but did not tell the user how long the print job would
take — the single most-asked-about number for a real operator. `LayerTimingCalculator`
already computes per-layer time with full dependency coverage (ADR-0007
release mechanism branch; ADR-0005 recipe-sourced exposure phases,
wait fields, lift_cycle_sec). The print-time-on-reportgenerator change surfaces that
computation as a first-class `SimSummary` projection (total_time_sec +
bottom/transition/normal split) and delivers it through both human and JSON
output paths.

**Pipeline change (ADR-0015, issue 15).** `report health` no longer builds
the simulation in-process from CTB + profile args. The producer/consumer
split makes the simulation an explicit step: `resinsim sim` produces
`PATH.sim.json`; `resinsim report health --in PATH.sim.json` consumes it.
The print-time projection contract is unchanged — total_time_sec and the
phase split still appear in stdout/JSON — but UATs drive the two-step
pipeline.

## UAT-1: `report health --in <sim.json>` human output shows Total time plus per-phase breakdown

```gherkin
Scenario: UAT-1 report health human output contains print-time section
  Given the resinsim sim subcommand has produced cube.sim.json from an STL with --printer elegoo_mars5_ultra + --resin elegoo_ceramic_grey_v2 + --n-supports 0
  When the user invokes resinsim report health --in cube.sim.json
  Then stdout contains the line "Total time:"
  And stdout contains the line "bottom:"
  And stdout contains the line "transition:"
  And stdout contains the line "normal:"
  And the process exits with code 0
```

## UAT-2: `report health --in <sim.json> --json` emits per-phase time fields summing to total

```gherkin
Scenario: UAT-2 JSON output carries total_time_sec and per-phase fields
  Given a sim.json envelope produced by `resinsim sim` against shipped profiles
  When the user invokes resinsim report health --in <PATH.sim.json> --json
  Then the JSON summary object has a numeric total_time_sec > 0
  And the JSON summary has numeric bottom_time_sec, transition_time_sec, normal_time_sec
  And bottom_time_sec + transition_time_sec + normal_time_sec equals total_time_sec within 0.1% tolerance
```

## UAT-3: Tilt release mechanism gives a strictly smaller total than Linear on factory defaults

**Rationale.** ADR-0007's release-mechanism branch predicts different
per-layer durations for Linear (lift + retract) vs Tilt (single lift_cycle).
On the shipped factory pair (elegoo_mars5_ultra Tilt with generic_standard
recipe, vs generic_msla_4k Linear with generic_standard recipe), Tilt is
10.5s per normal layer and Linear is 14.0s — Tilt total must be strictly less.
A future recipe change that reversed the direction would be a real signal,
not a flake.

```gherkin
Scenario: UAT-3 Tilt total_time_sec < Linear total_time_sec on factory defaults
  Given two sim.json envelopes from `resinsim sim` against the same STL and resin profile
  And the first produced with --printer elegoo_mars5_ultra (Tilt)
  And the second produced with --printer generic_msla_4k (Linear)
  When the user invokes resinsim report health --in <each>.sim.json --json
  Then the Tilt total_time_sec is strictly less than the Linear total_time_sec
```

### Empirical observation (Lilith Torso, 4492 layers, ADR-0015 e2e check)

| Printer (release mechanism)    | total_time_sec | Wall time     |
|--------------------------------|---------------:|---------------|
| `elegoo_mars5_ultra` (Tilt)    |          56468 | 15h 41m 08s   |
| `generic_msla_4k`  (Linear)    |          78928 | 21h 55m 28s   |

Linear is **39.7% slower** than Tilt on the same input + resin (Elegoo
Ceramic Grey V2). This matches ADR-0007's per-layer prediction (Tilt
≈10.5s vs Linear ≈14.0s on factory defaults) scaled to 4492 normal-phase
layers. A future recipe / lift-kinematics change that flipped the
direction would be a real signal — this empirical band is the
ground-truth check.
