//! Pure simulation-build path. No Bevy dependency — `resinsim-viz` and the
//! `resinsim sim` CLI subcommand both call into here. Per ADR-0010 the
//! layering rule is one-way (presentation → core), so `build_simulation_*`
//! must not import any Bevy types. The viz `apply_run_request` system is a
//! thin Bevy adapter that constructs a [`RunRequest`] from picker state and
//! delegates to one of these functions.
//!
//! Run path is **CTB-only** in v1 (per ADR-0011). The two-layer
//! [`build_simulation_from_layers`] / [`build_simulation_from_path`] split
//! lets unit tests synthesise `Vec<LayerInput>` directly — no in-tree CTB
//! writer + no committed CTB fixture, so the synthesised path is the
//! load-bearing default-suite test. End-to-end CTB coverage is env-var
//! gated on `RESINSIM_SLICED_FIXTURE` at the call sites that exercise the
//! parse path.

use std::path::Path;

use crate::app::SimulationRunner;
use crate::io::{ctb, sliced::LayerInput};
use crate::repositories::{PrinterProfileRepository, ResinProfileRepository};
use crate::services::build_plate::PlateAdhesionProfile;
use crate::services::failure_predictor::SupportConfig;
use crate::simulation::PrintSimulation;
use crate::values::{AmbientTemperature, InitialLedTemperature};

/// Pure-data simulation request. No Bevy types — the GUI's
/// `#[derive(Message)] RunSimRequest` and the CLI's clap-derive args both
/// project into this shape before calling [`build_simulation_from_layers`]
/// or [`build_simulation_from_path`]. ADR-0010 layering rule: this struct
/// (and the `build_simulation_*` functions that consume it) has no
/// presentation-layer dependency.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub resin_name: String,
    pub printer_name: String,
    pub support_config: SupportConfig,
    pub ambient: AmbientTemperature,
    pub initial_led: Option<InitialLedTemperature>,
}

impl RunRequest {
    /// Construct a v1-defaults request. The GUI commits to a single
    /// click → result so it doesn't expose `support_config` /
    /// `ambient` in the picker; the CLI's `resinsim sim` accepts them
    /// as overridable args. Both call here for the baseline.
    ///
    /// Defaults match the previous viz hardcoded values (when the
    /// build path lived in `resinsim-viz/src/sim.rs`):
    ///
    /// - `support_config`: tip_radius_mm=0.2, n_supports=20
    /// - `ambient`: 22.0 °C
    /// - `initial_led`: None (cold start from ambient)
    ///
    /// The 22.0 °C ambient is in domain by construction; the `expect`
    /// is documented at the call site.
    pub fn new_with_v1_defaults(
        resin_name: impl Into<String>,
        printer_name: impl Into<String>,
        initial_led: Option<InitialLedTemperature>,
    ) -> Self {
        Self {
            resin_name: resin_name.into(),
            printer_name: printer_name.into(),
            support_config: SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 20,
            },
            ambient: AmbientTemperature::new(22.0).expect(
                "default 22.0 °C is in AmbientTemperature domain (validated at type construction)",
            ),
            initial_led,
        }
    }
}

/// Pure profile-repository bundle. Used by both the GUI run path
/// (wrapped in a Bevy `Resource` newtype on the viz side) and the CLI
/// `resinsim sim` body. Convention: `<data_dir>/resins/` and
/// `<data_dir>/printers/` — matches `resinsim-inspect` and the
/// workspace's shipped `data/` directory.
pub struct ProfileRepos {
    pub resin: ResinProfileRepository,
    pub printer: PrinterProfileRepository,
}

impl ProfileRepos {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            resin: ResinProfileRepository::new(&data_dir.join("resins")),
            printer: PrinterProfileRepository::new(&data_dir.join("printers")),
        }
    }
}

/// PlateAdhesionProfile is hard-defaulted to textured here (the v1
/// surface — neither the GUI picker nor the CLI exposes plate type yet).
/// Promoting it to a `RunRequest` field is left to a future issue.
fn default_plate() -> PlateAdhesionProfile {
    PlateAdhesionProfile::default_textured()
}

/// Run the simulation against a pre-parsed layer stack. Used directly by
/// unit tests (synthesised `Vec<LayerInput>` mirrors the pattern at
/// `crates/resinsim-core/tests/sim_summary_time_integration.rs:50`) and
/// by the CLI when the input is already-decoded layers (rare).
pub fn build_simulation_from_layers(
    req: &RunRequest,
    layers: &[LayerInput],
    repos: &ProfileRepos,
) -> Result<PrintSimulation, String> {
    let resin = repos.resin.load(&req.resin_name)?;
    let printer = repos.printer.load(&req.printer_name)?;
    SimulationRunner::run_from_layer_inputs(
        layers,
        &resin,
        &printer,
        &req.support_config,
        &default_plate(),
        req.ambient,
        req.initial_led,
    )
}

