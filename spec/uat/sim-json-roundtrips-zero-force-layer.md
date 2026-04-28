---
issue: 15-extract-resinsim-run
date: 2026-04-28
---

# UAT: sim.json round-trips zero-force layers (INFINITY safety_factor) via JSON null

## Rationale

Per ADR-0015 + UAT-1 of `safety-factor-zero-force.md`, zero-force layers
carry `safety_factor = f32::INFINITY` in the in-memory aggregate.

The pre-fix bug: JSON has no Infinity literal, so `serde_json` writes
`null` for non-finite floats. The default deserializer rejects `null →
f32` with `invalid type: null, expected f32`. Pre-ADR-0015 the producer
and consumer ran in-process (no JSON round-trip), so the asymmetry
never surfaced. ADR-0015's canonical-interchange split forces the round-
trip and exposed the bug on real CTBs (4492-layer Lilith Torso, Mars 5
Ultra — final layer at area=0, force=0, INFINITY safety factor).

The fix: a serde adapter (`f32_with_infinity` in
`crates/resinsim-core/src/entities/layer_result.rs`) maps INFINITY ↔ null
losslessly. The unit-level regression test
(`simulation_repo.rs::save_to_path_round_trips_infinity_safety_factor_via_null`)
covers the adapter. This UAT pins the pipeline-level contract end-to-end
through the CLI surface that originally crashed.

See also:
- `docs/patterns/anti/serde-json-non-finite-f32-null-coercion.md` — anti-
  pattern context
- `docs/patterns/null-as-sentinel-for-non-finite-float-serde.md` — fix
  template

## UAT-1: Producer writes JSON null for INFINITY safety_factor

```gherkin
Scenario: UAT-1 producer writes null for INFINITY safety_factor
  Given a CTB whose final layer has cross_section_area_mm2 = 0
  When the user invokes "resinsim sim --file <PATH> --resin <R> --printer <P> --out <OUT>"
  Then the process exits 0
  And jq '.simulation.layers[-1].safety_factor' on <OUT> emits the literal string "null"
  And jq '.simulation.layers[-1].total_force_n' on <OUT> emits "0"
```

## UAT-2: Consumer reads null safety_factor without crashing

```gherkin
Scenario: UAT-2 consumer reads null safety_factor without crashing
  Given a sim.json envelope where at least one layer has safety_factor: null
  When the user invokes "resinsim report health --in <PATH>"
  Then the process exits 0
  And stdout contains a "Min safety factor:" line
  And the rendered minimum reflects only finite-force layers
  (zero-force layers don't constrain the minimum)
```

## UAT-3: Consumer JSON mode preserves the round-trip semantics

```gherkin
Scenario: UAT-3 consumer json-mode produces a finite min_safety_factor
  Given a sim.json envelope where at least one layer has safety_factor: null
  When the user invokes "resinsim report health --in <PATH> --json"
  Then the JSON output's summary.min_safety_factor is a finite number > 0
  And the null-SF layers are excluded from the min(), not propagating null
```

## Empirical observation (Lilith Torso, 4492 layers)

With Mars 5 Ultra (Tilt) on the full Lilith Torso CTB:
- Layer count: 4492
- Layers with safety_factor=null on disk: 1 (the final layer, index 4491)
- That layer's `peel_force_n`, `suction_force_n`, `total_force_n`,
  `cross_section_area_mm2` are all 0.0
- The reported `min_safety_factor` (10.084 at layer 10) reflects only
  finite-force layers — exactly what the JSON null sentinel pattern
  guarantees.

The UAT-1 / UAT-2 / UAT-3 contracts above are what the e2e test demonstrates
on real production data.
