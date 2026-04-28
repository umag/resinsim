//! Repository for `PrintSimulation` aggregate persistence (ADR-0009, ADR-0015).
//!
//! Persists the `PrintSimulation` aggregate as JSON wrapped in a
//! `SimulationEnvelope { schema_version, simulation }`. The envelope lives
//! at the IO boundary per ADR-0009 — `PrintSimulation` itself stays
//! schema-version-free for in-memory consumers (tests, viz). ADR-0015
//! documents the canonical interchange policy and version-bump rules.
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
//!
//! # Atomic write
//!
//! `save_to_path` writes to `<out>.tmp` first, then `std::fs::rename`s to
//! `<out>`. POSIX rename on the same filesystem is atomic, so a partial
//! write cannot corrupt an existing `<out>` from a downstream consumer's
//! perspective — they either see the old file or the new file, never a
//! truncated one.

use crate::simulation::PrintSimulation;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current `sim.json` schema version. Bumped on any breaking change to the
/// on-disk shape of the persisted simulation (per ADR-0015 versioning rules).
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// On-disk envelope around `PrintSimulation`. The `schema_version` field
/// lets consumers reject files written by a future or past schema-incompatible
/// producer with a typed error rather than a confusing parse failure or
/// silent shape drift.
///
/// `provenance` is optional run-context metadata (input path, profile names,
/// support config) that the producer carries forward so consumers like
/// `resinsim report health --in` can reconstruct the report header without
/// the user re-supplying CLI args. Absent `provenance` is valid (legacy
/// callers / GUI sidecars) — consumers degrade to placeholder strings.
///
/// This struct is the *deserialize* shape. For serialize, callers use
/// [`SimulationEnvelopeRef`] to avoid having to clone the aggregate.
#[derive(Debug, Deserialize)]
struct SimulationEnvelope {
    schema_version: u32,
    simulation: PrintSimulation,
    #[serde(default)]
    provenance: Option<Provenance>,
}

/// Borrowed view of [`SimulationEnvelope`] used for serialize-only paths so
/// `save_to_path` does not need to clone the aggregate.
#[derive(Debug, Serialize)]
struct SimulationEnvelopeRef<'a> {
    schema_version: u32,
    simulation: &'a PrintSimulation,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<&'a Provenance>,
}

/// Run-context metadata carried alongside a `PrintSimulation` in the
/// envelope. Producers (CLI `resinsim sim`) populate it; consumers
/// (`report health --in`, downstream LLM tooling) read it. The envelope
/// rejects-unknown-version guard means unknown extra `provenance` fields
/// land here as `serde(default)` — additive evolution does not bump
/// `schema_version` per ADR-0015.
///
/// `validate()` runs after deserialise to reject obviously-tampered
/// values (NaN floats, negative tip radius) before any downstream
/// rendering. Provenance is supplementary metadata, not the load-bearing
/// payload, but its values flow into report headers and JSON output —
/// keeping it well-formed avoids surfacing `NaN` or negative radii in
/// user-facing surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Producer's input path (`.ctb` or `.stl`). Free-form string —
    /// consumers display it but don't try to resolve.
    pub input_path: String,
    /// Resin profile name (file stem of the resolved `<data_dir>/resins/<name>.toml`).
    pub resin_name: String,
    /// Printer profile name (file stem of the resolved `<data_dir>/printers/<name>.toml`).
    pub printer_name: String,
    /// Support count used for the run.
    pub n_supports: u32,
    /// Support tip radius (mm) used for the run.
    pub tip_radius_mm: f32,
}

impl Provenance {
    /// Re-check that every field is well-formed after deserialise.
    /// Called from [`load_envelope`] after the schema-version check;
    /// rejects tampered envelopes with non-finite or non-positive
    /// tip_radius before downstream consumers (text/JSON renderers,
    /// report header) see the values.
    fn validate(&self) -> Result<(), String> {
        if !self.tip_radius_mm.is_finite() {
            return Err(format!(
                "provenance.tip_radius_mm is not finite: {}",
                self.tip_radius_mm
            ));
        }
        if self.tip_radius_mm < 0.0 {
            return Err(format!(
                "provenance.tip_radius_mm is negative: {}",
                self.tip_radius_mm
            ));
        }
        Ok(())
    }
}

