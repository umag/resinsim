---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: Switching resin on the same printer changes simulation output

## UAT-1: Same printer + different resin produces different cure depth

**Rationale.** The motivating observation from the ADR-0005 triage:

> "A Saturn with Ceramic Grey uses 2.0 s exposure, the same Saturn with Premium Black
> uses 2.5 s. Today, simulating a different resin on the same printer silently uses the
> wrong exposure because exposure lives on the printer."

Pre-refactor, switching resin on a paired `SimulationRunner::run_from_areas` call would
produce identical exposure-derived results because exposure came from `PrinterProfile`.
Post-refactor, exposure comes from `ResinProfile.recipe()` — so the same printer + a
different resin drives a different simulation.

Companion UATs (`recipe-inside-printer-range.md`, `recipe-outside-printer-range.md`) lock
the *safety rail* (pairing validates ranges). This UAT locks the *behavioural fix*: the
refactor's whole point is that the recipe travels with the resin, and this is observable
at the simulation output layer.

```gherkin
Scenario: UAT-1 same printer + different resin produces different cure depth
  Given a printer profile "P" (e.g. PrinterProfile::elegoo_mars5_ultra())
  And two resin profiles "R_A" and "R_B" that both pair with "P" but differ in recipe:
    | resin | factory                               | recipe.normal_exposure_sec |
    | R_A   | ResinProfile::generic_standard()       | 2.5                        |
    | R_B   | ResinProfile::elegoo_ceramic_grey_v2() | 3.2                        |
  And a fixed-area per-layer vector "areas" exercising non-bottom layers
  When SimulationRunner.run_from_areas(areas, R_A, P, ...) produces "sim_A"
  And SimulationRunner.run_from_areas(areas, R_B, P, ...) produces "sim_B"
  Then sim_A.layers[i].cure_depth_um != sim_B.layers[i].cure_depth_um for non-bottom layers i (Beer-Lambert scales with exposure-derived energy; different normal_exposure_sec produces different cure depth)
  And the observed cure-depth difference reflects R_A.recipe vs R_B.recipe, not P's range fields (P now carries only hardware envelopes, no baked recipe values)
```

**Regression framing.** If a future PR reverts `FailurePredictor::predict_layer` to read
exposure from `printer` instead of `recipe`, this UAT surfaces the regression: both
`sim_A` and `sim_B` would produce identical `cure_depth_um` (because `P` is the same).

## UAT-2: Same printer + same resin produces identical output (sanity)

**Rationale.** Orthogonal invariant: the refactor doesn't spuriously introduce
non-determinism. Same inputs → same outputs.

```gherkin
Scenario: UAT-2 same printer + same resin produces identical output (determinism sanity)
  Given a printer "P" and resin "R" that pair
  When SimulationRunner.run_from_areas(areas, R, P, ...) is called twice with the same inputs
  Then the two PrintSimulation outputs are structurally equal
  And no side-effect has been observed (resin/printer are unchanged; repositories untouched)
```
