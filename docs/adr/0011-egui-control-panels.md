---
issue: 04-egui-control-panels
date: 2026-04-26
---

# ADR-0011: egui control panels (pickers + plots) for resinsim-viz

## Status
Accepted

## Context

Phase 2 step 4 of the simulation plan
(`projects/000-global/research/resinsim-physics-simulation-plan.md`)
introduces a side-panel UI for driving the simulation from inside the
viz app. ADR-0010 deferred `bevy_egui` until this issue. Issue 04
adds the picker controls AND (per Mag, 2026-04-26 redirect) the
time-series plots that issue 05 originally owned — fan out folded
forward so a single click yields a result *with* visualisation, not
just a Bevy `Resource` no one can see.

Mid-plan two additional pivots happened:
1. **Plots come first.** "Fold in plots — graphs of forces,
   temperature, and print progress vs time as a primary v1
   deliverable." (Mag, 2026-04-26)
2. **Run path is CTB-only.** "Skip STL, CTB is the main focus."
   (Mag, 2026-04-26) Existing STL drag-drop visualisation
   (issues 01–02) stays; STL Run is deferred. Single primary input
   format simplifies v1 UX, and the CTB pipeline already carries
   per-layer exposure + lift-speed metadata that the simulator
   needs.

## Decision

### Dependency version chain

`bevy_egui = "0.39"` and `egui_plot = "0.34"`. The chain is
`bevy_egui 0.39` → bundled `egui 0.33` → matched
`egui_plot 0.34`. ADR-0010 pre-stated `bevy_egui = "0.36"` before
bevy 0.18 had landed; 0.39.0 (2026-01-14) is the version that
ships with egui 0.33 and bevy 0.18. egui_plot 0.34 is the version
that targets egui 0.33 — `egui_plot 0.33` confusingly targets
egui 0.32 (off by one), the pin gets surfaced explicitly in the
viz `Cargo.toml` so future readers don't repeat the trap.

### Multipass mandatory; no `enable_multipass_for_primary_context`

bevy_egui 0.39 deprecated single-pass mode. We use
`EguiPlugin::default()` and register panel systems on the
`EguiPrimaryContextPass` schedule (per
`bevy_egui::lib.rs::doc-test:33`). Auto-attach to the first
`Camera3d` is fine — the existing `setup_scene` in main.rs
spawns one `Camera3d` carrying both `PanOrbitCamera` and
`AmbientLight` (per ADR-0010); egui mounts on it without disturbing
trackpad input.

### No `rfd` (file dialog) in v1

The Open STL / Open CTB button was deferred. Drag-drop is the sole
file affordance — `handle_dropped_files` (main.rs) already routes
both `.stl` (visualisation) and `.ctb` (Run target) drops through
`load_stl_into_world` / `load_ctb_into_world`. A future rfd-backed
dialog can land in a follow-up if user feedback warrants. Avoiding
`rfd` keeps the dep graph small and avoids macOS-only AppKit
linkage in v1.

### `data_dir::resolve_data_dir` duplicates resinsim-inspect

ADR-0010 forbids viz → inspect deps. The 4-stage chain
(flag → `RESINSIM_DATA_DIR` env → `$CWD/data` → exe-sibling
`data/`, per `docs/patterns/cli-data-dir-resolution-chain.md`) is
duplicated in `crates/resinsim-viz/src/data_dir.rs` rather than
shared via an upstream crate. Acceptable v1 — the chain is small,
documented, and won't drift silently because both call sites are
unit-tested. Future shared-crate consolidation
(`resinsim-shared-resolver`) is a follow-up if a third consumer
appears.

### `LoadedSliceStack` carries the path; `LoadedStlMesh` stays a marker

`LoadedSliceStack { pub path: PathBuf }` so `apply_run_request`
(sim.rs) knows which CTB to feed `ctb::parse_ctb`. The world
always reflects "what Run will consume." `LoadedStlMesh` stays a
unit-struct because STL Run is out of scope for v1; visualisation
queries are field-agnostic. Symmetric path-extension on
LoadedStlMesh is a follow-up if STL Run reopens.

### `ProfileRepos` Bevy `Resource` newtype

