---
issue: t1f1
date: 2026-04-17
---

# UAT: Thermal degradation detection

**ADR-0015 note.** Thermal degradation events surface in the
`PrintSimulation.failures` collection. The producer/consumer split (issue 15)
preserves this: `resinsim sim` produces an envelope including any
`ThermalDegradation` failure events, and `resinsim report health --in`
renders them with their original severity and message. Pipeline change
does NOT alter the detection contract.

## UAT-1: Thermal degradation detection survives DDD refactor

**Rationale.** T1-F1 deleted `VatTemperature::is_degradation_risk` and moved the
call site to `ResinProfile::is_degradation_risk`. This scenario verifies the
end-to-end path (failure_predictor → ResinProfile) remains intact.

```gherkin
Scenario: UAT-1 thermal degradation detection survives DDD refactor
  Given a resin with a 50°C degradation threshold
  When a simulation runs with a vat temperature that rises above 50°C during printing
  Then the simulation output includes a thermal degradation warning event
  And the warning references the vat temperature that exceeded the threshold
```
