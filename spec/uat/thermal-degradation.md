---
issue: t1f1
date: 2026-04-17
---

# UAT: Thermal degradation detection

## UAT-1: Thermal degradation detection survives DDD refactor

**Rationale.** T1-F1 deleted `VatTemperature::is_degradation_risk` and moved the
call site to `ResinProfile::is_degradation_risk`. This scenario verifies the
end-to-end path (failure_predictor → ResinProfile) remains intact.

**Scenario:**

Given a resin with a 50°C degradation threshold
When a simulation runs with a vat temperature that rises above 50°C during printing
Then the simulation output includes a thermal degradation warning event
And the warning references the vat temperature that exceeded the threshold