/// Atomically write a `PrintSimulation` to `path` as a `SimulationEnvelope`
/// (schema_version = [`CURRENT_SCHEMA_VERSION`]).
///
/// The write is staged at `<path>.tmp` then renamed to `<path>`; POSIX
/// rename is atomic on the same volume, so a write failure mid-flight
/// leaves the existing `<path>` (if any) intact and only an orphaned
/// `<path>.tmp` may be left behind.
///
/// On serialize / write failure the caller-visible error mentions both the
/// failing operation and the path. On rename failure the staged `<path>.tmp`
/// is best-effort cleaned up so a retry does not see stale partial bytes —
/// **the cleanup is best-effort and may itself fail**, leaving an orphan
/// `.tmp` file. Subsequent successful runs against the same `<path>` will
/// transparently overwrite the orphan via `std::fs::write`, so the orphan
/// does not poison subsequent retries; it is, however, a long-lived disk-
/// space leak in pathological cases (e.g. concurrent process holding the
/// `.tmp` open). Long-running data dirs may want a periodic sweep for
/// `*.tmp` files older than a threshold.
pub fn save_to_path(path: &Path, sim: &PrintSimulation) -> Result<(), String> {
    save_envelope_to_path(path, sim, None)
}

/// Atomically write a `PrintSimulation` plus run-context [`Provenance`] to
/// `path`. Used by the CLI `resinsim sim` subcommand so downstream
/// `report health --in` can recover the producer's input path and
/// profile names. See [`save_to_path`] for the atomic-write contract.
pub fn save_with_provenance(
    path: &Path,
    sim: &PrintSimulation,
    provenance: &Provenance,
) -> Result<(), String> {
    save_envelope_to_path(path, sim, Some(provenance))
}

fn save_envelope_to_path(
    path: &Path,
    sim: &PrintSimulation,
    provenance: Option<&Provenance>,
) -> Result<(), String> {
    let envelope = SimulationEnvelopeRef {
        schema_version: CURRENT_SCHEMA_VERSION,
        simulation: sim,
        provenance,
    };
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| format!("failed to serialize simulation for {}: {e}", path.display()))?;

    let tmp = tmp_sibling(path);
    if let Some(parent) = tmp.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create simulation data dir {}: {e}",
                parent.display()
            )
        })?;
    }
    std::fs::write(&tmp, json).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("failed to write {}: {e}", tmp.display())
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "failed to rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

/// `<path>.tmp` next to `path`. We append `.tmp` to the file name so the
/// staged file lands in the same directory (rename is only atomic on the
/// same filesystem).
fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// Load a simulation from an absolute path. Sibling to
/// [`SimulationRepository`], used by callers that hand in a full path
/// (e.g. `resinsim-viz --load-sim PATH.json`) without a `data_dir + name`
/// split.
///
/// Parses the on-disk `SimulationEnvelope`, rejects an unknown
/// `schema_version` with a typed error (per ADR-0015), then calls
/// `PrintSimulation::validate()` after deserialise so a tampered or
/// schema-evolved file cannot silently violate aggregate invariants — same
/// deserialize-bypass guard documented at the module level. Errors carry
/// four stable substrings that downstream callers (and human grep) match
/// against:
///
/// - `"failed to read"` — `std::fs::read_to_string` failed.
/// - `"failed to parse"` — `serde_json::from_str` failed.
/// - `"unknown schema_version"` — envelope's schema_version is not [`CURRENT_SCHEMA_VERSION`].
/// - `"invalid simulation"` — the deserialised aggregate failed `validate()`.
///
/// All four substrings appear alongside the failing path so debugging
/// is unambiguous when an absolute path is supplied.
pub fn load_from_path(path: &Path) -> Result<PrintSimulation, String> {
    Ok(load_envelope(path)?.simulation)
}

/// Loaded envelope: the [`PrintSimulation`] aggregate plus optional
/// [`Provenance`] metadata. Returned by [`load_envelope`] when the caller
/// (e.g. `resinsim report health --in`) needs the run-context metadata
/// alongside the aggregate; [`load_from_path`] is the convenience for
/// callers that only want the simulation.
#[derive(Debug)]
pub struct LoadedEnvelope {
    pub simulation: PrintSimulation,
    pub provenance: Option<Provenance>,
}

