---
issue: t1f2
date: 2026-04-17
---

# UAT: Safety factor zero-force boundary

## UAT-1: Zero peel force does not trigger support overload failure

**Rationale.** T1-F2 changed `SafetyFactor::compute()` to return `None` for zero
force. The guard in `failure_predictor` uses `map_or(false, ...)` — `None` must
produce no `SupportOverload` event.

**Scenario:**

Given a print with zero peel force on one or more layers (e.g. layer area = 0)
When the failure predictor runs on those layers
Then no SupportOverload failure event is emitted for those layers
And the layer result safety_factor is recorded as Infinity
