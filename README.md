# resinsim

**BLUF: resinsim simulates the physics of a resin 3D print (peel forces, suction, cure depth, thermals) from a sliced CTB or NanoDLP file, so a slicer developer can answer "is this job safe to print, and if not which layer breaks first and why" — without burning resin.**

Supported inputs: `.ctb`, `.nanodlp` (Athena II), `.stl`. A `.nanodlp` job also
carries the real Athena force log, so `resinsim inspect calibrate --file
job.nanodlp` reconciles the simulated peel force against what the printer
actually measured (see [ADR-0021](docs/adr/0021-nanodlp-import-and-calibration.md)).

## What's here

Rust workspace with three crates:

- `crates/resinsim-core` — the physics simulation library (layer-by-layer forces, cure, temperature).
- `crates/resinsim-inspect` — `resinsim` CLI for inspecting simulation domains and generating reports.
- `crates/resinsim-viz` — `resinsim-viz`, a Grafana-style pre-flight dashboard: time-series per layer, per-layer stats, geometry to localise flagged layers. See [PRODUCT.md](PRODUCT.md) and [DESIGN.md](DESIGN.md).

Supporting material:

- `data/` — printer profiles (Athena, Elegoo), calibrated resin profiles, test-cube STL/CTB fixtures.
- `docs/adr/`, `docs/kb/`, `docs/patterns/` — decisions, knowledge base, and (anti-)patterns.
- `spec/`, `schemas/`, `tests/` — behaviour specs, data schemas, integration tests.

## Quick start

```sh
cargo build --workspace
cargo run -p resinsim-inspect -- --help   # CLI: inspect / report
cargo run -p resinsim-viz                 # dashboard
cargo nextest run                         # tests
```

## Conventions

- `unwrap()` is denied workspace-wide (docs/adr/0003-unwrap-policy.md).
- This directory is its own jj repo — commit here, not from the parent repo.
