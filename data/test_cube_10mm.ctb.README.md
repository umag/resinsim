---
issue: print-time-on-reportgenerator
date: 2026-04-24
---

# `test_cube_10mm.ctb` — optional sliced fixture

This file documents the provenance of a CTB fixture used by the optional
`report_health_sliced_ctb_json_shape` test in
`crates/resinsim-inspect/tests/report_health_time_cli.rs`.

## Status

**Not yet committed.** The CLI smoke test that consumes it is gated behind
the `RESINSIM_SLICED_FIXTURE` env var (matching the existing
`RESINSIM_EXTERNAL_CTB_FIXTURE` convention in
`crates/resinsim-core/tests/suction_detector_integration.rs`). Until a
fixture is committed, run the test locally with:

```sh
RESINSIM_SLICED_FIXTURE=/path/to/cube.ctb \
  cargo nextest run --run-ignored=all report_health_sliced_ctb
```

Unconditional numeric coverage for `SimSummary` time fields lives in
`crates/resinsim-core/tests/sim_summary_time_integration.rs`, which
exercises `SimulationRunner::run_from_layer_inputs` with synthesised
`LayerInput` stacks and asserts equality against
`LayerTimingCalculator::cumulative_times_sec`. The CLI smoke test is
additional shape-level coverage, not primary correctness coverage.

## When committed, the fixture should be

- A 10mm solid cube (or similar small shape), ~500 layers at 20µm layer
  height — big enough for three-phase coverage (bottom / transition /
  normal), small enough not to bloat the repo (~1–2 MB on the wire).
- Sliced with `elegoo_mars5_ultra` printer configuration and
  `elegoo_ceramic_grey_v2` recipe defaults (bottom_layer_count = 6,
  bottom_exposure_sec per the TOML, normal_exposure_sec per the TOML).
- Produced by the maintainer's Mars 5 Ultra slicer pipeline — no in-tree
  CTB writer exists. The `resinsim-core/src/io/ctb.rs` module is
  parse-only.

## Why not synthesise

CTB is an encrypted format with AES + XOR obfuscation. A test-only writer
would be a separate effort comparable in size to the whole print-time-on-
reportgenerator issue. The env-var gate is the pragmatic interim —
identical to the pattern already in use for `RESINSIM_EXTERNAL_CTB_FIXTURE`.
