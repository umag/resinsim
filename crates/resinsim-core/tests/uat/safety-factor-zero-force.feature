# Source: spec/uat/safety-factor-zero-force.md
# Spike: hand-copied scenario text. See docs/adr/0008-bdd-uat-spike-notes.md
# for drift-detection caveats.

Feature: Safety factor zero-force boundary

  Scenario: Zero peel force does not trigger support overload failure
    Given a print with zero peel force on one or more layers (e.g. layer area = 0)
    When the failure predictor runs on those layers
    Then no SupportOverload failure event is emitted for those layers
    And the layer result safety_factor is recorded as Infinity
