# Source: spec/uat/cure-depth-nan-guard.md
# Spike: hand-copied scenario text. See docs/adr/0008-bdd-uat-spike-notes.md
# for drift-detection caveats.

Feature: Cure depth NaN guard

  Scenario: Invalid critical energy is caught before cure depth calculation
    Given a resin profile with a critical energy Ec that is zero or non-finite
    When the Beer-Lambert cure depth calculator runs for a layer
    Then the system fails loudly with a clear diagnostic message referencing critical_energy
    And does NOT silently return a negative cure depth or a false is_sufficient result

  Scenario: NaN scale factor from uniformity does not propagate silently
    Given a print with a uniformity profile where the intensity factor computation produces a non-finite value
    When the uniformity-corrected cure depth is computed
    Then the system panics with a clear "scale factor must be positive and finite" message from Energy::scale
    And does NOT silently produce a NaN cure depth that is misinterpreted as undercure
