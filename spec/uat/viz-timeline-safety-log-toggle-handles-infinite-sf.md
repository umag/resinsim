---
issue: 05-layer-timeline-chart
date: 2026-04-27
---

# UAT: Safety factor log-scale toggle handles ∞-SF layers without panic

## Rationale

`safety-factor-zero-force.md` (UAT-1, issue T1-F2) pins
`safety_factor = f32::INFINITY` for any layer whose computed peel
force is zero. The issue 05 chart projects safety_factor as a line;
the linear projection drops `∞` (it isn't finite), and the log10
projection additionally drops zero / negative values (log10 is
undefined there).

The log-scale toggle is a sub-option of `show_safety` — visible only
when the parent is enabled, and reset to false when the parent is
disabled (so re-enabling Safety later starts in linear mode rather
than silently remembering log).

## UAT-1: ∞-SF layer is absent from both linear and log safety series

```gherkin
Scenario: ∞-SF layer is absent from the safety line in both modes
  Given the resinsim-viz binary running with --load-ctb + --load-sim
        for a print where at least one layer has zero peel force
        (e.g. a sliced-area-zero geometry — the bottom rafts of
        many test prints satisfy this)
  When the user enables the "Safety factor" series
  Then no panic is raised when the chart paints
  And the safety line has gaps at the layer indices whose
      safety_factor is INFINITY
  And the safety series y values for the surviving layers are
      finite-positive

  When the user enables the "log10" sub-checkbox
  Then no panic is raised
  And the safety series additionally drops any layers with
      safety_factor ≤ 0 (none expected in normal sims, but the
      filter is correctness-load-bearing)
  And the legend label changes from "Safety factor (×)" to
      "Safety factor (log10)"
```

## UAT-2: log toggle visibility tracks show_safety

```gherkin
Scenario: log10 toggle is hidden when Safety factor is off
  Given a fresh resinsim-viz session, the bottom panel rendering
  When the user observes the checkbox row above the chart
  Then "log10" checkbox is NOT visible (Safety factor is off by
       default per the issue body)

  When the user enables "Safety factor"
  Then "log10" checkbox becomes visible

  When the user enables "log10", then disables "Safety factor"
  Then "log10" is no longer visible
  And the next time "Safety factor" is enabled, the chart paints
      in linear mode (log10 resets — unsurprising re-enable)
```
