---
issue: t2f3.1-post-impl-calibration-followups
date: 2026-05-20
---

# UAT: Calibration disclosure caveat for partially-calibrated resins

**ADR-0018 §9 + §10 note.** `ResinProfile::has_calibrated_moduli()` is a
3-of-3 predicate over `youngs_modulus_mpa` (KB-163), `poissons_ratio`
(KB-163), and `shrinkage_anisotropy_z_ratio` (KB-164). The producer
(`FailurePredictor::predict_strain_failures`) MUST emit the uncalibrated-
moduli caveat in any `FailureEvent.message` whenever ANY of the three is
defaulted. t2f3.1 A1 widened the predicate from the original 2-of-2
(E + ν only); these UATs lock the new disclosure contract at the
user-facing level.

## UAT-1: z_ratio defaulted while E + ν explicit still fires the caveat

**Rationale.** Pre-t2f3.1 the predicate was 2-of-2 (E + ν) — a profile
with E + ν set but z_ratio defaulted was silently reported as
calibrated and the caveat was incorrectly omitted. Inline coverage:
`crates/resinsim-core/src/entities/resin_profile.rs` test
`has_calibrated_moduli_false_when_z_ratio_unset` (passes after A1;
would have failed before). This UAT raises the same contract to the
user-facing layer.

```gherkin
Scenario: z_ratio defaulted while E + ν explicit still fires the caveat
  Given a resin profile with youngs_modulus_mpa = 2000
    And poissons_ratio = 0.35
    And shrinkage_anisotropy_z_ratio is unset
  When the strain/stress pipeline emits a WarpingRisk event
  Then the FailureEvent.message contains "uncalibrated moduli"
    And the message cites both "KB-163" and "KB-164"
```

## UAT-2: All three moduli Some suppresses the caveat

**Rationale.** Positive control on the 3-of-3 predicate. Inline
coverage: failure_predictor.rs `message_no_caveat_for_calibrated_resin`
test against `ResinProfile::generic_standard()` (which ships with
E = 2000 MPa, ν = 0.35, z_ratio = 1.5).

```gherkin
Scenario: All three moduli explicit suppresses the caveat
  Given a resin profile with youngs_modulus_mpa = 2000
    And poissons_ratio = 0.35
    And shrinkage_anisotropy_z_ratio = 1.5
  When the strain/stress pipeline emits a WarpingRisk event
  Then the FailureEvent.message does NOT contain "uncalibrated moduli"
```

## UAT-3: Either-of-three-missing also fires the caveat

**Rationale.** Exhaustive single-axis-missing coverage at the
user-facing layer. The 3-of-3 predicate must report false on
EXACTLY one of E, ν, or z_ratio being None — not just z_ratio.
Inline coverage: resin_profile.rs `has_calibrated_moduli_requires_all_three`
test (renamed from `_requires_both` in t2f3.1 Phase 5 fold).

```gherkin
Scenario Outline: Any single missing modulus fires the caveat
  Given a resin profile where <field> is unset
    And the other two calibrated-moduli fields are explicit
  When the strain/stress pipeline emits a WarpingRisk event
  Then the FailureEvent.message contains "uncalibrated moduli"

  Examples:
    | field                          |
    | youngs_modulus_mpa             |
    | poissons_ratio                 |
    | shrinkage_anisotropy_z_ratio   |
```
