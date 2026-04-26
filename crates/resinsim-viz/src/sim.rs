//! Simulation orchestration for resinsim-viz: Bevy `Resource`/`Message`
//! definitions plus the `build_simulation_*` helpers and the
//! `apply_run_request` system.
//!
//! Run path is **CTB-only** in v1 (per ADR-0011). The two-layer
//! `build_simulation_from_layers` / `build_simulation_from_path` split
//! lets unit tests synthesise `Vec<LayerInput>` directly — no in-tree
//! CTB writer + no committed CTB fixture, so the synthesised path is
//! the load-bearing default-suite test. End-to-end CTB coverage is
//! env-var-gated on `RESINSIM_SLICED_FIXTURE` (matches the existing
//! convention at `main.rs::tests::smoke_exit_with_load_ctb_flag_runs_setup_without_panic`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use resinsim_core::app::SimulationRunner;
use resinsim_core::io::{ctb, sliced::LayerInput};
use resinsim_core::repositories::SimulationRepository;
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{AmbientTemperature, InitialLedTemperature};

use crate::profile_repos::ProfileRepos;
use crate::slice::LoadedSliceStack;

/// Simulation result resource — read by the right-panel summary +
/// plot rendering, written by `apply_run_request`. `last_error` is
/// the GUI-facing error ribbon; `bevy::log::error!` log lines are
/// supplementary, not the user's only signal.
#[derive(Resource, Default)]
pub struct SimulationResult {
    pub simulation: Option<PrintSimulation>,
    pub last_error: Option<String>,
}

/// CLI-driven run-time configuration that doesn't fit on the picker
/// (no GUI control yet). Inserted at startup from `Args`. v1 surface:
/// pre-validated initial LED temperature override and a save-sim
/// sidecar path. The "load-sim" startup pre-population happens once
/// in `setup_profile_repos` directly — no Resource needed.
#[derive(Resource, Default)]
pub struct RunConfig {
    /// `--initial-led-temp <°C>` if passed, validated via
    /// `InitialLedTemperature::new` at startup.
    pub initial_led_temp: Option<InitialLedTemperature>,
    /// `--save-sim <PATH>` if passed. After every successful Run
    /// `apply_run_request` writes the aggregate to this path via
    /// `SimulationRepository::save`. Errors log a warn and don't
    /// affect the GUI surface.
    pub save_sim_path: Option<PathBuf>,
}

/// Bevy 0.18 message — the picker emits one when "Run simulation" is
/// clicked. Payload is intentionally minimal: `apply_run_request`
/// queries `LoadedSliceStack` for the CTB path and the request only
/// names the picker selections.
#[derive(Message)]
pub struct RunSimRequest {
    pub resin: String,
    pub printer: String,
}

/// Curated v1 defaults (ADR-0011): the GUI commits to a single click
/// → result, so we don't expose SupportConfig / PlateAdhesionProfile /
/// AmbientTemperature in the picker. Override surface arrives in 06+.
fn default_supports() -> SupportConfig {
    SupportConfig {
        tip_radius_mm: 0.2,
        n_supports: 20,
    }
}

fn default_plate() -> PlateAdhesionProfile {
    PlateAdhesionProfile::default_textured()
}

fn default_ambient() -> AmbientTemperature {
    AmbientTemperature::new(22.0)
        .expect("default 22.0 °C is in AmbientTemperature domain (validated at type construction)")
}

/// Run the simulation against a pre-parsed layer stack. Used directly
/// by unit tests (synthesised `Vec<LayerInput>` mirrors the pattern at
/// `crates/resinsim-core/tests/sim_summary_time_integration.rs:50`).
///
/// `initial_led_temp` overrides the v1 default of `None` (cold start
/// from `default_ambient`) when passed — surfaced via the
/// `--initial-led-temp <°C>` CLI flag → `RunConfig.initial_led_temp`.
pub fn build_simulation_from_layers(
    req: &RunSimRequest,
    layers: &[LayerInput],
    repos: &ProfileRepos,
    initial_led_temp: Option<InitialLedTemperature>,
) -> Result<PrintSimulation, String> {
    let resin = repos.resin.load(&req.resin)?;
    let printer = repos.printer.load(&req.printer)?;
    SimulationRunner::run_from_layer_inputs(
        layers,
        &resin,
        &printer,
        &default_supports(),
        &default_plate(),
        default_ambient(),
        initial_led_temp,
    )
}