```rust
#[derive(Resource)]
pub struct ProfileRepos {
    pub resin: ResinProfileRepository,
    pub printer: PrinterProfileRepository,
}
```

Constructed at startup from the resolved data dir. Inserted only
on success. Systems hold `Option<Res<ProfileRepos>>` so a missing
data dir keeps the app running with empty pickers + a visible
error string in `SimulationResult.last_error`. Newtype keeps the
core repository types out of system signatures.

### Two-layer `build_simulation`

```rust
pub fn build_simulation_from_layers(
    req: &RunSimRequest,
    layers: &[LayerInput],
    repos: &ProfileRepos,
) -> Result<PrintSimulation, String>;

pub fn build_simulation_from_path(
    req: &RunSimRequest,
    ctb_path: &Path,
    repos: &ProfileRepos,
) -> Result<PrintSimulation, String>;
```

`_from_path` wraps `ctb::parse_ctb` then delegates to `_from_layers`.
The split lets unit tests drive `_from_layers` with a synthesised
`Vec<LayerInput>` (mirrors the pattern at
`crates/resinsim-core/tests/sim_summary_time_integration.rs:50`)
without needing a CTB on disk — the workspace ships no in-tree CTB
writer + no committed CTB fixture (verified 2026-04-26). End-to-end
CTB coverage is env-var-gated on `RESINSIM_SLICED_FIXTURE`,
matching the existing convention at
`main.rs::tests::smoke_exit_with_load_ctb_flag_runs_setup_without_panic`.

### Curated v1 simulation defaults

Recipe is `pub(crate)` per
`docs/patterns/entity-validate-on-mutation.md`; viz cannot patch
layer_height_um or normal_exposure_sec from outside the core
crate. v1 ships **read-only** labels in the left panel sourced
from the picked profile. Override surface for those fields is the
follow-up `core-recipe-overrides-api` issue.

For the orchestration-level inputs (`SupportConfig`,
`PlateAdhesionProfile`, `AmbientTemperature`, `InitialLedTemperature`)
the GUI commits to a single click → result. v1 hard-codes:

```rust
SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }
PlateAdhesionProfile::default_textured()
AmbientTemperature::new(22.0)?
initial_led_temp = None
```

Override surface for those arrives in 06+ via the right panel's
"Material editor" anchor stub.

### `PrintSimulation::cumulative_times_sec` accessor (core)

The plot panel needs a per-layer time axis. The existing
`LayerTimingCalculator::cumulative_times_sec` takes `(&Recipe,
&PrinterProfile, n_layers)` — viz holds neither directly because
`PrintSimulation` keeps `recipe`/`printer` private. Rather than
expose `recipe()` / `printer()` accessors (leaks aggregate
internals), this issue adds a single narrow accessor:

```rust
impl PrintSimulation {
    pub fn cumulative_times_sec(&self) -> Vec<f32> {
        LayerTimingCalculator::cumulative_times_sec(
            &self.recipe, &self.printer, self.layers.len() as u32,
        )
    }
}
```

Length always equals `self.layers().len()`, monotonic
non-decreasing, all-zero on empty. Encapsulation preserved. Tests
co-located in `print_simulation.rs::tests`.

### Plot panel design (right side)

Three stacked `egui_plot::Plot` widgets, each ~150 px tall, with a
linked x-axis group (`"resinsim-viz-time-axis"`) so zoom/pan on
one mirrors to the others. Locked axis labels + legend wording:

| Plot | x | y | Series |
|------|---|---|--------|
| Print progress | Time (s) | Z height (mm) | "Z height" |
| Forces | Time (s) | Force (N) | "Peel" / "Suction" / "Total" |
| Temperature | Time (s) | "Temperature (°C) / Viscosity (mPa·s)" | "Vat temp" / "Viscosity" |

Temperature uses a shared y-axis in v1 (different scales —
egui_plot auto-bounds handles it). If readability is poor in
practice, v1.1 can split into two stacked plots without contract
churn.

### `PlotData` projection

`crates/resinsim-viz/src/ui/plots.rs::build_plot_data(&PrintSimulation)
-> PlotData` is pure: zips `sim.layers()` with
`sim.cumulative_times_sec()`, accumulates layer heights from
`LayerResult.effective_layer_height_um`, casts everything to
`f64` for egui_plot. Pure-fn shape is the test seam per
`docs/patterns/bevy-app-test-seam.md` — egui rendering remains
out of scope for in-process tests.

