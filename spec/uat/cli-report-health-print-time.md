---
issue: print-time-on-reportgenerator
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
output paths. Under the v4 design landed by this lifecycle, `PrintSimulation`
aggregate OWNS its `Recipe` + `PrinterProfile` so `summary()` is arg-less and
the aggregate matches its docstring contract.

## UAT-1: `report health` human output shows Total time plus per-phase breakdown

```gherkin
Scenario: UAT-1 report health human output contains print-time section
  Given the resinsim report health subcommand
  When the user invokes it with an STL + --printer elegoo_mars5_ultra + --resin elegoo_ceramic_grey_v2 + --n-supports 0
  Then stdout contains the line "Total time:"
  And stdout contains the line "bottom:"
  And stdout contains the line "transition:"
  And stdout contains the line "normal:"
  And the process exits with code 0
```

## UAT-2: `report health --json` emits per-phase time fields summing to total

```gherkin
Scenario: UAT-2 JSON output carries total_time_sec and per-phase fields
  Given the resinsim report health subcommand
  When the user invokes it with --json, an STL, --printer, --resin, and --n-supports 0
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
  Given the resinsim report health subcommand with --json
  And the same STL and the same resin profile
  When invoked once with --printer elegoo_mars5_ultra
  And once with --printer generic_msla_4k
  Then the Tilt total_time_sec is strictly less than the Linear total_time_sec
```
