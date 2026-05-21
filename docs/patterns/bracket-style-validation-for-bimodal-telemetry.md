---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pattern
---

# Pattern: Bracket-style validation for bimodal real-world telemetry

## Context

When a model is calibrated against real-world telemetry data, the
naive validation approach is to assert the data's mean matches the
model's predicted asymptotic value. This works when the system
spends most of its time at the asymptote.

For systems with episodic usage (a printer that prints
intermittently, a server that handles spiky traffic, a battery that
is occasionally charged), the data distribution is bimodal: most
time spent at the idle baseline, with intermittent excursions to
the active peak. A naive mean falls BETWEEN the two modes, matching
neither and failing the validation test.

## The naive validation

```rust
const EXPECTED_PLATEAU_C: f32 = 40.5;
const TOLERANCE: f64 = 0.5;
let csv_mean = samples.iter().sum::<f32>() / samples.len() as f32;
assert!((csv_mean - EXPECTED_PLATEAU_C).abs() < TOLERANCE);
```

For Mars 5 Ultra LED-case telemetry: idle ≈ 27 °C, active plateau
≈ 40.5 °C. The CSV mean is 28.3 °C (dominated by idle hours).
The naive assertion fails, but the model is fine — the data just
isn't all-plateau.

## Pattern

Replace the single-mean assertion with TWO bracket assertions:

```rust
const IDLE_BASELINE_C: f32 = 27.0;
const ACTIVE_PEAK_C: f32 = 40.5;
const TOLERANCE: f64 = 1.0;

let mut sorted = samples.clone();
sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
let median = sorted[sorted.len() / 2];
let peak = sorted.last().unwrap();

assert!((median as f64 - IDLE_BASELINE_C as f64).abs() < TOLERANCE,
    "CSV median {} must match idle baseline {}", median, IDLE_BASELINE_C);
assert!((peak as f64 - ACTIVE_PEAK_C as f64).abs() < TOLERANCE,
    "CSV peak {} must match active plateau {}", peak, ACTIVE_PEAK_C);
```

Each bracket validates ONE end of the distribution against the
model's prediction. If the median drifts, the idle baseline has
shifted (ambient or controller-electronics dissipation). If the
peak drifts, the steady-state delta has shifted (LED load or
thermal coupling). Either drift is meaningful and actionable.

## When this pattern applies

- Telemetry from systems with **episodic usage patterns** (printers,
  build runners, batteries, IoT sensors that sleep+wake).
- Multi-day fixtures that capture the **full usage envelope**, not
  just the active phase.
- Calibration data where the **physical model has multiple regimes**
  (idle, transient, steady-state).

## When this pattern does NOT apply

- Steady-state-only data (e.g. a 24/7 server running at constant
  load).
- Synthetic / lab data captured during a single active episode.
- Models with no clear regime structure (e.g. continuous-flow
  chemistry).

## Calibration use

When the brackets drift outside tolerance:

1. **Median drift** → refit the idle baseline (e.g. KB-152
   `initial_led_c`). Update the constant in lockstep with the
   model's TOML default.
2. **Peak drift** → refit the steady-state delta (e.g. KB-152
   `led_delta_t_steady_c`). Same update path.
3. **Both drift in opposite directions** → suspect ambient
   temperature change at the data collection site; check the
   ambient-CSV companion.

## See also

- KB-152 §"Calibration use" — documents the three-test invariant
  (idle median + active peak + formula evaluation) for the Mars 5
  Ultra LED case.
- `crates/resinsim-core/tests/mars5_ultra_led_case_anchoring.rs` —
  the canonical implementation of the pattern.
