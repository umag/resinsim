# Elegoo telemetry — calibration fixtures

This directory holds user-supplied home-server telemetry from an Elegoo printer.
**Not** to be confused with `data/athena/` which is reserved for Athena II
force-sensor data (a different printer brand).

## Files

### `roden_uv_led_temp_dec_jan_hourly.csv`

Hourly aggregates (mean / min / max, °C) of the UV LED case temperature on the
user's Elegoo Mars 5 Ultra, captured by the home server over Dec 2025 – Jan 2026.

Columns: `timestamp_start, timestamp_end, mean_c, min_c, max_c`. 722 hours of data.

### `kitchen_temperature_dec_jan_hourly.csv`

Hourly aggregates (mean / min / max, °C) of the ambient kitchen temperature at
the same site and cadence, used as the `ambient_c` calibration input.

Columns: `timestamp_start, timestamp_end, mean_c, min_c, max_c`. 1,393 hours of data.

## Calibration use

The data feeds `KB-152-led-vat-thermal-coupling.md` and the fitted values in
`data/printers/elegoo_mars5_ultra.toml`:

| Coefficient | Fitted value | Derivation |
|---|---|---|
| `led_delta_t_steady_c` | **13.5 °C** | plateau ≈ 40.5 °C − idle ≈ 27 °C |
| `led_tau_sec` | **~4000 s** | 3–4 h to reach 95 % of plateau ⇒ 3τ ≈ 3–4 h |
| `led_to_vat_coupling` | **0.71** | LED 40, vat estimated 35, ambient 23 ⇒ ΔT_led = 17, ΔT_vat = 12, 12/17 ≈ 0.71 |

## Environment context

- **Printer:** Elegoo Mars 5 Ultra (TILT-RELEASE mechanism — see ADR-0007)
- **Room ambient:** ~23 °C (kitchen)
- **Printer location:** closet with ventilation (semi-confined air volume — heat
  accumulates slower-to-dissipate than in an open room, so the fitted `τ` is
  specific to this enclosure and should be re-calibrated if moved)
- **Overnight plateau:** LED reaches 40.3–40.6 °C within 3–4 h of print start
  and stays stable for the rest of the print
- **LED idle baseline:** 26.5–27.0 °C (standby electronics dissipation above the
  23 °C room ambient — not a warm-up cycle)

## Known limitations

- Hourly aggregates average out the first-hour dynamics. Good enough to fit
  `τ` and steady-state; inadequate for per-layer or sub-hour validation.
- Vat / resin temperature is **estimated** (no vat sensor). The user's 35 °C
  estimate is based on LED-heatsink proximity reasoning, not measurement.
  Re-calibrate `led_to_vat_coupling` when a vat sensor is added.
- Coefficients are specific to this printer + enclosure + overnight session;
  different printers or print conditions will need their own calibration.
