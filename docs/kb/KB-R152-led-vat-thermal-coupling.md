---
id: KB-R152
issue: resinsim
kind: formula
date: 2026-04-22
source: Elegoo Mars 5 Ultra home-server overnight telemetry (resinsim/data/elegoo/)
---

# Two-stage LED → vat thermal coupling

Supersedes KB-150's single-temperature assumption for printers where LED-
case thermistor readings are available. KB-150 vectors remain valid
regression tests under the delegation path
`initial_led_c = ambient; led_to_vat_coupling = 1.0` (see ADR-0007).

## Equations

### Stage A — LED case temperature vs time

```
led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 - exp(-t / led_tau_sec))
```

Where:
- `initial_led_c` — LED case baseline at `t = 0`. Typically the printer's
  idle-standby temperature, NOT room ambient. On the Mars 5 Ultra this sits
  ~4 °C above ambient even with UV LEDs off (controller-electronics
  dissipation).
- `led_delta_t_steady_c` — asymptotic LED rise above `initial_led_c`.
- `led_tau_sec` — time constant of the LED heat-up curve.

### Stage B — vat from LED via coupling factor

```
vat_temp = ambient_c + coupling × (led_temp - ambient_c)
```

Where `coupling ∈ [0, 1]` is dimensionless:
- `coupling = 0` ⇒ perfect isolation, vat stays at ambient.
- `coupling = 1` ⇒ perfect coupling, vat equals LED case.

The coupling lumps three physical pathways (frame conduction, LCD
radiation, vat convection) into one fitted constant. See "Uncertainty"
below for the fitting method and error band.

### Per-layer cumulative time

```
t(layer) = Σ_{i=0..layer-1} layer_time_i
```

Where `layer_time_i` is the per-layer time computed by
`LayerTimingCalculator` — branches on `PrinterProfile.release_mechanism`
(ADR-0007):
- `Linear`: `exposure + wait_before_cure + lift + wait_before_release + retract + wait_after_release`
- `Tilt`:   `exposure + wait_before_cure + lift_cycle_sec + wait_after_release`

Exposure itself is phase-dependent (bottom / transition / normal) per the
three-phase model.

## Fitted coefficients

### Elegoo Mars 5 Ultra (Tilt release)

Source: overnight session at resinsim/data/elegoo/ (Dec 2025 — Jan 2026).
Plateau = 40.5 °C; idle LED baseline = 27.0 °C; kitchen ambient 23 °C;
closet ventilation. 3–4 h to 95 % of plateau ⇒ 3τ ≈ 3–4 h ⇒ τ ≈ 4000 s.

| Parameter | Value | Source |
|-----------|-------|--------|
| `initial_led_c` (user-supplied) | 27.0 | Idle LED reading before overnight session |
| `led_delta_t_steady_c` | 13.5 | plateau 40.5 − idle 27 |
| `led_tau_sec` | 4000 | 3τ ≈ 3–4 h to 95 % |
| `led_to_vat_coupling` | 0.71 | user estimate (no vat sensor): LED 40, vat ~35, ambient 23 ⇒ ΔT_led = 17, ΔT_vat = 12, 12/17 = 0.71 |

### Conservative legacy defaults

For printers without fitted data (every profile except Mars 5 Ultra today):

| Parameter | Default | Rationale |
|-----------|---------|-----------|
| `led_delta_t_steady_c` | 10.0 | midpoint for MSLA-class |
| `led_tau_sec` | 1200 | matches KB-150 legacy τ for one-profile back-compat |
| `led_to_vat_coupling` | 0.5 | conservative midpoint until measured |

## Uncertainty

1. **Coupling is a user-estimate, not a measurement.**
   The 0.71 value for Mars 5 Ultra was derived from the user's visual
   correlation between LED readout (40 °C, thermistor) and resin behaviour
   consistent with ~35 °C vat (viscosity drop, cure-depth observation).
   No vat thermistor. Re-calibrate when a vat sensor is added — the
   `led_to_vat_coupling` field is isolated and a single-value replace is
   sufficient.

2. **CSV telemetry is hourly aggregates.**
   First-hour dynamics are averaged out. Good enough for τ fit and
   steady-state plateau; inadequate for per-layer validation. Sub-hour
   resolution would require LED thermistor sampling at the printer's own
   cadence.

3. **Per-printer calibration needed.**
   Coefficients are NOT portable between printers. LED thermal mass, vat
   volume, frame geometry, and coupling all differ — a Saturn 4 needs its
   own session.

## Limitations

- Lumped coupling — does not distinguish frame vs LCD vs air pathways.
  Adequate for Phase-1 predictions; a multi-compartment model would need
  a spatial solver (Tier 2).
- Fitted at one ambient (23 °C kitchen); extrapolation to extreme ambient
  (below 15 °C or above 30 °C) is untested.
- Assumes LED duty cycle is approximately constant over the print; bursty
  duty would invalidate the constant `led_delta_t_steady_c`.

## Test vectors

Enforced in `thermal_calculator.rs` unit tests:

| Scenario | Input | Expected |
|----------|-------|----------|
| Stage A at t=0 | initial=27, Δ=13.5, τ=4000, t=0 | LED = 27.0 |
| Stage A at t=τ | initial=27, Δ=13.5, τ=4000, t=4000 | LED ≈ 27 + 13.5(1−1/e) = 35.53 |
| Stage A at t=5τ | initial=27, Δ=13.5, τ=4000, t=20000 | LED ≈ 40.4 (99.3% rise) |
| Stage B coupling=0 | LED=40, ambient=23, c=0 | vat = 23 |
| Stage B coupling=1 | LED=40, ambient=23, c=1 | vat = 40 |
| Stage B Mars 5 Ultra | LED=40, ambient=23, c=0.71 | vat = 35.07 |

Plus property tests (`tests/thermal_properties.rs`) that exercise
monotonicity, asymptote, and `vat ∈ [ambient, LED]` bounds across random
inputs.

## References

- ADR-0007 — architectural record introducing the two-stage split.
- KB-150 — legacy single-stage formula (regression-vector only).
- KB-151 — screen heat flux (complementary — the LED-side heat source that
  feeds stage A).
- KB-141 — viscosity Arrhenius (downstream consumer of `vat_temp`).
- KB-153 — cure-kinetics Arrhenius Ec(T) (sibling downstream consumer).
- resinsim/data/elegoo/README.md — raw telemetry source.
