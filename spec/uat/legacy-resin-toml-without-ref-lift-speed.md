---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: Legacy resin TOML without `ref_lift_speed_mm_min` rejected at deserialize

## UAT-1: Missing `ref_lift_speed_mm_min` rejected at parse

**Rationale.** ADR-0005 §3 moved `ref_lift_speed_mm_min` from `PrinterProfile` (where it
was a printer-motion field) to `ResinProfile` (where it is chemistry metadata for
`peel_adhesion_kpa` — the speed at which adhesion was measured, per KB-112 + KB-114).
`ResinProfile.ref_lift_speed_mm_min` has no `#[serde(default)]`: a pre-ADR-0005 resin
TOML that predates the split will fail to deserialise.

This is deliberate loud-failure per ADR-0005 Consequences — the same discipline as the
required `[recipe]` table. Both migrations are documented in the sibling UAT
(`legacy-resin-toml-without-recipe.md`) but *that* UAT's scenarios focus on the `[recipe]`
case. This UAT locks the `ref_lift_speed_mm_min` case specifically so a future PR cannot
silently reintroduce `#[serde(default)]` on the chemistry side without a failing test.

**Scenario:**

Given a pre-ADR-0005 resin TOML containing:
  - `name`, `penetration_depth_um`, `critical_energy_mj_cm2`, `tensile_strength_mpa`,
    `peel_adhesion_kpa`, `viscosity_mpa_s`, `reference_temp_c`,
    `activation_energy_kj_mol`, `density_g_cm3`, `linear_shrinkage_pct`
  - a valid `[recipe]` table (isolating the `ref_lift_speed_mm_min` failure mode)
  - NO `ref_lift_speed_mm_min` field
When `toml::from_str::<ResinProfile>(&contents)` is called
Then parse returns `Err`
  And the error message names the missing `ref_lift_speed_mm_min` field
  And `validate()` is never reached (parse is the gate)

## UAT-2: Migration patch adds `ref_lift_speed_mm_min = 60.0` and the same TOML then loads

**Rationale.** The ADR-0005 migration guidance (in this UAT's sibling
`legacy-resin-toml-without-recipe.md` and in the ADR Consequences) prescribes adding
`ref_lift_speed_mm_min = 60.0` as a safe default when the original measurement speed is
unknown. This scenario confirms the migration path works — a pre-ADR-0005 TOML plus the
single-line migration patch produces a valid `ResinProfile`.

**Scenario:**

Given the same pre-ADR-0005 resin TOML from UAT-1
When a user appends `ref_lift_speed_mm_min = 60.0` to the chemistry section
  (the industry-standard default per KB-112)
And `toml::from_str::<ResinProfile>(&contents)` is called
Then parse returns `Ok(profile)`
  And `profile.validate()` returns `Ok`
  And `profile.ref_lift_speed_mm_min() == 60.0`
