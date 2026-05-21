---
issue: t2f4-thermal-diffusion
date: 2026-05-21
status: pending-implementation
---

# UAT: PrinterProfile rejects sub-minimum envelope extents under field-sim

## Rationale

Code-review MEDIUM finding (round 1, 2026-05-21): the
`SimulationRunner` voxel-state init computes
`nx_thermal = (envelope.width_mm / thermal_voxel_mm).round().max(2.0) as u32`
which silently inflates the thermal field's physical span for
sub-millimetre printer envelopes. A printer with `width_mm = 1.5`
at `THERMAL_VOXEL_MIN_MM = 2.0` rounds to 1 then clamps to 2,
producing a 4 mm × 4 mm × 4 mm thermal domain physically LARGER
than the printer. No real printer hits this today (Mars-class
envelopes are 100+ mm) but the silent clamp is a latent semantic
bug.

The polish fix lifts the validation to `PrinterProfile::validate()`
under `cfg(feature = "field-sim")` — reject envelopes smaller than
`2 × THERMAL_VOXEL_MIN_MM = 4 mm` per axis with an actionable error.

**Status: pending-implementation.** This UAT scenario is the TDD
anchor for the polish ticket — it documents the intended contract
ahead of the implementation, so when the polish lands, the test
already exists to fail-then-pass.

## UAT-1: envelope smaller than 2 × THERMAL_VOXEL_MIN_MM per axis is rejected

```gherkin
Scenario: PrinterProfile.validate() rejects sub-minimum envelope extents
          under the field-sim feature
  Given a PrinterProfile under the field-sim feature
  And `build_envelope_mm.width_mm = 3.0` (below 2 * THERMAL_VOXEL_MIN_MM = 4.0 mm)
  When `printer.validate()` runs
  Then it returns `Err` whose message names `build_envelope_mm.width_mm`
       and the minimum-extent threshold
  And the message references ADR-0020 §Decision i for the rationale
  And the same TOML loads + validates cleanly under a default-feature
       binary (the field-sim envelope-extent check is gated by `cfg`)
```

## See also

- Code-review round 1 finding "MED: sub-mm envelope silent-clamp at
  simulation_runner.rs:580".
- ADR-0020 §Decision i — FTCS + CFL budget.
- `crates/resinsim-core/src/app/simulation_runner.rs:580` — the
  current silent-clamp site.
