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

**Scenario:**

Given a printer profile `P` with `layer_height_range_um = { min: 100.0, max: 150.0 }`
And a resin profile `R` whose `recipe.layer_height_um = 50.0` (below range min)
When `SimulationRunner::run_from_areas(areas, R, P, ...)` is invoked
Then the call returns `Err` whose message begins with `"pairing:"`
  And the error names `layer_height_um` as the offending recipe field
  And `slice_areas` was never called (observable: no geometry layer was sliced)
  And `predict_layer` was never called

## UAT-2: ALL violations reported in one pass

**Rationale.** When a recipe violates multiple range constraints (e.g. layer
height below the minimum AND exposure above the maximum), `PairingValidator`
collects every violation into `Vec<String>` and reports them together. A user
fixing a misconfigured recipe should see every mismatch in one pass, not have
to iterate through N fix-and-rerun cycles for N violations.

**Scenario:**

Given a printer `P` with:
  - `layer_height_range_um = { min: 100.0, max: 150.0 }`
  - `exposure_range_sec = { min: 10.0, max: 60.0 }`
And a resin `R` whose `recipe` has:
  - `layer_height_um = 50.0` (below range min)
  - `normal_exposure_sec = 2.5` (below range min)
When `SimulationRunner::run_from_areas(areas, R, P, ...)` is invoked
Then the returned Err contains BOTH `layer_height_um` AND `normal_exposure_sec`
  joined with `"; "` in a single error message
  And the simulation did not proceed to `slice_areas`
