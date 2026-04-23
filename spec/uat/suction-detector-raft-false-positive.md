---
issue: suction-detector-raft-false-positive
date: 2026-04-21
---

# UAT: `SuctionDetector` distinguishes sealed cavities from fluid-permeable support regions

## Background

The previous `SuctionDetector` used a 2D area-drop heuristic: any layer whose
cross-sectional area dropped sharply relative to the previous layer was
flagged as a sealed cavity and assigned ~50 kPa × area of suction force. This
heuristic cannot distinguish between two topologically different geometries
that produce the same 2D area signature:

- **Sealed cavity** (ring walls around trapped fluid) — real suction risk.
- **Fluid-permeable supports** (discrete columns atop a raft with gaps that
  reach the lateral bounding box) — no suction, fluid drains to the vat.

Rafts atop supports are the single most common non-trivial MSLA print
topology, so every rafted print produced a false-positive
`SuctionCup` critical failure at the raft-exit layer, often compounded by
downstream fabricated `SupportOverload` and `ZAxisCatastrophic` events.

The fix replaces the area-drop heuristic with a 3D-topology-aware
`CavityDetector` that only emits events for voids truly sealed on all sides.

## UAT-1: Fluid-permeable supports produce no suction event

**Rationale.** The load-bearing regression. A stack of layers where a raft
(fully-solid plate) is followed by discrete support columns with
inter-column gaps touching the lateral bounding box must not produce any
`SuctionCup` critical failure — the void between columns drains laterally
to the vat and creates no vacuum during peel.

```gherkin
Scenario: UAT-1 fluid-permeable supports produce no suction event
  Given a LayerInput stack comprising:
    """
    solid raft (width × height fully-solid LayerMask) for the first ~23 layers
    discrete-column layer (multiple small solid footprints with inter-column gaps spanning the full bbox width/height) for ~8 layers
    solid model body above
    """
  When SimulationRunner.run_from_layer_inputs(layers, resin, printer, ...) is invoked
  Then no FailureEvent { failure_type: SuctionCup, severity: Critical } appears in sim.failures()
```

**Evidence:**

- `resinsim-core/tests/suction_detector_integration.rs::raft_plus_fluid_permeable_supports_emits_no_suction_critical`
- `resinsim-core/tests/cavity_detector.rs::raft_plus_columns_no_suction`

## UAT-2: Topologically-sealed cavity produces one event at closure

**Rationale.** Positive reproduction — the detector must still flag real
suction cups. A closed cup (solid floor → ring walls → solid cap) produces
exactly one event at the layer that closes the cavity from the FEP side.

```gherkin
Scenario: UAT-2 topologically-sealed cavity produces one event at closure
  Given a LayerInput stack comprising:
    """
    solid base mask for layer 0
    ring-wall masks (outer frame solid, interior void) for layers 1..N-1
    solid cap mask at layer N
    """
  When the simulation runs
  Then exactly one SuctionCup failure appears in sim.failures()
  And it is at layer N (the cap layer)
  And sealed_area_mm2 equals the ring interior's cell count × voxel²
  And suction_force_n equals 50 kPa × sealed_area_mm2 × 1e-3
```

**Evidence:**

- `resinsim-core/tests/cavity_detector.rs::closed_cup_emits_one_event`
- `resinsim-core/src/app/simulation_runner.rs::tests::closed_cup_triggers_suction_warning`

## UAT-3: Open-topped hollow produces no event

**Rationale.** A cup whose walls never close (open at the FEP side through
the final layer) is not a sealed cavity. The wall peel produces no
concentrated vacuum; each ring-layer peel releases cleanly. No event.

```gherkin
Scenario: UAT-3 open-topped hollow produces no event
  Given a LayerInput stack with solid floor + ring walls continuing to the last layer (no solid cap)
  When the simulation runs
  Then sim.failures().iter().filter(|f| f.failure_type == SuctionCup).count() == 0
```

**Evidence:**

- `resinsim-core/tests/cavity_detector.rs::open_topped_cup_no_events`

## UAT-4: Multiple disjoint cavities produce separate events

**Rationale.** A print with more than one sealed cavity must emit one event
per cavity, each at its respective closure layer, independent of ordering
or spatial proximity (as long as topologically disjoint).

```gherkin
Scenario: UAT-4 multiple disjoint cavities produce separate events
  Given a stack containing N topologically-separated sealed cavities
  When the simulation runs
  Then exactly N SuctionCup failures appear
  And each fires at its own cavity's closure layer
```

**Evidence:**

- `resinsim-core/tests/cavity_detector.rs::two_disjoint_cups_two_events`
- `resinsim-core/tests/cavity_detector.rs::proptest_swiss_cheese_small_cube`
- `resinsim-core/tests/cavity_detector.rs::proptest_swiss_cheese_buildplate_scale`
  — validates the above property over randomised placements inside the
  full Mars 5 Ultra build volume (~3.2M voxels per stack).

## UAT-5: Sub-threshold cavities are suppressed

**Rationale.** A cavity too small to overpower the lift mechanism is not a
print-failing risk. Below `MIN_SEALED_AREA_MM2` (1.0 mm² at the detector),
events are not emitted. Below the downstream 1 N gate in
`FailurePredictor`, no failure is reported even if an event emerges.

```gherkin
Scenario: UAT-5 sub-threshold cavities are suppressed
  Given a stack with a single sealed cavity whose interior measures < 1 mm² at the configured voxel resolution
  When the detector runs
  Then CavityDetector::detect(masks) returns an empty Vec<CavityEvent>
```

**Evidence:**

- `resinsim-core/src/services/cavity_detector.rs::tests::below_min_sealed_area_threshold_no_event`

## UAT-6: Optional — external CTB fixture regression

**Rationale.** For anyone with a concrete real-world CTB that previously
mis-flagged, a gated end-to-end test validates the fix against the full
CTB parser → mask extraction → detector pipeline.

```gherkin
Scenario: UAT-6 external CTB fixture regression (optional, gated)
  Given any CTB file whose real-world print succeeded on an MSLA printer
  When "RESINSIM_EXTERNAL_CTB_FIXTURE=/path/to/any.ctb cargo nextest run --run-ignored=all external_ctb" runs the simulation end-to-end
  Then no SuctionCup critical failure appears
```

**Evidence:**

- `resinsim-core/tests/suction_detector_integration.rs::external_ctb_emits_no_suction_critical`
  (gated behind `RESINSIM_EXTERNAL_CTB_FIXTURE` env var; `#[ignore]` in
  default CI).