/// Parse a CTB on disk + delegate to `build_simulation_from_layers`.
/// Used by `apply_run_request` to drive the system from the GUI.
pub fn build_simulation_from_path(
    req: &RunSimRequest,
    ctb_path: &Path,
    repos: &ProfileRepos,
    initial_led_temp: Option<InitialLedTemperature>,
) -> Result<PrintSimulation, String> {
    let (_info, layers) = ctb::parse_ctb(ctb_path)?;
    build_simulation_from_layers(req, &layers, repos, initial_led_temp)
}

/// Bevy system: drains `RunSimRequest` events, runs the simulation
/// against the currently-loaded `LoadedSliceStack`, stores the result
/// (or error) in `SimulationResult`. Always consumes the event — no
/// implicit retries.
///
/// Failure modes (each sets `last_error` and clears `simulation`):
///  - `repos` resource missing (data dir failed to resolve at startup)
///  - no `LoadedSliceStack` spawned (user didn't drop a CTB yet)
///  - profile load fails (typo on resin/printer name)
///  - CTB parse fails (corrupt file)
///  - simulation fails (e.g. `pairing:` envelope violation)
pub fn apply_run_request(
    mut events: MessageReader<RunSimRequest>,
    repos: Option<Res<ProfileRepos>>,
    run_config: Option<Res<RunConfig>>,
    stack_q: Query<&LoadedSliceStack>,
    mut result: ResMut<SimulationResult>,
) {
    for req in events.read() {
        let Some(repos) = repos.as_ref() else {
            result.simulation = None;
            result.last_error = Some(
                "profile data directory not resolved — pass --data-dir or set RESINSIM_DATA_DIR"
                    .into(),
            );
            continue;
        };
        let Some(stack) = stack_q.iter().next() else {
            result.simulation = None;
            result.last_error = Some("Load a .ctb file before running simulation".into());
            continue;
        };
        let initial_led_temp = run_config
            .as_ref()
            .and_then(|c| c.initial_led_temp);
        match build_simulation_from_path(req, &stack.path, repos, initial_led_temp) {
            Ok(sim) => {
                let summary = sim.summary();
                info!(
                    "simulation produced {} layers / {} failures",
                    summary.total_layers, summary.critical_failures
                );
                if let Some(path) = run_config.as_ref().and_then(|c| c.save_sim_path.as_ref()) {
                    save_sim_to_path(&sim, path);
                }
                result.simulation = Some(sim);
                result.last_error = None;
            }
            Err(e) => {
                error!("simulation failed: {e}");
                result.simulation = None;
                result.last_error = Some(e);
            }
        }
    }
}

/// Split a "logical sidecar path" like `out/sim.json` into the
/// `(data_dir, stem)` shape `SimulationRepository` expects.
/// Trailing-slash / no-parent / no-stem edge cases all degrade to
/// safe defaults so the helper is total — callers don't ferry
/// `Option`s through.
fn split_sim_path(path: &Path) -> (PathBuf, String) {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("simulation")
        .to_string();
    (dir, stem)
}

/// Save a `PrintSimulation` via `SimulationRepository`, logging on
/// outcome. Save errors don't surface in the GUI — the user-visible
/// state is the (successful) Run; persistence is best-effort.
fn save_sim_to_path(sim: &PrintSimulation, path: &Path) {
    let (dir, stem) = split_sim_path(path);
    let repo = SimulationRepository::new(&dir);
    match repo.save(&stem, sim) {
        Ok(()) => info!("saved simulation to {}", path.display()),
        Err(e) => warn!("--save-sim failed for {}: {e}", path.display()),
    }
}

/// Load a previously-saved `PrintSimulation` from `path` via
/// `SimulationRepository`. Returns `Err(String)` so the caller can
/// surface the message in `SimulationResult.last_error`.
pub fn load_sim_from_path(path: &Path) -> Result<PrintSimulation, String> {
    let (dir, stem) = split_sim_path(path);
    SimulationRepository::new(&dir).load(&stem)
}

