# resinsim

**resinsim simulates the physics of a resin 3D print — peel forces, suction,
cure depth, thermals, shrinkage strain, and residual stress — from a sliced
build file, so you can answer "will this job print safely?" before burning
resin.** It produces a per-layer risk assessment with safety factors, failure
predictions, and calibration against real force-sensor data.

Supported inputs: `.ctb` (encrypted slicer format), `.nanodlp` (Athena II
with embedded force logs), `.stl` (geometry-only, no slicer metadata).

## Quick start

```sh
cargo build --workspace
cargo run -p resinsim-inspect -- sim \
    --file model.ctb --resin generic_standard \
    --printer generic_msla_4k --out model.sim.json
cargo run -p resinsim-inspect -- report health --in model.sim.json
cargo run -p resinsim-viz                       # pre-flight dashboard
cargo nextest run                               # test suite
```

The two-step pipeline (`sim` then `report health`) is the primary workflow.
`sim` produces a `sim.json` envelope; every downstream consumer reads that
envelope, not raw slicer files.

## Feature matrix

resinsim has three capability tiers, controlled by Cargo feature flags:

| Capability | T1 (default) | T2 (`field-sim`) | GPU (`gpu`) |
|---|:---:|:---:|:---:|
| Per-layer peel force | x | x | x |
| Suction / cavity detection | x | x | x |
| Z-axis deflection | x | x | x |
| Lumped thermal model | x | x | x |
| Safety factor / failure prediction | x | x | x |
| Print time breakdown | x | x | x |
| NanoDLP import + calibration | x | x | x |
| 3D voxel cure field | | x | x |
| Photoinitiator depletion | | x | x |
| Light crosstalk convolution | | x | x |
| Shrinkage strain / residual stress | | x | x |
| Spatial thermal diffusion (FTCS) | | x | x |
| Binary sidecar persistence | | x | x |
| wgpu compute shader dispatch | | | x |

Build with features:

```sh
cargo build --workspace --features resinsim-inspect/field-sim   # T2
cargo build --workspace --features resinsim-inspect/gpu         # GPU (implies T2)
```

## CLI reference

### `resinsim sim`

Produces a `sim.json` envelope from a sliced input.

Key flags: `--file <CTB|NANODLP>` or `--stl <STL>`, `--resin <name>`,
`--printer <name>`, `--out <path>`, `--voxel-cure-mm <mm>` (T2),
`--gpu` (GPU tier). Resin and printer names resolve via the
[ADR-0004](docs/adr/0004-cli-profile-loading.md) 4-stage data-dir chain.

### `resinsim report health`

Full print risk assessment from a `sim.json` envelope.

Key flags: `--in <sim.json>`, `--json`.

### `resinsim inspect <domain>`

Single-domain inspectors for interactive exploration:

| Subcommand | Purpose |
|---|---|
| `cure` | Cure depth from Beer-Lambert equation |
| `force` | Peel force for a given cross-section |
| `thermal` | Vat temperature and viscosity drift |
| `zaxis` | Z-axis deflection and effective layer height |
| `athena` | Query Athena II force sensor CSV data |
| `layers` | Per-layer data from a sliced file |
| `calibrate` | NanoDLP job simulation vs real force log |
| `field` | 2D slice through a Tier-2 voxel field (T2) |

Run `resinsim inspect <subcommand> --help` for per-domain flags.

## Data flow

```
 Inputs                   Simulation              Consumers
 ──────                   ──────────              ─────────
 .ctb / .nanodlp / .stl ─┐
                          ├─► resinsim sim ─┬─► sim.json ──────► report health
 printers/*.toml ─────────┘                │                  ► resinsim-viz
 resins/*.toml ───────────┘                └─► fields.bin     ► inspect field
                                               (T2 sidecar)
```

`sim.json` is the canonical interchange format
([ADR-0015](docs/adr/0015-sim-json-canonical-interchange.md)). When
`--voxel-cure-mm` is set, a paired `fields.bin` binary sidecar carries the
four voxel fields (cure, photoinitiator, strain, stress) plus the thermal
field ([ADR-0019](docs/adr/0019-voxel-field-on-disk-persistence.md)).

## Architecture

Rust workspace with three crates:

