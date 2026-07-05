---
issue: nanodlp-import
date: 2026-07-05
---

# UAT: `resinsim sim --file <job.nanodlp>` simulates a real Athena job

## Rationale

New input format introduced by `nanodlp-import`. NanoDLP jobs now route through
the same `sliced::parse_sliced` dispatcher as CTB, so `sim` must accept a
`.nanodlp` and produce a full per-layer simulation. Covers the import + PNG-mask
decode + recipe-mapping path end to end.

## UAT-1: sim accepts a .nanodlp and writes a per-layer sim.json

```gherkin
Scenario: UAT-1 simulate a NanoDLP job
  Given a .nanodlp job with M slice PNGs and NanoDLP profile/slicer/plate JSON
  When the user invokes `resinsim sim --file <job.nanodlp> --out out.sim.json`
  Then stderr reports "Producing sim.json from <job>"
  And a sim.json is written with M per-layer results
  And each layer result carries a peel_force_n and a cross_section_area_mm2
  And the reported layer count equals the NanoDLP LayersCount
```

## UAT-2: NanoDLP recipe maps profile.json exposure and speeds

```gherkin
Scenario: UAT-2 bottom layers use support exposure
  Given a .nanodlp whose profile.json sets SupportLayerNumber = K
  When the job is imported
  Then the first K layers use the support (bottom) exposure time
  And subsequent layers use the normal cure time
```
