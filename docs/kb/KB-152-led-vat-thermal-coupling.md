---
issue: recipe-aware-time-and-thermal (created 2026-04-22)
authored: t2f4-thermal-diffusion (2026-05-21)
date: 2026-05-21
---

# KB-152: Two-stage LED → vat thermal coupling

Tier-1 lumped-capacitance thermal model for resinsim. Tracks two coupled
surfaces — the UV LED case (stage A, directly observable via the printer's
onboard thermistor) and the resin vat (stage B, inferred via a coupling
factor). This is the load-bearing scalar model that downstream consumers
read for viscosity, cure kinetics, and per-layer thermal dispatch.

**Authoring provenance.** The decisions captured here originally lived
inline in ADR-0007 §Decision. They were lifted into this KB during the
t2f4-thermal-diffusion lifecycle on 2026-05-21 so the formulas + fitted
coefficients have a canonical home, separate from the architectural decision
record. ADR-0007 now keeps a "see KB-152" pointer.

## Formulas

### Stage A — LED case vs time

Exponential approach from `initial_led_c` (idle-standby baseline) to
`initial_led_c + led_delta_t_steady_c` with time constant `led_tau_sec`:

```
led_temp(t) = initial_led_c + led_delta_t_steady_c × (1 − exp(−t / led_tau_sec))
```

`initial_led_c` is the LED case temperature at print start. The user
specifies it via `--initial-led-temp` (defaults to `ambient_c` if absent,
preserving legacy KB-150 semantics).

### Stage B — vat via coupling factor

Dimensionless `led_to_vat_coupling ∈ [0, 1]` captures conduction through
the printer frame, radiation through the LCD, and convection in the vat:

```
vat_temp = ambient_c + coupling × (led_temp − ambient_c)
```

At `coupling = 0` the vat is perfectly isolated (stays at ambient); at
`coupling = 1` the vat equals the LED case.

### Legacy KB-150 delegation

Legacy KB-150 vectors pass through the new API as

```
vat_temperature_at_layer_v2(..., initial_led_c = ambient, coupling = 1.0)
```

which collapses stage B to identity and makes stage A numerically identical
to the old single-stage formula. The delegation invariant is enforced by
`v2_legacy_delegation_matches_kb150_vector` in
`crates/resinsim-core/src/services/thermal_calculator.rs`.

## Mars 5 Ultra fitted coefficients

Fitted from home-server Elegoo Mars 5 Ultra telemetry over Dec 2025 –
Jan 2026 (`data/elegoo/`). Set in
`data/printers/elegoo_mars5_ultra.toml`:

| Coefficient | Fitted value | Derivation |
|---|---|---|
| `led_delta_t_steady_c` | **13.5 °C** | Plateau ≈ 40.5 °C − idle ≈ 27 °C; from `roden_uv_led_temp_dec_jan_hourly.csv` |
| `led_tau_sec` | **4000 s** (≈ 67 min) | 3 τ ≈ 3–4 h to 95 % of plateau; same CSV |
| `led_to_vat_coupling` | **0.71** | **USER ESTIMATE** — no vat thermistor data exists. Recalibrate when a vat sensor is added |
| `ambient_c` | live, per-CSV | Sourced from `kitchen_temperature_dec_jan_hourly.csv` |

`led_to_vat_coupling = 0.5` is the conservative midpoint default for all
other printer profiles until printer-specific calibration data lands.

## Telemetry provenance (`data/elegoo/`)

- **`roden_uv_led_temp_dec_jan_hourly.csv`** — Hourly aggregates of UV LED
  case temperature (mean / min / max, °C) on the user's Mars 5 Ultra.
  Columns: `timestamp_start, timestamp_end, mean_c, min_c, max_c`. 722
  hours of data. Anchors the Stage A fit.
- **`kitchen_temperature_dec_jan_hourly.csv`** — Hourly aggregates of
  ambient kitchen temperature at the same site. Same column shape. 1,393
  hours of data. Used as the `ambient_c` calibration input.

## Vat-side ground truth gap (load-bearing)

**There is no vat thermistor data in `data/elegoo/`.** The 0.71 coupling
value is a user estimate, not a measurement. This gap propagates forward:

- Tier-2 (`t2f4-thermal-diffusion`) anchors its LED case boundary
  condition against `roden_uv_led_temp_*.csv` (ground truth) but cannot
  anchor the vat-side against raw telemetry.
- The vat-side surface temperature in Tier-2 is reported in the run-end
  summary log but not constrained against external data.
- A follow-on ticket "collect vat-thermistor telemetry on Mars 5 Ultra"
  (BME280 + thermocouple proposal) is filed at t2f4 harvest. Once
  collected, KB-152 should be revised with a calibrated coupling value
  and the same vat-thermistor anchor extended to Tier-2.

## Calibration use

When this KB is updated with new fitted values:

1. Run the integration test
   `crates/resinsim-core/tests/mars5_ultra_led_case_anchoring.rs` (added
   by t2f4). Three assertions guard the fit:
   - `mars5_ultra_led_csv_idle_median_matches_kb152_initial_led` — the
     722-hour fixture's median hourly mean is within ±1.0 °C of
     `initial_led_c = 27 °C` (idle-standby baseline).
   - `mars5_ultra_led_csv_active_peak_matches_kb152_plateau` — the
     fixture's peak hourly mean is within ±1.0 °C of
     `initial_led_c + led_delta_t_steady_c = 40.5 °C` (plateau).
   - `thermal_calculator_far_future_matches_kb152_plateau` — the
     `ThermalCalculator::led_temperature_at_time(10 τ)` formula
     evaluation matches the plateau prediction within 0.01 °C
     (formula correctness, not fit drift).

   The bracket-style (idle median + active peak) shape replaces the
   originally-planned `mean ± 0.5 °C` because the real telemetry is
   dominated by idle hours (the printer prints intermittently); a
   single-number mean undershoots the plateau.

2. Update the `data/printers/elegoo_mars5_ultra.toml` values in
   lockstep.
3. Document the recalibration date in the table above with a note
   about what data informed the change.

## See also

- ADR-0007 — accepts the two-stage model architecturally; defers spatial
  diffusion to Tier-2. Points to this KB for formulas + coefficients.
- ADR-0020 — Tier-2 spatial thermal diffusion; uses Stage A LED case as
  the Dirichlet bottom boundary condition source.
- `data/elegoo/README.md` — raw telemetry provenance.
- KB-150 — legacy single-stage formula (superseded-for-context; regression
  vectors still exercised via the v2 delegation path).
- KB-153 — Ec(T) Arrhenius correction (KB-152's sibling for cure kinetics).