| Crate | Binary | Role |
|---|---|---|
| `resinsim-core` | (library) | Physics simulation: entities, value objects, domain services, repositories, I/O adapters |
| `resinsim-inspect` | `resinsim` | CLI for running simulations, generating reports, and inspecting domains |
| `resinsim-viz` | `resinsim-viz` | Bevy/egui pre-flight dashboard: time-series plots, per-layer stats, 3D geometry viewer |

Supporting directories:

- `data/` — printer profiles, calibrated resin profiles, test fixtures
- `docs/adr/` — architecture decision records (see below)
- `docs/kb/` — knowledge base entries (physics sources and references)
- `docs/patterns/` — implementation patterns and anti-patterns
- `spec/uat/` — acceptance test specifications
- `schemas/` — data schemas (sim.json zod definitions)

## Conventions

- `unwrap()` is denied workspace-wide
  ([ADR-0003](docs/adr/0003-unwrap-policy.md)). Use `.expect("justification")`
  where infallibility is provable.
- This directory is its own jj repo — commit here, not from the parent repo.

## ADR index

| # | Decision |
|---|---|
| [0001](docs/adr/0001-ddd-layer-dependency-rule.md) | Values layer must not import Entities |
| [0002](docs/adr/0002-option-not-sentinel-for-absent-values.md) | Use `Option<T>`, not sentinel values |
| [0003](docs/adr/0003-unwrap-policy.md) | Deny `unwrap_used` workspace-wide |
| [0004](docs/adr/0004-cli-profile-loading.md) | CLI profile loading — 4-stage data-dir chain |
| [0005](docs/adr/0005-three-axis-printer-resin-recipe.md) | PrinterProfile / ResinProfile / Recipe domain split |
| [0006](docs/adr/0006-ambient-boundary-policy-for-cavity-detection.md) | Ambient boundary policy for cavity detection |
| [0007](docs/adr/0007-led-and-vat-as-separate-temperatures.md) | LED and vat as separate coupled thermal surfaces |
| [0008](docs/adr/0008-bdd-uat-spike-notes.md) | BDD UAT runner with cucumber-rs |
| [0009](docs/adr/0009-repositories-vs-io-placement.md) | `repositories/` vs `io/` placement rule |
| [0010](docs/adr/0010-resinsim-viz-presentation-layer.md) | resinsim-viz is the presentation layer |
| [0011a](docs/adr/0011-egui-control-panels.md) | egui control panels for resinsim-viz |
| [0011b](docs/adr/0011-world-z-up-and-msla-orientation.md) | World Z-up coordinate system (MSLA orientation) |
| [0012](docs/adr/0012-printer-build-envelope-on-profile.md) | `build_envelope_mm` on PrinterProfile |
| [0013](docs/adr/0013-screenshot-exit-code-disjunction.md) | Screenshot exit-code propagation |
| [0014](docs/adr/0014-bevy-egui-retained-for-viewer-redesign.md) | bevy_egui retained for viz v2 redesign |
| [0015](docs/adr/0015-sim-json-canonical-interchange.md) | sim.json as canonical interchange format |
| [0016](docs/adr/0016-layer-timeline-chart-and-bottom-panel.md) | Layer timeline chart and bottom panel |
| [0017](docs/adr/0017-voxel-cure-field-and-photoinitiator-depletion.md) | 3D voxel cure field and photoinitiator depletion |
| [0018a](docs/adr/0018-light-crosstalk-3d-gaussian-convolution.md) | 3D light crosstalk via Gaussian convolution |
| [0018b](docs/adr/0018-shrinkage-strain-stress-accumulation.md) | Per-voxel shrinkage strain and residual stress |
| [0019](docs/adr/0019-voxel-field-on-disk-persistence.md) | Voxel field binary sidecar persistence |
| [0020](docs/adr/0020-spatial-thermal-diffusion.md) | Spatial thermal diffusion (FTCS solver) |
| [0021](docs/adr/0021-nanodlp-import-and-calibration.md) | NanoDLP import + Athena calibration |
| [0022](docs/adr/0022-peel-force-model-corrections-roadmap.md) | Peel-force model corrections roadmap |
| [0023](docs/adr/0023-field-inspector-read-side-contract.md) | Field inspector read-side contract |
| [0024](docs/adr/0024-second-uat-harness-in-resinsim-viz.md) | Second cucumber UAT harness in resinsim-viz |
| [0025](docs/adr/0025-gpu-acceleration-wgpu.md) | GPU acceleration via wgpu |
