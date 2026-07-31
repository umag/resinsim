# Athena II session data — reserved, currently empty

This directory is reserved for **real** Phase 3 Athena II force-sensor
session data, in the layout fixed by `spec/EXPERIMENT-PLAN-v1.1.md` section 6
(`data pipeline`):

```
data/athena/
  <session-id>/
    manifest.yaml          # environment, printer sn, operator, resin, FEP
    E1/patches.csv
    E2/<print-id>/force.csv + print.json
    ...
    checksums.sha256       # SHA256 of every file in this session
  reports/
    <experiment>-<date>.md
    <experiment>-<date>.json
```

Session ID = `YYYY-MM-DD-<printer-sn>-<session-seq>` (e.g.
`2026-05-02-A2-0007-01`). Raw session files are immutable (`chmod 444`) once
captured — see the spec for the full manifest schema and ingest contract.

**Not** to be confused with `data/elegoo/` (a different printer brand's
telemetry) — see `data/elegoo/README.md`.

## No test fixtures here

This directory holds **no test fixtures**. Every committed test fixture in
this repo lives under `crates/*/tests/fixtures/`; the synthetic Athena
analytic-log fixture used by the property and round-trip tests is at
`crates/resinsim-core/tests/fixtures/synthetic_stepped_forces.csv` — reserved
for real E-series campaign data, not synthetic test inputs.

## Status

Empty. No E-series session has been captured yet (ADR-0022's "S4 — E-series"
stage is deferred pending an E2b campaign). This README's only job right now
is to make the directory exist in a fresh clone — previously it was entirely
untracked.
