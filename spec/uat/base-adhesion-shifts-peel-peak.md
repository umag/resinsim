---
issue: peel-corrections-s1-base-adhesion
date: 2026-07-23
---

# UAT: first-layer base adhesion shifts the peel-force peak (KB-116 / ADR-0022 Stage 1)

## Rationale

ADR-0022 Stage 1 adds a first-layer base-adhesion term (KB-116 oxygen-freshness
σ-relaxation) as an opt-in resin parameter. It closes the KB-115 gap: the
area-driven sim peak used to sit mid-print while the real force peaks at layer 0.
These scenarios guard that the term is (a) active + visible when a resin opts in,
and (b) fully behaviour-preserving when it does not.

## UAT-1: an opting-in resin lifts and reveals the base term

```gherkin
Scenario: UAT-1 a resin with base_adhesion_elevation_kpa > 0 adds a first-layer term
  Given a resin whose base_adhesion_elevation_kpa is non-zero
  When a job is simulated with that resin
  Then the bottom layers report total_force_n greater than peel_force_n
  And each LayerResult carries a base_force_n that is largest at layer 0 and
    relaxes toward 0 over ~bottom_layer_count layers
  And `inspect calibrate` prints "Predicted base adhesion (layer 0): <N> N"
  And the predicted-vs-real peak-layer offset is smaller than without the term
```

## UAT-2: an unset resin is behaviour-preserving

```gherkin
Scenario: UAT-2 a resin without the parameter gets no base term
  Given a resin whose base_adhesion_elevation_kpa is unset (or 0)
  When a job is simulated with that resin
  Then every layer's base_force_n is 0.0
  And total_force_n equals peel_force_n + suction_force_n (no base contribution)
  And `inspect calibrate` prints no "Predicted base adhesion" line
```

## UAT-3: the KB-114 peel vectors are undisturbed

```gherkin
Scenario: UAT-3 the pure peel force is unchanged by the base term
  Given the KB-114 reference cases (σ, A, f(v))
  When peel_force is evaluated
  Then it returns the KB-114 Newton values unchanged
  # base adhesion is a SEPARATE PeelForceCalculator method, never folded into peel_force
```
