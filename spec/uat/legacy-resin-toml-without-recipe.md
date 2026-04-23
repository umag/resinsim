---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: Legacy (pre-ADR-0005) resin TOML without `[recipe]` table → loud parse error

**Migration guidance for community resin TOMLs.** If you have a pre-ADR-0005 resin
TOML (e.g. from the wild, from community contributions, or from a bespoke calibration
session), the following two fields are now REQUIRED and have no serde defaults:

1. **`ref_lift_speed_mm_min`** (chemistry metadata, was on `PrinterProfile`): add
   `ref_lift_speed_mm_min = 60.0` alongside the chemistry fields unless you know the
   specific speed at which `peel_adhesion_kpa` was measured (KB-112, KB-114).
2. **`[recipe]` table**: add the operating-point values (layer_height_um, exposure
   times, lift kinematics, wait times). See `data/resins/generic_standard.toml` for
   a complete template.

Both requirements surface as `failed to parse ...: missing field` at load time. See
ADR-0005 Consequences for the full migration note.

## UAT-1: Missing `[recipe]` rejected at deserialize

**Rationale.** ADR-0005 moves recipe fields off `PrinterProfile` and into a required
nested `[recipe]` table on `ResinProfile`. Legacy resin TOMLs written before this
refactor have no `[recipe]` table; if they were silently accepted (e.g. via a
`#[serde(default)]` on `recipe`), the simulation would use a fabricated recipe
that bears no relation to the resin's chemistry — defeating the whole point of
the refactor. Recipe is REQUIRED, and the parse must fail loudly.

This is the inverse of the KB-150 thermal-threshold serde(default) pattern:
thermal thresholds have reasonable defaults (50 °C / 15 °C) that work across
most resins. Recipe has NO reasonable default per-resin (that's the refactor's
motivating observation).

```gherkin
Scenario: UAT-1 missing [recipe] table rejected at deserialize
  Given a pre-ADR-0005 resin TOML file containing all chemistry fields (name, penetration_depth_um, critical_energy_mj_cm2, tensile_strength_mpa, peel_adhesion_kpa, ref_lift_speed_mm_min, etc.) but NO [recipe] table
  When "toml::from_str::<ResinProfile>(&contents)" is called
  Then parse returns Err
  And the error message names the missing field ("recipe")
  And validate() is NEVER reached — the parse layer is the gate
```

## UAT-2: `[recipe]` with NaN field rejected at validate()

**Rationale.** A `[recipe]` table that parses successfully but contains a NaN
field (e.g. `normal_exposure_sec = nan`) must fail `validate()`. Locks the
parse-then-validate loop per `docs/patterns/nan-two-layer-defence.md`: the
parse layer is permissive; the validate layer is the invariant gate.

```gherkin
Scenario: UAT-2 [recipe] with NaN field rejected at validate()
  Given a resin TOML file with a full [recipe] table but with recipe.normal_exposure_sec = nan
  When the file is deserialised into ResinProfile then validate() is called
  Then deserialize succeeds (serde accepts NaN as a valid f32)
  And validate() returns Err
  And the error message prefix is "recipe:" (because ResinProfile::validate() delegates to Recipe::validate() and tags the error)
  And the error names "normal_exposure_sec" as the offending field
```