/// Parse a sliced file on disk + delegate to [`build_simulation_from_layers`].
/// This is the production producer for both the GUI Run button and the CLI
/// `resinsim sim --file <path>`; it routes through
/// [`crate::io::sliced::parse_sliced`] so every supported format (CTB, NanoDLP)
/// is handled here without a per-format branch (ADR-0021).
pub fn build_simulation_from_path(
    req: &RunRequest,
    path: &Path,
    repos: &ProfileRepos,
) -> Result<PrintSimulation, String> {
    let (_info, layers) = crate::io::sliced::parse_sliced(path)?;
    build_simulation_from_layers(req, &layers, repos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::LayerMask;
    use std::path::PathBuf;

    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    fn shipped_repos() -> ProfileRepos {
        ProfileRepos::new(&workspace_data_dir())
    }

    /// Build an n-layer cube-like LayerInput stack — mirrors the test
    /// fixture pattern at `tests/sim_summary_time_integration.rs:50`. Uses
    /// a 1×1 fully-solid mask at the printer's voxel resolution so cavity
    /// detection emits zero events (correct for a solid cross-section).
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
                .expect(
                    "test fixture: positive exposure + non-negative area satisfy LayerInput::new",
                )
                .with_mask(mask)
            })
            .collect()
    }

    fn ok_request() -> RunRequest {
        RunRequest::new_with_v1_defaults("generic_standard", "generic_msla_4k", None)
    }

    #[test]
    fn build_simulation_from_layers_happy_path() {
        let layers = cube_layer_inputs(100);
        let sim = build_simulation_from_layers(&ok_request(), &layers, &shipped_repos()).expect(
            "test fixture: shipped profiles + cube-like inputs satisfy run_from_layer_inputs",
        );
        assert_eq!(sim.layers().len(), 100);
        assert_eq!(sim.summary().total_layers, 100);
    }

    #[test]
    fn build_simulation_unknown_resin_returns_err() {
        let layers = cube_layer_inputs(10);
        let req =
            RunRequest::new_with_v1_defaults("definitely_not_a_resin", "generic_msla_4k", None);
        let err = build_simulation_from_layers(&req, &layers, &shipped_repos())
            .expect_err("unknown resin name must return Err");
        assert!(
            err.contains("failed to read"),
            "err must surface the underlying read failure; got: {err}"
        );
    }

    #[test]
    fn build_simulation_unknown_printer_returns_err() {
        let layers = cube_layer_inputs(10);
        let req =
            RunRequest::new_with_v1_defaults("generic_standard", "definitely_not_a_printer", None);
        let err = build_simulation_from_layers(&req, &layers, &shipped_repos())
            .expect_err("unknown printer name must return Err");
        assert!(
            err.contains("failed to read"),
            "err must surface the underlying read failure; got: {err}"
        );
    }

    /// Pairing-violation path: PrinterProfile fields are `pub(crate)`
    /// (`printer_profile.rs:23`) so direct field mutation isn't available.
    /// Hand-craft a TOML in a tempdir with `layer_height_range_um =
    /// [100.0, 150.0]` outside the `generic_standard` recipe's 50.0 µm —
    /// deserialisation bypasses the `pub(crate)` restriction. Then build
    /// a `ProfileRepos` pointing at the tempdir, mixing in the shipped
    /// resin TOML. Expected: `Err` starting with `"pairing:"`.
    #[test]
    fn build_simulation_pairing_violation() {
        let tmp = tempfile::tempdir().expect("test fixture: tempdir creation");
        let printers_dir = tmp.path().join("printers");
        let resins_dir = tmp.path().join("resins");
        std::fs::create_dir_all(&printers_dir).expect("test fixture: mkdir printers");
        std::fs::create_dir_all(&resins_dir).expect("test fixture: mkdir resins");

        let shipped_resin = workspace_data_dir()
            .join("resins")
            .join("generic_standard.toml");
        std::fs::copy(&shipped_resin, resins_dir.join("generic_standard.toml"))
            .expect("test fixture: copy shipped resin TOML");

        let shipped_printer = workspace_data_dir()
            .join("printers")
            .join("generic_msla_4k.toml");
        let mut printer_toml = std::fs::read_to_string(&shipped_printer)
            .expect("test fixture: read shipped printer TOML");
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
        let req = RunRequest::new_with_v1_defaults("generic_standard", "narrow_envelope", None);
        let err = build_simulation_from_layers(&req, &layers, &repos)
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

    /// End-to-end CTB integration — env-var-gated. Loads a real CTB
    /// fixture path provided via `RESINSIM_SLICED_FIXTURE` and runs the
    /// parse + simulate path. Skips by default per the existing convention
    /// at `viz/src/main.rs::tests::smoke_exit_with_load_ctb_flag_runs_setup_without_panic`.
    #[test]
    fn build_simulation_from_path_with_ctb_fixture() {
        let Ok(fixture) = std::env::var("RESINSIM_SLICED_FIXTURE") else {
            return;
        };
        let path = PathBuf::from(fixture);
        let sim = build_simulation_from_path(&ok_request(), &path, &shipped_repos())
            .expect("RESINSIM_SLICED_FIXTURE pointed at a valid CTB; simulation must run");
        assert!(
            !sim.layers().is_empty(),
            "fixture must yield at least one layer"
        );
    }
}
