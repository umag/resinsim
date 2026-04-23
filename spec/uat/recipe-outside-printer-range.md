---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: Recipe outside printer envelope → ALL violations reported before slicing

## UAT-1: Pairing fails before slicing

**Rationale.** ADR-0005 Consequences require pairing to fire at simulation entry,
BEFORE `slice_areas` or `predict_layer`. An out-of-range recipe must short-circuit
the simulation with a clear error — not after geometry has been processed. This
prevents wasted work and surfaces user misconfiguration immediately.

```gherkin
Scenario: UAT-1 Pairing fails before slicing
  Given a narrowed printer "P" with layer_height_range_um min 100.0 max 150.0
  And a resin profile "R" whose recipe has layer_height_um 50.0
  When SimulationRunner.run_from_areas is invoked
  Then the call returns Err whose message begins with "pairing:"
  And the error names "layer_height_um" as the offending recipe field
```

Two observability notes — "`slice_areas` was never called" and "`predict_layer`
was never called" — are implicit in the short-circuit: `run_from_areas` returns
`Err` at line 92-93 (pairing gate) before any call to `slice_areas` (not in
scope on this path) or `predict_layer`. If pairing stops accepting violations,
the unit test at `simulation_runner.rs::pairing_violation_returns_err_before_slice_areas`
fires before this UAT would run.

## UAT-2: ALL violations reported in one pass

**Rationale.** When a recipe violates multiple range constraints (e.g. layer
height below the minimum AND exposure above the maximum), `PairingValidator`
collects every violation into `Vec<String>` and reports them together. A user
fixing a misconfigured recipe should see every mismatch in one pass, not have
to iterate through N fix-and-rerun cycles for N violations.

```gherkin
Scenario: UAT-2 ALL violations reported in one pass
  Given a narrowed printer "P" with ranges:
    | range                 | min   | max   |
    | layer_height_range_um | 100.0 | 150.0 |
    | exposure_range_sec    | 10.0  | 60.0  |
  And a resin "R" whose recipe has:
    | field               | value |
    | layer_height_um     | 50.0  |
    | normal_exposure_sec | 2.5   |
  When SimulationRunner.run_from_areas is invoked
  Then the returned Err begins with "pairing:"
  And the returned Err mentions every field:
    """
    layer_height_um
    normal_exposure_sec
    """
  And violations are joined with "; " in a single error message
```

Observability note — "the simulation did not proceed to `slice_areas`" — is
implicit in the short-circuit: the pairing gate at `run_from_areas:92-93`
fires before any downstream slicing. Locked by
`simulation_runner.rs::pairing_reports_all_violations_at_once`.
