---
issue: peel-corrections-s2-suction-split
date: 2026-07-23
---

# UAT: per-printer suction ΔP scales sealed-cavity force (ADR-0022 Stage 2)

## Rationale

ADR-0022 Stage 2 replaces the global `cavity_detector::VACUUM_PRESSURE_KPA = 50`
constant with an optional per-printer `vacuum_pressure_kpa` (ΔP) on
`PrinterProfile`, and routes the sealed-cavity force through the previously-dead
`PeelForceCalculator::suction_force` (removing the inline-vs-method duplication).
These scenarios guard that ΔP is (a) sourced per profile and scales the suction
linearly, (b) fully behaviour-preserving at the 50 kPa default, and (c) bounded
by atmospheric pressure at validation time. The ΔP magnitude itself is a KB-184
data gap (50–101 kPa); the default is indicative pending KB-173 calibration.

## UAT-1: a per-printer ΔP scales the sealed-cavity suction linearly

```gherkin
Scenario: UAT-1 a printer with vacuum_pressure_kpa set scales suction
  Given a printer profile whose vacuum_pressure_kpa is 101 kPa
  And a job containing one sealed cavity of 25 mm² sealing area
  When the job is simulated
  Then the cavity closure layer's suction_force_n equals 101 × 25 × 1e-3 = 2.525 N
  # vs 1.25 N at the 50 kPa default — force scales linearly with ΔP
```

## UAT-2: an unset ΔP is behaviour-preserving (50 kPa default)

```gherkin
Scenario: UAT-2 a printer without vacuum_pressure_kpa inherits the 50 kPa default
  Given a printer profile whose vacuum_pressure_kpa is unset
  When a job with a sealed cavity is simulated
  Then effective_vacuum_pressure_kpa() returns 50.0
  And every sealed-cavity suction_force_n equals 50 kPa × sealed_area × 1e-3
    (byte-identical to the pre-Stage-2 output)
```

## UAT-3: ΔP is validated to not exceed atmospheric

```gherkin
Scenario: UAT-3 an out-of-range ΔP is rejected at profile validation
  Given a printer profile whose vacuum_pressure_kpa exceeds 101.325 kPa (atmospheric)
  When the profile is validated (factory or TOML load)
  Then validate() returns an error naming vacuum_pressure_kpa
  # a sealed-cavity vacuum cannot pull harder than one atmosphere; 0/negative/NaN also rejected
```

## Evidence

- `crates/resinsim-core/src/app/simulation_runner.rs::tests::profile_vacuum_pressure_scales_suction`
  (UAT-1 — end-to-end profile ΔP → LayerResult.suction_force_n).
- `crates/resinsim-core/src/services/cavity_detector.rs::tests::detect_scales_force_linearly_with_pressure`
  (UAT-1 — detector-level linearity, 0.45 N at 50 kPa vs 0.90 N at 100 kPa).
- `crates/resinsim-core/src/app/simulation_runner.rs::tests::{closed_cup_triggers_suction_warning,suction_adds_to_total_force,solid_cube_no_suction}`
  (UAT-2 — default-path preservation).
- `crates/resinsim-core/src/entities/printer_profile.rs::tests::{factories_inherit_vacuum_pressure_default,legacy_toml_without_vacuum_pressure_defaults_to_none,vacuum_pressure_above_atmospheric_rejected,vacuum_pressure_at_atmospheric_accepted,vacuum_pressure_zero_rejected}`
  (UAT-2/UAT-3 — default + bounds).
- Qualitative: `inspect calibrate` on the 37 MB Athena reference print is
  unchanged from Stage 1 (corr 0.948, peak 0, R² 0.562) — the refactor preserves
  real-print behaviour at the default ΔP.
