---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: Recipe inside printer envelope → simulation runs

## UAT-1: Happy-path pairing

**Rationale.** ADR-0005 moved exposure, layer height, lift speed, and bottom-layer
count off `PrinterProfile` and into a new `Recipe` value object nested in
`ResinProfile`. `PairingValidator` gates simulation entry: a recipe whose fields
lie within the paired printer's hardware envelope must pass through cleanly,
exercise the full physics pipeline, and produce a `PrintSimulation` result.
Locks the affirmative contract — pairing does not mistakenly reject valid pairs.

**Scenario:**

Given a printer profile `P` with:
  - `layer_height_range_um = { min: 20.0, max: 100.0 }`
  - `exposure_range_sec = { min: 1.0, max: 60.0 }`
  - `lift_speed_range_mm_min = { min: 10.0, max: 200.0 }`
  - `bottom_layer_count_max = 15`
And a resin profile `R` whose `recipe` has:
  - `layer_height_um = 50.0`
  - `normal_exposure_sec = 2.5`, `bottom_exposure_sec = 25.0`
  - `lift_speed_mm_min = 60.0`
  - `bottom_layer_count = 6`
When `SimulationRunner::run_from_areas(areas, R, P, ...)` is invoked on a
  non-trivial area vector
Then `validate_pairing(P, R.recipe())` returns `Ok(())`
  And `SimulationRunner` proceeds through `slice_areas → predict_layer` for
      every layer
  And the returned `PrintSimulation` has the expected layer count
  And no pairing-prefixed error appears in the Err path

## UAT-2: Boundary values accepted

**Rationale.** Pairing uses inclusive comparison (`value >= range.min && value <= range.max`).
A recipe pinned exactly at a printer's range boundary — layer_height = range.min,
or lift_speed = range.max — is still a valid pairing. Fixed-parameter hardware
(a zero-width range `min == max`) must also accept a matching recipe value.

**Scenario:**

Given a printer `P` with `layer_height_range_um = { min: 20.0, max: 100.0 }`
  and resin `R` whose `recipe.layer_height_um = 20.0` (exactly at min)
When `validate_pairing(P, R.recipe())` is called
Then it returns `Ok(())`
  And the same is true when `recipe.lift_speed_mm_min` equals the range max
      of `lift_speed_range_mm_min`

Given a printer `P2` pinned to a single layer height
  (`layer_height_range_um = { min: 50.0, max: 50.0 }`)
  and resin `R2` whose `recipe.layer_height_um = 50.0`
When pairing is validated
Then it returns `Ok(())`
  And `R2` with `recipe.layer_height_um = 50.1` returns `Err` naming the field
