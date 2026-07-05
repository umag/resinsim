---
issue: nanodlp-import
date: 2026-07-05
---

# UAT: `resinsim inspect calibrate` compares simulated vs real Athena force

## Rationale

Primary user-facing surface introduced by `nanodlp-import`: the full loop that
simulates a `.nanodlp` job and reconciles it against the real Athena force log
embedded in the same archive. No prior UAT covered it. Guards the contract that
calibration output is framed as *suggested* (never applied) and carries a
fit-quality signal — the property that KB-115's finding depends on.

## UAT-1: calibrate reports a comparison and suggested overrides

```gherkin
Scenario: UAT-1 calibrate reconciles predicted peel force against the real log
  Given a .nanodlp job containing slice PNGs and an analytic-*.csv.gz force log
  When the user invokes `resinsim inspect calibrate --file <job.nanodlp>`
  Then stderr reports "Simulating <job>"
  And stdout reports "Compared N layers" where N equals the layer count
  And stdout reports a "Correlation (predicted vs real peel)" value in [-1, 1]
  And stdout reports a "peel gain" with a "fit R²" value
  And stdout labels the overrides "Suggested" and "NOT applied"
  And the printer profile file on disk is unchanged
```

## UAT-2: low fit quality is flagged, not hidden

```gherkin
Scenario: UAT-2 a single-print fit with poor R² warns the user
  Given a .nanodlp whose real force peaks at a different layer than the sim
  When the user runs `resinsim inspect calibrate --file <job.nanodlp>`
  Then stdout contains a low-fit-quality warning
  And the suggested gain is still reported for transparency
```