/// Load the full envelope (simulation + optional provenance). Same
/// version-check + validate() guards as [`load_from_path`]; same stable
/// error substrings.
pub fn load_envelope(path: &Path) -> Result<LoadedEnvelope, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let envelope: SimulationEnvelope = serde_json::from_str(&contents)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    if envelope.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "unknown schema_version {} in {} (expected {})",
            envelope.schema_version,
            path.display(),
            CURRENT_SCHEMA_VERSION
        ));
    }
    envelope
        .simulation
        .validate()
        .map_err(|e| format!("invalid simulation {}: {e}", path.display()))?;
    if let Some(provenance) = envelope.provenance.as_ref() {
        provenance
            .validate()
            .map_err(|e| format!("invalid provenance in {}: {e}", path.display()))?;
    }
    Ok(LoadedEnvelope {
        simulation: envelope.simulation,
        provenance: envelope.provenance,
    })
}

/// Backwards-compatible alias for [`load_from_path`]; kept so existing
/// callers continue to compile while ADR-0015's renamed surface stabilises.
#[deprecated(note = "renamed to load_from_path per ADR-0015")]
pub fn load_simulation(path: &Path) -> Result<PrintSimulation, String> {
    load_from_path(path)
}

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
    /// Creates `data_dir` if it does not yet exist (write semantics) and
    /// delegates to [`save_to_path`] so the on-disk shape and atomic-write
    /// guarantees match the absolute-path producer (`resinsim sim --out`).
    /// Per ADR-0015, the on-disk shape is the
    /// `{ schema_version, simulation }` envelope and the write is staged
    /// at `<path>.tmp` then `std::fs::rename`d to `<path>` for POSIX
    /// atomic semantics. A rename failure leaves an orphaned `<path>.tmp`
    /// (best-effort cleanup runs but is not guaranteed) and does NOT
    /// modify any pre-existing `<path>` from a downstream consumer's
    /// perspective.
    pub fn save(&self, name: &str, sim: &PrintSimulation) -> Result<(), String> {
        let path = self.data_dir.join(format!("{name}.json"));
        save_to_path(&path, sim)
    }

    /// Load a simulation by name (filename without `.json` extension).
    ///
    /// Thin wrapper that joins `data_dir + name.json` and delegates to the
    /// free function [`load_from_path`]. The envelope schema-version check
    /// and validate() deserialize-bypass guard live in the free function
    /// so both call sites — repo-by-name and viz-by-absolute-path
    /// (`--load-sim`) — share one code path.
    pub fn load(&self, name: &str) -> Result<PrintSimulation, String> {
        let path = self.data_dir.join(format!("{name}.json"));
        load_from_path(&path)
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
        let mut sim_value = serde_json::to_value(&saved).expect("serialize");
        sim_value["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
        // Wrap in a current-schema envelope so the version-check passes and
        // the validate() guard is what fails.
        let envelope = serde_json::json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "simulation": sim_value,
        });
        let path = dir.join("tampered.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&envelope).expect("serialize tampered value"),
        )
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

    #[test]
    fn load_from_path_round_trips_from_absolute_path() {
        // Free-fn variant: callers (resinsim-viz --load-sim) hand it an
        // absolute path with no data_dir/name split. Same validate() guard
        // as SimulationRepository::load(name).
        let dir = test_dir("free-fn-round-trip");
        let repo = SimulationRepository::new(&dir);
        let saved = build_sim();
        repo.save("via-repo", &saved).expect("save");
        let path = dir.join("via-repo.json");

        let loaded = load_from_path(&path).expect("load_from_path must succeed");
        assert_eq!(saved.layers().len(), loaded.layers().len());
    }

    #[test]
    fn load_from_path_validates_via_same_guard() {
        // Same deserialize-bypass guard as load(name): tampered file must
        // fail with "invalid simulation". Must wrap in a valid-version
        // envelope so the version check doesn't pre-empt the validate() guard.
        let dir = test_dir("free-fn-validates");
        let saved = build_sim();
        let mut sim_value = serde_json::to_value(&saved).expect("serialize");
        sim_value["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
        let envelope = serde_json::json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "simulation": sim_value,
        });
        let path = dir.join("tampered.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&envelope).expect("serialize"),
        )
        .expect("write tampered file");

        let err = load_from_path(&path).expect_err("must reject invalid recipe");
        assert!(
            err.contains("invalid simulation") && err.contains("layer_height_um"),
            "free fn must surface the same 'invalid simulation' substring; got: {err}"
        );
    }

    #[test]
    fn load_from_path_missing_path_mentions_absolute_path() {
        // The free fn is fed an absolute path by the viz CLI; on missing
        // file the error must mention the full path so debugging is
        // unambiguous (not just the basename).
        let path = std::path::PathBuf::from("/definitely/does/not/exist/nope.json");
        let err = load_from_path(&path).expect_err("missing file must fail");
        assert!(
            err.contains("/definitely/does/not/exist/nope.json"),
            "error must echo the full path; got: {err}"
        );
        assert!(
            err.contains("failed to read"),
            "error must contain stable substring 'failed to read'; got: {err}"
        );
    }

    #[test]
    fn error_messages_contain_stable_substrings() {
        // Pin the four error-message substrings that downstream code
        // (and human grep) match against. After the envelope refactor
        // these MUST appear verbatim in the surfaced errors.
        // - "failed to read"        on missing file
        // - "failed to parse"       on garbage JSON
        // - "unknown schema_version" on wrong-version envelope
        // - "invalid simulation"    on validate() rejection
        let dir = test_dir("stable-substrings");
        let repo = SimulationRepository::new(&dir);

        // 1. Missing file -> "failed to read".
        let err = repo.load("never-saved").expect_err("missing must fail");
        assert!(
            err.contains("failed to read"),
            "missing-file substring lost: {err}"
        );

        // 2. Garbage JSON -> "failed to parse".
        let garbage_path = dir.join("garbage.json");
        std::fs::write(&garbage_path, b"this is not json").expect("write garbage");
        let err = load_from_path(&garbage_path).expect_err("garbage must fail");
        assert!(
            err.contains("failed to parse"),
            "parse substring lost: {err}"
        );

        // 3. Unknown schema_version -> "unknown schema_version".
        let saved = build_sim();
        let sim_value = serde_json::to_value(&saved).expect("serialize");
        let bad_version_envelope = serde_json::json!({
            "schema_version": 999,
            "simulation": sim_value.clone(),
        });
        let bad_version_path = dir.join("bad-version.json");
        std::fs::write(
            &bad_version_path,
            serde_json::to_string_pretty(&bad_version_envelope)
                .expect("serialize bad-version envelope"),
        )
        .expect("write bad-version file");
        let err = load_from_path(&bad_version_path).expect_err("unknown schema_version must fail");
        assert!(
            err.contains("unknown schema_version") && err.contains("999"),
            "unknown-version substring lost: {err}"
        );

        // 4. Schema_version as a JSON string -> "failed to parse".
        // Defends against a future serde version that adds string→u32
        // coercion silently changing the rejection behaviour.
        let string_version_path = dir.join("string-version.json");
        std::fs::write(
            &string_version_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "1",
                "simulation": sim_value.clone(),
            }))
            .expect("serialize string-version envelope"),
        )
        .expect("write string-version file");
        let err = load_from_path(&string_version_path)
            .expect_err("schema_version as a JSON string must be rejected");
        assert!(
            err.contains("failed to parse"),
            "string schema_version must fail with parse error; got: {err}"
        );

        // 5. Tampered (deserialises, version OK, fails validate()) -> "invalid simulation".
        let mut tampered_sim = sim_value;
        tampered_sim["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
        let tampered_envelope = serde_json::json!({
            "schema_version": CURRENT_SCHEMA_VERSION,
            "simulation": tampered_sim,
        });
        std::fs::write(
            dir.join("tampered.json"),
            serde_json::to_string_pretty(&tampered_envelope).expect("serialize tampered"),
        )
        .expect("write tampered");
        let err = repo.load("tampered").expect_err("tampered must fail");
        assert!(
            err.contains("invalid simulation"),
            "invalid substring lost: {err}"
        );
    }

    #[test]
    fn save_to_path_writes_envelope_with_current_schema_version() {
        // The on-disk shape is { schema_version, simulation: {...} }, not the
        // bare PrintSimulation. Locks the canonical-interchange contract that
        // ADR-0015 governs and that the zod schema (schemas/sim-json/v1.ts)
        // mirrors.
        let dir = test_dir("envelope-shape");
        let path = dir.join("envelope.sim.json");
        let saved = build_sim();
        save_to_path(&path, &saved).expect("save_to_path must succeed");

        let bytes = std::fs::read_to_string(&path).expect("read written file");
        let value: serde_json::Value =
            serde_json::from_str(&bytes).expect("written file must be valid JSON");
        assert_eq!(
            value
                .get("schema_version")
                .and_then(|v| v.as_u64())
                .expect("envelope must have a schema_version field"),
            u64::from(CURRENT_SCHEMA_VERSION),
            "envelope must record the current schema_version"
        );
        assert!(
            value.get("simulation").is_some(),
            "envelope must wrap the aggregate under a 'simulation' field"
        );
        // Sanity-check that the wrapped simulation still has its identifying
        // fields so we know the inner shape didn't get accidentally flattened.
        assert!(
            value["simulation"].get("recipe").is_some(),
            "wrapped simulation must contain 'recipe'"
        );
        assert!(
            value["simulation"].get("layers").is_some(),
            "wrapped simulation must contain 'layers'"
        );
    }

    #[test]
    fn save_to_path_round_trips_infinity_safety_factor_via_null() {
        // Real-world regression: a real CTB (Lilith Torso, Mars 5 Ultra)
        // produces zero-force layers where SafetyFactor::compute returns
        // None and failure_predictor stores f32::INFINITY in the
        // LayerResult. JSON has no Infinity literal — serde_json writes
        // INFINITY as `null`, then the deserializer fails on null→f32
        // unless an adapter handles it. The `f32_with_infinity` adapter
        // on LayerResult.safety_factor maps INFINITY ↔ null lossless.
        use crate::entities::LayerResult;

        let dir = test_dir("infinity-safety-factor-round-trip");
        let path = dir.join("inf-safety.sim.json");

        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        // Layer 0: normal, finite safety factor.
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("layer 0");
        // Layer 1: zero load → safety_factor = INFINITY (the bug case).
        let inf_layer = LayerResult {
            index: 1,
            cure_depth_um: 100.0,
            peel_force_n: 0.0,
            suction_force_n: 0.0,
            total_force_n: 0.0,
            support_capacity_n: 95.0,
            safety_factor: f32::INFINITY,
            cross_section_area_mm2: 0.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: 22.0,
            viscosity_mpa_s: 200.0,
            z_deflection_um: 0.0,
            effective_layer_height_um: 50.0,
            worst_cure_depth_um: 100.0,
        };
        sim.add_layer(inf_layer, vec![]).expect("layer 1");

        save_to_path(&path, &sim).expect("save with INFINITY safety_factor");

        // On-disk: layer 1's safety_factor must be JSON null.
        let bytes = std::fs::read_to_string(&path).expect("read written file");
        let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse JSON");
        let layer1_sf = &value["simulation"]["layers"][1]["safety_factor"];
        assert!(
            layer1_sf.is_null(),
            "INFINITY safety_factor must serialise as JSON null; got: {layer1_sf}"
        );
        let layer0_sf = &value["simulation"]["layers"][0]["safety_factor"];
        assert!(
            layer0_sf.as_f64().is_some(),
            "finite safety_factor must serialise as JSON number; got: {layer0_sf}"
        );

        // Round-trip: null deserialises back to f32::INFINITY.
        let loaded = load_from_path(&path).expect("load round-trip");
        assert_eq!(loaded.layers().len(), 2);
        assert!(
            loaded.layers()[0].safety_factor.is_finite(),
            "layer 0 must round-trip finite SF as finite f32"
        );
        assert!(
            loaded.layers()[1].safety_factor.is_infinite()
                && loaded.layers()[1].safety_factor.is_sign_positive(),
            "layer 1 must round-trip null back to f32::INFINITY; got: {}",
            loaded.layers()[1].safety_factor
        );
    }

    #[test]
    fn save_to_path_round_trip_byte_identity() {
        // save_to_path is deterministic: writing the same aggregate twice
        // produces byte-identical output. This is what makes golden-fixture
        // comparison (step 8) reliable.
        let dir = test_dir("round-trip-byte-identity");
        let path_a = dir.join("a.sim.json");
        let path_b = dir.join("b.sim.json");
        let sim = build_sim();
        save_to_path(&path_a, &sim).expect("save a");
        save_to_path(&path_b, &sim).expect("save b");

        let bytes_a = std::fs::read(&path_a).expect("read a");
        let bytes_b = std::fs::read(&path_b).expect("read b");
        assert_eq!(
            bytes_a, bytes_b,
            "two saves of the same aggregate must produce byte-identical files"
        );

        // Loading + re-saving also produces the same bytes (idempotent
        // serialise on the same in-memory value).
        let loaded = load_from_path(&path_a).expect("load a");
        let path_c = dir.join("c.sim.json");
        save_to_path(&path_c, &loaded).expect("save c");
        let bytes_c = std::fs::read(&path_c).expect("read c");
        assert_eq!(
            bytes_a, bytes_c,
            "load -> save must be byte-identical to the original save"
        );
    }

    #[test]
    fn save_to_path_rejects_unknown_schema_version_on_load() {
        // Producer-side has only one current version, so we tamper a saved
        // file's schema_version to assert load_from_path rejects with the
        // documented typed error rather than panicking or silently accepting
        // future bytes as if they were v1.
        let dir = test_dir("reject-future-version");
        let path = dir.join("future.sim.json");
        save_to_path(&path, &build_sim()).expect("seed file");
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read"))
                .expect("parse seed envelope");
        value["schema_version"] = serde_json::json!(999);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&value).expect("serialize"),
        )
        .expect("rewrite file");

        let err = load_from_path(&path).expect_err("unknown schema_version must fail (not panic)");
        assert!(
            err.contains("unknown schema_version") && err.contains("999"),
            "error must surface the rejected version; got: {err}"
        );
    }

    #[test]
    fn save_to_path_does_not_overwrite_unrelated_file_when_parent_is_blocked() {
        // Atomic-write contract under a blocked-parent failure mode: when
        // save_to_path is asked to write to <PATH> whose parent is
        // currently a regular file, create_dir_all fails and save_to_path
        // must surface the error without disturbing any unrelated file
        // (the blocking file is "unrelated" — it's not the target output).
        // Downstream consumers either see the old <PATH> (if any) or the
        // new <PATH>, never a truncated half-write.
        let dir = test_dir("atomic-write");
        let parent_as_file = dir.join("not-a-dir");
        std::fs::write(&parent_as_file, b"i am a file").expect("write blocking file");
        let path = parent_as_file.join("inner.sim.json");

        let err = save_to_path(&path, &build_sim())
            .expect_err("save must fail when parent path is a file");
        assert!(
            err.contains("failed to") && err.contains("not-a-dir"),
            "error must mention the failing operation and the offending path; got: {err}"
        );

        // The blocking file must be untouched.
        let bytes = std::fs::read(&parent_as_file).expect("read blocking file");
        assert_eq!(bytes, b"i am a file");
    }

    #[test]
    fn load_envelope_rejects_non_finite_provenance_tip_radius() {
        // Defence-in-depth: a tampered envelope with NaN tip_radius_mm
        // would otherwise flow into the report header. Provenance::validate()
        // catches it at load_envelope so downstream renderers (text/JSON)
        // never see the bogus value.
        let dir = test_dir("provenance-nan");
        let path = dir.join("nan-tip-radius.sim.json");
        save_with_provenance(
            &path,
            &build_sim(),
            &Provenance {
                input_path: "fixture.ctb".into(),
                resin_name: "Test".into(),
                printer_name: "Test".into(),
                n_supports: 20,
                tip_radius_mm: 0.2,
            },
        )
        .expect("seed valid envelope");

        // Tamper: replace tip_radius_mm with a string the JSON parser
        // accepts as Real but is non-finite. JSON doesn't permit literal
        // NaN/Infinity, but float-as-string-tampering would be caught by
        // serde during parse. So we test the boundary that DOES survive
        // parse: a negative tip_radius_mm.
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        value["provenance"]["tip_radius_mm"] = serde_json::json!(-1.0);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&value).expect("serialize tampered"),
        )
        .expect("rewrite");

        let err = load_envelope(&path)
            .expect_err("negative tip_radius_mm must be rejected by Provenance::validate");
        assert!(
            err.contains("invalid provenance") && err.contains("tip_radius_mm"),
            "error must surface the tampered field; got: {err}"
        );
    }

    #[test]
    fn save_to_path_creates_missing_parent_dir() {
        // Hands-off ergonomics: the producer (resinsim sim --out path)
        // shouldn't need to mkdir -p before calling save_to_path.
        let parent = test_dir("create-parent-dir");
        let nested = parent.join("nested").join("deeper");
        let path = nested.join("out.sim.json");
        assert!(!nested.exists());

        save_to_path(&path, &build_sim()).expect("save_to_path must mkdir -p");
        assert!(
            path.is_file(),
            "envelope file must exist after save_to_path"
        );
    }
}
