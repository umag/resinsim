---
issue: recipe-aware-time-and-thermal
date: 2026-04-22
---

# ADR-0007: LED case and vat temperatures are separate coupled surfaces; per-layer time branches on release mechanism

## Status
Accepted

## Context

The pre-refactor thermal model (KB-150) treated the vat as one lumped body
heated directly from ambient:

```
T_vat(t) = T_ambient + ΔT_steady × (1 - exp(-t / τ))
```

Three empirical observations from the user's home-server Elegoo Mars 5 Ultra
telemetry (data/elegoo/, overnight Dec 2025 — Jan 2026) break this model:

1. **The measurement surface and the physics surface are different.**
   The reported 27 → 40 °C overnight rise is the **UV LED case temperature**
   via the printer's onboard thermistor, not the vat. The LED assembly and
   the vat are coupled but distinct — they have different thermal masses,
   time constants, and steady-state deltas. KB-150's single-temperature
   model can be fit to either the LED curve OR the vat curve, but not both.

2. **The LED does not start at ambient.**
   Idle-standby LED temperature sits ~4 °C above room ambient because of
   controller-electronics dissipation even with the UV LEDs off. Starting
   the integration at `T(0) = T_ambient` under-predicts the plateau by the
   same 4 °C.

3. **Per-layer time depends on release mechanism, not just lift speed.**
   Linear (classic MSLA) printers lift the plate vertically; Recipe's
   `lift_distance_mm / lift_speed_mm_min` maps directly to physical lift
   time, and a separate `retract_speed_mm_min` governs the return stroke.
   Tilt (Mars 5 Ultra, some Saturn 4 variants) printers rotate the vat to
   peel; the same `lift_distance / lift_speed` CTB fields are metadata that
   do **not** represent physical motion — the canonical per-layer release
   duration is `lift_cycle_sec`. Using `lift_distance/lift_speed` on a Tilt
   printer produces a wrong answer for per-layer time, and therefore a wrong
   cumulative time, and therefore a wrong vat temperature.

## Decision

### Two-stage thermal model

Track LED case temperature (stage A) and vat temperature (stage B) as
separate coupled surfaces. Formulas in KB-152; summary:

**Stage A — LED case vs time.** Exponential approach from
`initial_led_c` (idle-standby baseline) to `initial_led_c + led_delta_t_steady_c`
with time constant `led_tau_sec`:

```
led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 - exp(-t / led_tau_sec))
```

**Stage B — vat via coupling factor.** Dimensionless `led_to_vat_coupling`
∈ [0, 1] captures conduction through the printer frame, radiation through
the LCD, and convection in the vat:

```
vat_temp = ambient_c + coupling × (led_temp - ambient_c)
```

At `coupling = 0` the vat is perfectly isolated (stays at ambient); at
`coupling = 1` the vat equals the LED case. Mars 5 Ultra's user-estimate
coupling = 0.71 (KB-152) is the first fitted value; all other printers
default to `led_to_vat_coupling = 0.5` until measured.

Legacy KB-150 vectors pass through the new API as
`vat_temperature_at_layer_v2(..., initial_led_c = ambient, coupling = 1.0)`,
which collapses stage B to identity and makes stage A numerically identical
to the old single-stage formula. Delegation test:
`v2_legacy_delegation_matches_kb150_vector` in
`services/thermal_calculator.rs`.

### Release mechanism as a PrinterProfile axis

Add `PrinterProfile.release_mechanism: ReleaseMechanism` with two variants:

- `Linear` — build plate lifts vertically; per-layer time uses
  `lift_distance_mm / lift_speed_mm_min` + `lift_distance_mm / retract_speed_mm_min`
  plus all three waits. Default for legacy TOMLs via `#[serde(default)]`.
- `Tilt` — vat hinges to peel; per-layer time uses the lumped
  `lift_cycle_sec`. `wait_before_release_sec` does not apply (atomic motion).
  `retract_speed_mm_min` is carried for schema symmetry but not read on
  Tilt.

LayerTimingCalculator branches on this field; callers just pass
`(recipe, printer, layer)` and get back a time that reflects the actual
mechanism.

### Tilt fidelity is intentionally coarse

A full Tilt model requires `tilt_angle_deg` + `tilt_rate_deg_s` + vat-hinge
geometry, none of which are in the Recipe schema today. Deferred to a
follow-on issue. The lumped `lift_cycle_sec` is accurate for the per-layer
cumulative-time rollup (calibrated by measuring total print time), but it
collapses the sub-second peel-to-rest profile into one number. That sub-
second profile does not feed into the thermal model or the cure model, so
the approximation is load-bearing only for peel-force timing — tracked as a
known limitation, not a correctness bug.

### Cure-kinetics Ec(T) correction

Ec(T) Arrhenius correction gets its own record (KB-153) but lands in this
ADR because it is wired in tandem with the two-stage thermal model — both
use `vat_temp` as their physics surface. The correction is on by default
with a literature-midpoint `cure_kinetics_ea_kj_mol = 30 kJ/mol`; per-resin
measurements override via the TOML field. See KB-153 for rationale +
uncertainty band.

## Alternatives considered

**(a) Single-stage model with tuned τ + ΔT per printer.** Can reproduce
one curve (LED OR vat) but not both simultaneously, and forces the idle-
standby offset into an artificial ambient inflation. Rejected because the
artificial ambient would contaminate viscosity (Arrhenius) and Ec(T)
calculations downstream.

**(b) CFD / spatial thermal diffusion.** Correct physics, wrong cost. The
Phase-1 plan scopes thermal as lumped-capacitance; a CFD solver is Tier-2.

**(c) Fold release-mechanism into a `lift_model: enum` on Recipe.** The
mechanism is a property of the machine, not the operating point —
PrinterProfile axis per ADR-0005's three-axis split. Rejected as DDD
violation.

## Consequences

- `LayerTimingCalculator::cumulative_times_sec(recipe, printer, n)` is the
  canonical per-layer-time source. The legacy
  `ThermalCalculator::vat_temperature_at_layer(exposure, lift_cycle, ...)` is
  retained for KB-150 regression vectors; new code uses
  `vat_temperature_at_layer_v2`.
- `FailurePredictor` + `SimulationRunner` entry points gain
  `initial_led_temp_c: Option<f32>` (threaded from CLI
  `--initial-led-temp`). `None` preserves legacy semantics
  (initial_led = ambient).
- `PrinterProfile.led_delta_t_steady_c` / `led_tau_sec` /
  `led_to_vat_coupling` are DELTA-semantics fields paralleling the existing
  `delta_t_steady_c` / `thermal_tau_sec` — NOT absolute temperatures.
  Legacy defaults: `10.0 / 1200.0 / 0.5` (conservative midpoint until
  calibration data lands).
- Depends on ADR-0005 (three-axis split: release_mechanism is a Printer
  field, not a Recipe field). Neutral w.r.t. ADR-0006 (ambient-boundary
  policy for cavity detection operates on geometry, not thermal state).

## References

- KB-150 — legacy single-stage formula (superseded-for-context, regression
  vectors still exercised via the new delegation path).
- KB-152 — two-stage LED → vat thermal coupling (rationale, fitted
  coefficients, raw CSV fixtures).
- KB-153 — cure-kinetics Ea (KB-150's sibling for Ec(T) Arrhenius
  correction).
- data/elegoo/ — raw overnight CSV telemetry used to fit Mars 5 Ultra
  coefficients.
- ADR-0005 — three-axis printer / resin / pairing split that this record
  extends (release_mechanism is an Axis-1 field).
