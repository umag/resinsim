//! Simulation orchestration for resinsim-viz: Bevy `Resource`/`Message`
//! definitions plus the `apply_run_request` system that delegates to
//! `resinsim_core::app::build_simulation_*` (the pure compute path).
//!
//! Run path is **CTB-only** in v1 (per ADR-0011). The build helpers
//! themselves live in `resinsim-core` so the CLI `resinsim sim`
//! subcommand can reach them without taking on Bevy. ADR-0010 layering
//! rule (presentation → core, one-way) is preserved trivially because
//! this file imports core but core does not import viz.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use resinsim_core::app::{build_simulation_from_path, RunRequest};
use resinsim_core::repositories::{load_from_path, save_to_path};
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::InitialLedTemperature;

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
/// sidecar path.
#[derive(Resource, Default)]
pub struct RunConfig {
    /// `--initial-led-temp <°C>` if passed, validated via
    /// `InitialLedTemperature::new` at startup.
    pub initial_led_temp: Option<InitialLedTemperature>,
    /// `--save-sim <PATH>` if passed. After every successful Run
    /// `apply_run_request` writes the aggregate via the canonical
    /// envelope (per ADR-0015) — errors log a warn and don't affect
    /// the GUI surface.
    pub save_sim_path: Option<PathBuf>,
}

/// Bevy 0.18 message — the picker emits one when "Run simulation" is
/// clicked. Carries only the picker selections (resin / printer
/// names); `apply_run_request` constructs the full
/// [`RunRequest`] (with v1-default support config + ambient,
/// initial_led from `RunConfig`) and delegates to core.
#[derive(Message)]
pub struct RunSimRequest {
    pub resin: String,
    pub printer: String,
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
        let initial_led_temp = run_config.as_ref().and_then(|c| c.initial_led_temp);
        let core_req = RunRequest::new_with_v1_defaults(&req.resin, &req.printer, initial_led_temp);
        // Access the inner pure resinsim_core::app::ProfileRepos via the
        // Resource newtype's tuple field. `&repos.0` reads as `auto-deref
        // Res → ProfileRepos, then field .0 → &CoreProfileRepos`. The
        // alternative `&**repos` (deref-twice) is technically cleaner but
        // less obviously about a newtype unwrap; field access here makes
        // the wrapper-vs-inner relationship explicit.
        match build_simulation_from_path(&core_req, &stack.path, &repos.0) {
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

/// Save a `PrintSimulation` to `path` via the canonical envelope. Save
/// errors don't surface in the GUI — the user-visible state is the
/// (successful) Run; persistence is best-effort.
fn save_sim_to_path(sim: &PrintSimulation, path: &Path) {
    match save_to_path(path, sim) {
        Ok(()) => info!("saved simulation to {}", path.display()),
        Err(e) => warn!("--save-sim failed for {}: {e}", path.display()),
    }
}

/// Load a previously-saved `PrintSimulation` from `path` via the
/// canonical envelope loader. Returns `Err(String)` so the caller can
/// surface the message in `SimulationResult.last_error`.
pub fn load_sim_from_path(path: &Path) -> Result<PrintSimulation, String> {
    load_from_path(path)
}

/// Helper exposed for sibling modules (panels.rs) that want to surface
/// the loaded path basename in the GUI.
pub fn loaded_basename(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    fn shipped_repos() -> ProfileRepos {
        ProfileRepos::new(&workspace_data_dir())
    }

    fn ok_request() -> RunSimRequest {
        RunSimRequest {
            resin: "generic_standard".into(),
            printer: "generic_msla_4k".into(),
        }
    }

    /// `apply_run_request` system test: no `LoadedSliceStack` →
    /// `last_error` set, `simulation` cleared. Pure compute paths
    /// (build_simulation_from_layers / from_path / pairing-violation)
    /// have moved to `resinsim-core::app::build_simulation` and are
    /// covered by tests there; this file only retains the Bevy-glue
    /// tests that need an `App`.
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

    /// `--save-sim` then `--load-sim` round-trip via the canonical
    /// envelope (ADR-0015). Builds a sim through core's pure path,
    /// saves it via the GUI-side helper, loads it back, and confirms
    /// the layer counts match. Exercises the envelope's atomic write
    /// + version check from viz's perspective.
    #[test]
    fn save_then_load_sim_round_trip() {
        let tmp = tempfile::tempdir().expect("test fixture: tempdir creation");
        let sidecar = tmp.path().join("round_trip.sim.json");
        // Build via core's pure path (no Bevy App needed).
        let layers = {
            use resinsim_core::values::LayerMask;
            let layer_height_um = 50.0_f32;
            let voxel_size_mm = 0.05_f32;
            (0..20u32)
                .map(|i| {
                    let z_mm = (i as f32 + 1.0) * (layer_height_um / 1000.0);
                    let mask = LayerMask::new_all_solid(1, 1, voxel_size_mm)
                        .expect("test fixture: 1×1 mask");
                    resinsim_core::io::sliced::LayerInput::new(
                        i,
                        100.0,
                        2.5,
                        60.0,
                        layer_height_um,
                        z_mm,
                    )
                    .expect("test fixture: positive exposure + non-negative area")
                    .with_mask(mask)
                })
                .collect::<Vec<_>>()
        };
        let req = RunRequest::new_with_v1_defaults("generic_standard", "generic_msla_4k", None);
        let sim =
            resinsim_core::app::build_simulation_from_layers(&req, &layers, &shipped_repos().0)
                .expect("test fixture: shipped profiles + cube-like inputs run cleanly");
        super::save_sim_to_path(&sim, &sidecar);
        assert!(
            sidecar.exists(),
            "save_sim_to_path must write the JSON sidecar"
        );
        let loaded = super::load_sim_from_path(&sidecar)
            .expect("load_sim_from_path must succeed on a freshly-saved sidecar");
        assert_eq!(loaded.layers().len(), sim.layers().len());
        assert_eq!(loaded.summary().total_layers, sim.summary().total_layers);
    }

    /// `--load-sim` with a missing path returns Err with a "failed to
    /// read" message — `setup_profile_repos` surfaces it in
    /// `SimulationResult.last_error`.
    #[test]
    fn load_sim_missing_path_returns_err() {
        let err = super::load_sim_from_path(Path::new("/definitely/does/not/exist.json"))
            .expect_err("missing path must Err");
        assert!(
            err.contains("failed to read"),
            "err must surface read failure; got: {err}"
        );
    }
}
