---
issue: peel-corrections-s2-suction-split
date: 2026-07-23
---

# Pattern: Parametrize a global constant by threading it through the canonical method (not the inline copy)

## Context

A physical/config magnitude often starts life as a module-level `const` used at
one inline call site, while a *canonical* domain method that computes the same
quantity sits unused nearby. ADR-0022 Stage 2 had exactly this shape:

- `cavity_detector::VACUUM_PRESSURE_KPA = 50.0` (a global const), used by an
  inline `ΔP × area × 1e-3` formula inside `CavityDetector::detect`; and
- `PeelForceCalculator::suction_force(ΔP, area)` — the canonical formula, KB-114,
  fully tested but **dead** on the simulation path (only a standalone CLI +
  unit tests reached it).

The task was "make ΔP configurable per printer." The tempting move is to add the
config to the inline copy (give the const a profile override, keep the inline
formula). That entrenches the duplication.

## Pattern

Parametrize in the direction that *removes* the duplication:

1. **Home the value on the aggregate that owns it.** Add an optional field to the
   entity, following the established optional-field template
   (`Option<f32>` + `#[serde(default)]` + `effective_*()` + validate + factory)
   — here `PrinterProfile::vacuum_pressure_kpa` /
   `effective_vacuum_pressure_kpa()`, defaulting to the migrated const.
2. **Thread the parameter down the existing call chain**, not around it:
   `build_suction_map(masks, printer.effective_vacuum_pressure_kpa())` →
   `SuctionDetector::detect_from_masks(masks, ΔP)` →
   `CavityDetector::detect(masks, ΔP)`.
3. **Replace the inline formula with a call to the canonical method** at the leaf:
   the detector now does
   `PeelForceCalculator::suction_force(ΔP, CrossSectionArea::new(sealed_area)…)`.
   One move both parametrizes the value **and** revives the dead method, deleting
   the duplicate formula.

Keep the default numerically identical (here 50 kPa) so the change is
behaviour-preserving — prove it with the existing tolerance tests, the golden
diff (only the new serialized field appears), and a real-data regression
(`inspect calibrate` unchanged).

## Why

- **Single source of truth for the formula.** After the change there is exactly
  one place that computes `ΔP × A × 1e-3`; a future correction can't miss a copy.
- **Parametrization and de-duplication are the same edit**, so neither is
  deferred to a "later cleanup" that never lands.
- **The reviewer's "route through the shared code path" check passes by
  construction** — the new config flows into the canonical method rather than a
  parallel one, so it's never a new-entry-point HIGH.

## When NOT to

If the canonical method and the inline copy have genuinely diverged in meaning
(different units, different regime), don't force them together — reconcile the
semantics first, or the dedup hides a real difference.

## See also

- ADR-0022 Stage 2; `crates/resinsim-core/src/services/{cavity_detector,peel_force_calculator,suction_detector}.rs`.
- The optional-field template: `entities/resin_profile.rs::cure_kinetics_ea_kj_mol`,
  `entities/printer_profile.rs::{crosstalk_sigma_*,vat_wall_*}`.
- KB-184 (the ΔP data gap this field will eventually be calibrated against).
