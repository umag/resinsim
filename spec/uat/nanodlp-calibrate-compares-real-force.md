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
fit-quality signal — the property that KB-115's finding depends on. Since
ADR-0022 Stage 0 the comparison grades *total* separation force (peel + suction
+ base), not adhesion-only peel, and surfaces the predicted-vs-real peak layer.

## UAT-1: calibrate reports a comparison and suggested overrides

```gherkin
Scenario: UAT-1 calibrate reconciles predicted total force against the real log
  Given a .nanodlp job containing slice PNGs and an analytic-*.csv.gz force log
  When the user invokes `resinsim inspect calibrate --file <job.nanodlp>`
  Then stderr reports "Simulating <job>"
  And stdout reports "Compared N layers" where N equals the layer count
  And stdout reports a "Correlation (predicted total force vs real peel signal)" value in [-1, 1]
  And stdout reports a "Peak layer: predicted P, real A (offset ±D)" line
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

## UAT-3: the predicted-vs-real peak-layer offset is surfaced (KB-115)

```gherkin
Scenario: UAT-3 a large peak-layer offset is reported and hinted
  Given a .nanodlp whose real force peaks at layer 0 (base adhesion) while the
    area-driven sim peaks mid-print
  When the user runs `resinsim inspect calibrate --file <job.nanodlp>`
  Then stdout reports a "Peak layer:" line whose offset is non-zero
  And when the offset is large stdout emits a KB-115 / ADR-0022 hint line
  And `--json` output includes predicted_peak_layer, actual_peak_layer, and
    peak_layer_offset (null when a series is empty)
```