Per-frame rebuild is acceptable v1 (vec-of-floats transform on
the order of layer count; egui_plot itself is the dominant cost).
If profiling shows hot path, add a `LastPlotData` cache resource
keyed on a sim version stamp; not pre-optimised here.

### Picker UX

- ComboBox empty hint: "(no profiles — set --data-dir)" — points
  the user at the resolution chain when the data dir is missing.
- "Loaded CTB" hint label shows the basename of the currently
  loaded `LoadedSliceStack.path`, or "(drag a .ctb file in to
  load)" when none.
- Status line above the Run button always reads either
  "Ready to run" or a concrete missing-items list
  (`run_block_reason`): "Drag in a .ctb file, pick a resin
  profile" — natural single-item phrasing, full list on 2+
  blockers.
- Error ribbon below the Run button renders
  `SimulationResult.last_error` in `Color32::LIGHT_RED`. Cleared
  on successful Run. `bevy::log::error!` lines stay supplementary,
  not the user's only signal.

### `PickerState` profile cache

The read-only "Layer height: N µm" / "Exposure: N s" labels need a
loaded `ResinProfile` at render time. Loading every frame would
mean per-frame disk I/O — wrong. PickerState caches
`loaded_resin: Option<ResinProfile>` and `loaded_printer:
Option<PrinterProfile>`. `refresh_loaded_profiles` is **idempotent**
(equal names → no mutation), preventing
`is_changed`/refresh ping-pong even though the system writes
through `&mut`. Test `refresh_loaded_profiles_is_idempotent` pins
this contract.

### Panel anchor contract for 05+

Layout anchors are part of the contract surface for downstream
issues:

- `SidePanel::left("controls")` — pickers + Run + status line +
  error ribbon. Issue 05+ extends this with view-mode switches
  (07) + override drag-values (`core-recipe-overrides-api`).
- `SidePanel::right("inspectors")` — summary + plots +
  "Material editor" stub (06) + "Failure list" stub (06). Issues
  05/06/07 drop content into stable headings without reshuffling.

### Smoke test contract

`smoke_exit_with_load_ctb_flag_runs_setup_without_panic` (env-gated
on `RESINSIM_SLICED_FIXTURE`) is the load-bearing CTB integration
check after the LoadedSliceStack shape change. Touching that
component requires re-running:

```sh
RESINSIM_SLICED_FIXTURE=… cargo nextest run -p resinsim-viz --run-ignored=all
```

`new_resources_and_systems_do_not_panic_on_one_update` (default
suite) covers the non-egui half of the wiring (Startup
`setup_profile_repos`, Update `apply_run_request` + cache refresh)
without loading EguiPlugin — egui rendering needs a render backend
that headless tests can't supply.

## Consequences

- **Compile-time enforcement of the layering rule preserved.** Viz
  still depends only on `resinsim-core`. The `cumulative_times_sec`
  accessor is the only core change in this issue, and is
  encapsulation-preserving.
- **CTB-only Run.** The picker's Run button is gated on
  `LoadedSliceStack` presence. Drop a `.stl` and the button stays
  disabled with status line "Drag in a .ctb file". This is
  intentional v1 scope; STL Run is a follow-up.
- **No CTB writer / fixture in tree.** Default-suite happy-path
  test for `build_simulation_*` uses synthesised
  `Vec<LayerInput>`. End-to-end CTB integration is env-var-gated.
- **Recipe-override surface deferred.** Drag-value inputs for
  layer_height_um / normal_exposure_sec require a public
  `with_overrides`-shape on Recipe or ResinProfile. v1 ships
  read-only labels; the override is `core-recipe-overrides-api`.
- **Plot performance budget — not measured in v1.** Per-frame
  `build_plot_data` is fine for current sim sizes (tens to a few
  thousand layers). Beyond ~10k layers a `LastPlotData` cache
  becomes worthwhile; not pre-built.
- **`unused_crate_dependencies` lint clean.** Both `bevy_egui` and
  `egui_plot` are wired through panels.rs / plots.rs so no
  unused-dep warning surfaces if the lint is enabled later.
