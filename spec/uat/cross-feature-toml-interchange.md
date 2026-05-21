---
issue: t2f4-thermal-diffusion
date: 2026-05-21
---

# UAT: TOML profiles round-trip across the field-sim Cargo feature

## Rationale

Step 2 of t2f4 introduced the `Option<T> + serde(default) + validate-
time check under cfg(feature = "field-sim")` pattern for the new
thermal material fields. The load-bearing cross-cutting invariant is
**TOML interchange**: a TOML written or maintained by a binary built
WITH the `field-sim` feature must parse cleanly under a binary built
WITHOUT it, and vice versa. The seven in-repo profile TOMLs in
`data/printers/` + `data/resins/` are the canonical witnesses.

Today the invariant is verified implicitly by the cross-feature
test runs (`cargo nextest --workspace` + same with `--features
field-sim` both pass against the same TOMLs). A dedicated UAT
scenario protects against a future regression — e.g. adding
`#[serde(deny_unknown_fields)]` to `PrinterProfile` or `ResinProfile`
would break the interchange silently for some TOMLs.

## UAT-1: a field-sim-authored profile TOML loads + validates under default builds

```gherkin
Scenario: a profile TOML containing field-sim thermal fields loads cleanly
          under a default-feature binary
  Given a printer TOML containing:
        * top-level scalars including `convective_wall_h_w_m2k`,
          `vat_wall_thickness_mm`, `vat_wall_k_w_mk`
        * a `[build_envelope_mm]` table
  And a resin TOML containing top-level scalars including
       `thermal_conductivity_w_mk`, `specific_heat_j_kgk`,
       `convective_top_h_w_m2k`
  When the files are loaded by a binary BUILT WITHOUT the field-sim
       feature
  Then `toml::from_str` deserialises both TOMLs without any
       UnknownField error
  And both `printer.validate()` and `resin.validate()` return `Ok`
       (the field-sim-gated thermal-field Option requirement does not
       fire under default builds)
  And the loaded profiles behave identically to the same profiles
       loaded by a field-sim-feature binary (apart from the extra
       voxel/thermal code paths only the field-sim binary executes)
```

## UAT-2: a profile TOML missing the field-sim required fields is rejected under field-sim

```gherkin
Scenario: a profile TOML missing thermal_conductivity_w_mk fails validate()
          under the field-sim feature
  Given a resin TOML that has been authored under default builds
        (i.e. without the new thermal fields)
  When the file is loaded by a binary BUILT WITH the field-sim feature
  Then `toml::from_str` succeeds (the absent fields deserialise to
       Option::None)
  And `resin.validate()` returns `Err` whose message names
       `thermal_conductivity_w_mk` and the gating feature
       (`field-sim` / ADR-0020)
  And the message includes the literature-midpoint hint
       ("~0.20 W/m·K for acrylate photopolymer") so the user can
       resolve the error immediately
```

## See also

- ADR-0020 §Consequences — the cross-feature interchange policy.
- `docs/patterns/required-under-feature-via-option-plus-validate.md`
  — the harvest pattern that documents this shape.
- Step 2 commit on `feat/t2f4-thermal-diffusion` for the canonical
  implementation.
