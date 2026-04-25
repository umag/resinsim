//! Repository for `PrintSimulation` aggregate persistence (ADR-0009).
//!
//! Persists the `PrintSimulation` aggregate as JSON via
//! `serde_json::to_string_pretty` / `serde_json::from_str`. The format is
//! JSON (not TOML, unlike `printer_repo` and `resin_repo`) because
//! `PrintSimulation` carries `Vec<LayerResult>` and `Vec<FailureEvent>`
//! which TOML handles poorly; JSON is also already used by
//! `app::ReportGenerator` for simulation output.
//!
//! # Directory semantics
//!
//! `save` calls `fs::create_dir_all(data_dir)` because simulations are
//! user-output (callers may not have pre-created the dir). `load` and
//! `list` error on missing directory like `printer_repo` and `resin_repo`
//! do — read semantics fail loud. Write and read directory semantics
//! deliberately differ.
//!
//! # Naming
//!
//! Caller supplies the `name`; the repository does no UUID, timestamp, or
//! input-hash generation. This matches `printer_repo` / `resin_repo`. Phase
//! 2 callers (Bevy viz reload) are free to choose the naming convention
//! (timestamp, content hash, user label) without needing a repo redesign.
//!
//! # Default storage location
//!
//! The repository takes a caller-supplied `data_dir`. It does NOT default
//! to `data/` — that path ships fixtures (printer + resin TOMLs); it would
//! be a category error to mix user-generated simulation output with
//! shipped reference data. Phase 2 wiring should pick a user-data
//! directory.
//!
//! # Deserialize-bypass guard
//!
//! `load` calls `PrintSimulation::validate()` after `serde_json::from_str`.
//! The validate() method (added alongside this repository per ADR-0009)
//! re-checks child-entity invariants and aggregate-level layer-index
//! sequentiality that `#[derive(Deserialize)]` bypasses.

use crate::simulation::PrintSimulation;
use std::path::{Path, PathBuf};

pub struct SimulationRepository {
    data_dir: PathBuf,
}

impl SimulationRepository {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// Persist a simulation under `<data_dir>/<name>.json`.
    ///
    /// Creates `data_dir` if it does not yet exist (write semantics).
    pub fn save(&self, name: &str, sim: &PrintSimulation) -> Result<(), String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| {
            format!(
                "failed to create simulation data dir {}: {e}",
                self.data_dir.display()
            )
        })?;
        let path = self.data_dir.join(format!("{name}.json"));
        let json = serde_json::to_string_pretty(sim)
            .map_err(|e| format!("failed to serialize simulation {name}: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        Ok(())
    }

    /// Load a simulation by name (filename without `.json` extension).
    ///
    /// Calls `PrintSimulation::validate()` after deserialize so a tampered
    /// or schema-evolved file cannot silently violate aggregate invariants.
    pub fn load(&self, name: &str) -> Result<PrintSimulation, String> {
        let path = self.data_dir.join(format!("{name}.json"));
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let sim: PrintSimulation = serde_json::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        sim.validate()
            .map_err(|e| format!("invalid simulation {}: {e}", path.display()))?;
        Ok(sim)
    }

    /// List available simulation names (filenames stripped of `.json`).
    ///
    /// Errors on missing data_dir (read semantics).
    pub fn list(&self) -> Result<Vec<String>, String> {
        let entries = std::fs::read_dir(&self.data_dir)
            .map_err(|e| format!("failed to read {}: {e}", self.data_dir.display()))?;

        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FailureEvent, FailureType, Severity};
    use crate::simulation::print_simulation::tests::{default_recipe, linear_printer, make_layer};

    /// Per-test isolation directory under workspace `target/test-tmp/`.
    ///
    /// `target/` is gitignored and exists during cargo runs. The `<name>`
    /// suffix gives each test its own directory so nextest's parallel
    /// execution doesn't cross-contaminate. Each test starts by removing
    /// its directory and recreating fresh.
    fn test_dir(name: &str) -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = Path::new(manifest_dir)
            .join("../../target/test-tmp")
            .join(format!("sim-repo-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("test setup: must be able to create test_dir");
        dir
    }

    fn build_sim() -> PrintSimulation {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        sim.add_layer(
            make_layer(1, 20.0, 0.8, 22.5),
            vec![FailureEvent {
                layer: 1,
                failure_type: FailureType::SupportOverload,
                severity: Severity::Critical,
                message: "test".into(),
            }],
        )
        .expect("test fixture: explicit index 1 matches layer count 1 at this call site");
        sim.add_layer(make_layer(2, 10.0, 2.0, 23.0), vec![])
            .expect("test fixture: explicit index 2 matches layer count 2 at this call site");
        sim
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = test_dir("round-trip");
        let repo = SimulationRepository::new(&dir);
        let saved = build_sim();
        repo.save("run1", &saved).expect("save must succeed");
        let loaded = repo.load("run1").expect("load must succeed");

        let s = saved.summary();
        let l = loaded.summary();
        assert_eq!(s.total_layers, l.total_layers);
        assert!((s.max_peel_force_n - l.max_peel_force_n).abs() < 1e-6);
        assert!((s.min_safety_factor - l.min_safety_factor).abs() < 1e-6);
        assert!((s.total_time_sec - l.total_time_sec).abs() < 1e-4);

        assert_eq!(saved.layers().len(), loaded.layers().len());
        assert_eq!(saved.failures().len(), loaded.failures().len());
    }

    #[test]
    fn load_validates_child_entities() {
        let dir = test_dir("validates-child");
        let repo = SimulationRepository::new(&dir);
        let saved = build_sim();
        let mut value = serde_json::to_value(&saved).expect("serialize");
        value["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
        let path = dir.join("tampered.json");
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap())
            .expect("test setup: write tampered file");

        let err = repo
            .load("tampered")
            .expect_err("load must reject invalid recipe");
        assert!(
            err.contains("invalid simulation") && err.contains("layer_height_um"),
            "error must identify the violating field; got: {err}"
        );
    }

    #[test]
    fn list_returns_sorted_names() {
        let dir = test_dir("list-sorted");
        let repo = SimulationRepository::new(&dir);
        let sim = build_sim();
        repo.save("zebra", &sim).expect("save zebra");
        repo.save("alpha", &sim).expect("save alpha");
        repo.save("middle", &sim).expect("save middle");

        let names = repo.list().expect("list must succeed");
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn load_missing_returns_err() {
        let dir = test_dir("load-missing");
        let repo = SimulationRepository::new(&dir);
        let err = repo
            .load("does-not-exist")
            .expect_err("load of missing file must fail");
        assert!(
            err.contains("does-not-exist.json"),
            "error must mention the missing path; got: {err}"
        );
    }

    #[test]
    fn save_creates_data_dir_when_missing() {
        let parent = test_dir("create-dir-parent");
        let nested = parent.join("never_existed_yet");
        assert!(!nested.exists(), "precondition: nested dir must not exist");

        let repo = SimulationRepository::new(&nested);
        repo.save("first-run", &build_sim())
            .expect("save must create data_dir and succeed");

        assert!(nested.is_dir(), "save must have created the data_dir");
        assert!(
            nested.join("first-run.json").is_file(),
            "save must have written the file inside the new data_dir"
        );
    }
}