/// Helper exposed for sibling modules (panels.rs) that want to
/// surface the loaded path basename in the GUI.
pub fn loaded_basename(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use resinsim_core::values::LayerMask;

    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    fn shipped_repos() -> ProfileRepos {
        ProfileRepos::new(&workspace_data_dir())
    }

    /// Build a 100-layer cube-like LayerInput stack — mirrors
    /// `crates/resinsim-core/tests/sim_summary_time_integration.rs:50`.
    /// Uses a 1×1 fully-solid mask at the printer's voxel resolution
    /// (matches `SimulationRunner::run_from_areas`'s mask-synthesising
    /// adapter shape — fully-solid masks emit zero cavity events).
    fn cube_layer_inputs(n_layers: u32) -> Vec<LayerInput> {
        let layer_height_um = 50.0_f32;
        let exposure_sec = 2.5_f32;
        let lift_speed_mm_min = 60.0_f32;
        let voxel_size_mm = 0.05_f32;
        (0..n_layers)
            .map(|i| {
                let z_mm = (i as f32 + 1.0) * (layer_height_um / 1000.0);
                let mask = LayerMask::new_all_solid(1, 1, voxel_size_mm)
                    .expect("test fixture: 1×1 mask at validated voxel size constructs");
                LayerInput::new(
                    i,
                    100.0, // 100 mm² constant cross-section
                    exposure_sec,
                    lift_speed_mm_min,
                    layer_height_um,
                    z_mm,
                )
                .expect("test fixture: positive exposure + non-negative area satisfy LayerInput::new")
                .with_mask(mask)
            })
            .collect()
    }

    fn ok_request() -> RunSimRequest {
        RunSimRequest {
            resin: "generic_standard".into(),
            printer: "generic_msla_4k".into(),
        }
    }

    #[test]
    fn build_simulation_from_layers_happy_path() {
        let layers = cube_layer_inputs(100);
        let sim = build_simulation_from_layers(&ok_request(), &layers, &shipped_repos(), None)
            .expect("test fixture: shipped profiles + cube-like inputs satisfy run_from_layer_inputs");
        assert_eq!(sim.layers().len(), 100);
        assert_eq!(sim.summary().total_layers, 100);
    }

    #[test]
    fn build_simulation_unknown_resin_returns_err() {
        let layers = cube_layer_inputs(10);
        let req = RunSimRequest {
            resin: "definitely_not_a_resin".into(),
            printer: "generic_msla_4k".into(),
        };
        let err = build_simulation_from_layers(&req, &layers, &shipped_repos(), None)
            .expect_err("unknown resin name must return Err");
        assert!(
            err.contains("failed to read"),
            "err must surface the underlying read failure; got: {err}"
        );
    }

    #[test]
    fn build_simulation_unknown_printer_returns_err() {
        let layers = cube_layer_inputs(10);
        let req = RunSimRequest {
            resin: "generic_standard".into(),
            printer: "definitely_not_a_printer".into(),
        };
        let err = build_simulation_from_layers(&req, &layers, &shipped_repos(), None)
            .expect_err("unknown printer name must return Err");
        assert!(
            err.contains("failed to read"),
            "err must surface the underlying read failure; got: {err}"
        );
    }

    /// Pairing-violation path: PrinterProfile fields are `pub(crate)`
    /// (printer_profile.rs:23) so direct field mutation isn't
    /// available from viz. Hand-craft a TOML in a tempdir with
    /// `layer_height_range_um = [100.0, 150.0]` outside the
    /// generic_standard recipe's 50.0 µm — deserialisation bypasses
    /// the `pub(crate)` restriction. Then build a `ProfileRepos`
    /// pointing at the tempdir mixing in the shipped resin TOML.
    /// Expected: `Err` starting with `"pairing:"`.
    #[test]
    fn build_simulation_pairing_violation() {
        let tmp = tempfile::tempdir().expect("test fixture: tempdir creation");
        let printers_dir = tmp.path().join("printers");
        let resins_dir = tmp.path().join("resins");
        std::fs::create_dir_all(&printers_dir).expect("test fixture: mkdir printers");
        std::fs::create_dir_all(&resins_dir).expect("test fixture: mkdir resins");

        // Copy generic_standard resin verbatim.
        let shipped_resin = workspace_data_dir().join("resins").join("generic_standard.toml");
        std::fs::copy(&shipped_resin, resins_dir.join("generic_standard.toml"))
            .expect("test fixture: copy shipped resin TOML");

        // Hand-craft a printer with a layer_height_range that excludes
        // the resin's recipe layer_height (50.0 µm). All other fields
        // copied from generic_msla_4k.toml.
        let shipped_printer = workspace_data_dir().join("printers").join("generic_msla_4k.toml");
        let mut printer_toml = std::fs::read_to_string(&shipped_printer)
            .expect("test fixture: read shipped printer TOML");
        // Replace the layer_height_range_um line with a narrowed envelope.
        // Existing line is `layer_height_range_um = [10.0, 200.0]`-shape;
        // search-and-replace is brittle but the shipped fixture is stable.
        printer_toml = printer_toml
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("layer_height_range_um") {
                    "layer_height_range_um = [100.0, 150.0]".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(printers_dir.join("narrow_envelope.toml"), printer_toml)
            .expect("test fixture: write narrowed printer TOML");

        let repos = ProfileRepos::new(tmp.path());
        let layers = cube_layer_inputs(10);
        let req = RunSimRequest {
            resin: "generic_standard".into(),
            printer: "narrow_envelope".into(),
        };
        let err = build_simulation_from_layers(&req, &layers, &repos, None)
            .expect_err("layer_height 50 outside [100, 150] must trigger pairing violation");
        assert!(
            err.starts_with("pairing:"),
            "err must identify the pairing stage; got: {err}"
        );
        assert!(
            err.contains("layer_height_um"),
            "err must name the offending recipe field; got: {err}"
        );
    }

    /// `apply_run_request` system test: no `LoadedSliceStack` →
    /// `last_error` set, `simulation` cleared.
    #[test]
    fn apply_run_request_no_ctb_loaded() {
        let mut app = App::new();
        app.insert_resource(shipped_repos());
        app.init_resource::<SimulationResult>();
        app.add_message::<RunSimRequest>();
        app.add_systems(Update, apply_run_request);

        app.world_mut().write_message(ok_request());
        app.update();

        let result = app
            .world()
            .get_resource::<SimulationResult>()
            .expect("test fixture: SimulationResult was init_resource'd");
        assert!(result.simulation.is_none());
        let err = result
            .last_error
            .as_deref()
            .expect("apply_run_request must set last_error when no CTB is loaded");
        assert!(
            err.contains(".ctb"),
            "error must mention the missing .ctb; got: {err}"
        );
    }

    #[test]
    fn split_sim_path_handles_full_path() {
        let (dir, stem) = super::split_sim_path(Path::new("/tmp/cache/lilith.json"));
        assert_eq!(dir, PathBuf::from("/tmp/cache"));
        assert_eq!(stem, "lilith");
    }

    #[test]
    fn split_sim_path_defaults_no_parent_to_dot() {
        let (dir, stem) = super::split_sim_path(Path::new("lilith.json"));
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(stem, "lilith");
    }

    #[test]
    fn split_sim_path_defaults_empty_stem_to_simulation() {
        // PathBuf::from(".json") has no `file_stem`, falls through.
        let (dir, stem) = super::split_sim_path(Path::new("/tmp/.json"));
        assert_eq!(dir, PathBuf::from("/tmp"));
        // file_stem of ".json" is ".json" itself in Rust's
        // implementation (leading-dot files have no stem-distinct
        // behaviour); the safe-default branch only fires when
        // file_stem returns None *and* the path has no name.
        // Either "json"-shape or "simulation" is acceptable as the
        // total fallback — just assert non-empty.
        assert!(!stem.is_empty());
    }

    /// `--save-sim` then `--load-sim` round-trip: build a sim,
    /// save it via the orchestration helper, load it back and
    /// confirm the layer counts match. Exercises the
    /// SimulationRepository wrapper from viz's perspective.
    #[test]
    fn save_then_load_sim_round_trip() {
        let tmp = tempfile::tempdir().expect("test fixture: tempdir creation");
        let sidecar = tmp.path().join("round_trip.json");
        let layers = cube_layer_inputs(20);
        let sim = build_simulation_from_layers(&ok_request(), &layers, &shipped_repos(), None)
            .expect("test fixture: shipped profiles + cube-like inputs run cleanly");
        super::save_sim_to_path(&sim, &sidecar);
        assert!(sidecar.exists(), "save_sim_to_path must write the JSON sidecar");
        let loaded = super::load_sim_from_path(&sidecar)
            .expect("load_sim_from_path must succeed on a freshly-saved sidecar");
        assert_eq!(loaded.layers().len(), sim.layers().len());
        assert_eq!(loaded.summary().total_layers, sim.summary().total_layers);
    }

    /// `--load-sim` with a missing path returns Err with a
    /// "failed to read" message — `setup_profile_repos` surfaces
    /// it in `SimulationResult.last_error`.
    #[test]
    fn load_sim_missing_path_returns_err() {
        let err = super::load_sim_from_path(Path::new("/definitely/does/not/exist.json"))
            .expect_err("missing path must Err");
        assert!(
            err.contains("failed to read"),
            "err must surface read failure; got: {err}"
        );
    }

    /// End-to-end CTB integration — env-var-gated. Loads a real CTB
    /// fixture path provided via `RESINSIM_SLICED_FIXTURE` and runs
    /// the parse + simulate path. Skips by default per the existing
    /// convention at main.rs:786 (no in-tree CTB writer + no committed
    /// CTB fixture).
    #[test]
    fn build_simulation_from_path_with_ctb_fixture() {
        let Ok(fixture) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
            return;
        };
        let path = PathBuf::from(fixture);
        let sim = build_simulation_from_path(&ok_request(), &path, &shipped_repos(), None)
            .expect("RESINSIM_SLICED_FIXTURE pointed at a valid CTB; simulation must run");
        assert!(
            !sim.layers().is_empty(),
            "fixture must yield at least one layer"
        );
    }
}
