---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: Tier-1 lumped model as boundary-condition source for Tier-2 field model

## Context

ResinSim's thermal model evolved in two tiers:

- **Tier-1** (ADR-0007, KB-152) — two-stage scalar lumped-capacitance
  model. One LED case temperature, one vat temperature, coupled by a
  dimensionless factor. Fast (O(1) per layer). Calibrated against the
  Mars 5 Ultra LED-case telemetry; vat-side is an estimated coupling.
- **Tier-2** (ADR-0020, this work) — explicit 3D heat-equation solve
  over a dense voxel grid covering the full vat envelope. Replaces the
  scalar vat output with a per-voxel temperature field.

Tier-2 needs a **bottom boundary condition** at the LCD/FEP interface
(z=0 in vat coordinates) — the temperature at the LCD plane where heat
flows into the resin from the UV LED assembly. The natural source is
the LED case temperature: the LED case is what the printer's onboard
thermistor measures, and Tier-1's Stage A (`led_case_temperature_at`)
already maps print time to LED case temperature with KB-152 coefficients.

## Pattern

**Tier-1 stays as the LED-case time-series source even when Tier-2 is
active.** Specifically:

```rust
// In SimulationRunner::apply_voxel_thermal_for_layer:
let t_layer_end = LayerTimingCalculator::cumulative_times_sec(
    recipe, printer, layer_idx
);
let led_case_c = ThermalCalculator::led_case_temperature_at(
    t_layer_end,
    printer.initial_led_c().unwrap_or(ambient_c),
    printer.led_delta_t_steady_c(),
    printer.led_tau_sec(),
);
let bcs = BoundaryConditions {
    bottom: Dirichlet(led_case_c),
    top: ConvectiveFlux(resin.convective_top_h(), ambient_c),
    sides: ConvectiveFlux(printer.h_eff_lumped(), ambient_c),
};
solver.step(&mut sim.thermal_field, dt, alpha, &bcs)?;
```

What changes between Tier-1 and Tier-2:

- **Tier-1**: Stage B (`vat_temperature_at_layer_v2`) is consulted by
  downstream consumers (cure, viscosity) — Stage B is the canonical
  vat temperature.
- **Tier-2**: Stage B is **not consulted**. Downstream consumers read
  the per-voxel ThermalField. Stage A is consulted exclusively for the
  bottom BC.

Tier-1 Stage A is still the canonical LED-case source because the LED
case is what's actually measured. Tier-2 doesn't claim to model the LED
electronics — it just consumes the LED case temperature as a boundary
condition imposed on the vat.

## Rationale

- **Single source of LED-case truth.** The LED case time-series is
  fitted to the `roden_uv_led_temp_dec_jan_hourly.csv` measurement.
  Computing it once in Tier-1 and feeding Tier-2 keeps that fit as the
  single canonical source — no risk of Tier-2 inventing a parallel
  LED-case model that diverges.
- **Honest about the calibration gap.** The vat-side temperature in
  Tier-1 is estimated (`coupling = 0.71` user estimate, no measurement).
  Reusing Stage B as a "ground truth" for Tier-2 would entrench that
  estimate. Bypassing Stage B and replacing it with the diffusion solve
  output is honest about what we know vs estimate.
- **Linear-time release-mechanism awareness.** Tier-1's `cumulative_times_sec`
  already branches on `release_mechanism` (Linear vs Tilt per ADR-0007).
  Tier-2 inherits the correct cumulative-time computation by routing
  through the same calculator — no risk of Tilt printers (Mars 5 Ultra)
  getting wrong layer cumulative times because Tier-2 was implemented
  to read `lift_distance/lift_speed` directly.

## Consequences

- **`ThermalCalculator` is preserved unchanged.** No deprecation of Tier-1
  scalar API; it survives as the LED-case source.
- **`vat_temperature_at_layer_v2` becomes dead code under field-sim** —
  Tier-2 doesn't call it. Default builds (no field-sim) still consult
  it. Acceptable per the gating scheme.
- **Calibration of Tier-1 LED-case coefficients propagates to Tier-2
  automatically.** When KB-152 coefficients are refit (e.g. after a
  vat-thermistor data-collection follow-on), Tier-2 picks up the new BC
  without code changes.
- **Inner-layer BC interpolation is a documented fidelity bound.** Tier-1
  Stage A produces ONE value per layer (`t_layer_end`). Tier-2 inner
  CFL substeps see this value held CONSTANT. For fast-changing LED-case
  temperatures (e.g. raft → first part layer with a large layer-time
  delta), interpolating between layer-start and layer-end LED-case is a
  refinement. Filed as a follow-on (`sub-substep-bc-interpolation`).

## See also

- ADR-0007 — Tier-1 two-stage model architecture
- KB-152 — Tier-1 formulas, fitted coefficients
- ADR-0020 — Tier-2 design record; §Decision vi pins this pattern
- `docs/patterns/cfl-guard-on-anisotropic-stencil.md` — sibling pattern
  for the inner substep cadence
- ADR-0017 §3 — voxel field bbox-anchor convention (Tier-2 ThermalField
  uses a DIFFERENT bbox — vat envelope — per ADR-0020 §Decision ii)
