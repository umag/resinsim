---
issue: nanodlp-import
date: 2026-07-05
---

# UAT: Athena analytic force-log ingest (`inspect athena`)

## Rationale

`nanodlp-import` replaced the unusable wide `ForceRecord` schema with
`AnalyticLog`, which parses the real tall `ID,T,V` export (`.csv` or `.csv.gz`).
The T=6 pressure channel is the FSS peel signal, sign-corrected so peel reads
positive. No prior UAT exercised the real schema.

## UAT-1: tall analytic CSV parses and reports per-layer force

```gherkin
Scenario: UAT-1 inspect athena summarises real force data
  Given an Athena analytic log in tall "ID,T,V" form (gzip or plain)
  When the user runs `resinsim inspect athena --file <log.csv.gz>`
  Then stdout reports the number of layers with force data
  And stdout reports a peak peel signal in raw load-cell counts
  And stdout labels the values "not Newtons"
```

## UAT-2: malformed rows are rejected

```gherkin
Scenario: UAT-2 a non-numeric value row is a hard error
  Given an analytic CSV containing a row with a non-numeric V field
  When the user runs `resinsim inspect athena --file <log.csv>`
  Then the command exits non-zero with an actionable parse error naming the row
```
