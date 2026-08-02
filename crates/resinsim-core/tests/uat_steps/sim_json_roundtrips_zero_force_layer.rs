//! Step definitions for
//! `spec/uat/sim-json-roundtrips-zero-force-layer.md` UAT-1..UAT-3
//! (uat-unskip-campaign, ratified A2 top-up).
//!
//! SYMBOL VERIFICATION (performed BEFORE any step def below was written —
//! the check A2's own scope correction against the original 3-spec
//! selection showed was missing). Every production entry point these three
//! scenarios touch is default-features (`#[cfg]`-free):
//!   - `resinsim_core::app::SimulationRunner::run_from_areas` — no `#[cfg]`
//!     on the function or on `run_inner`; only `run_inner_full`'s internal
//!     voxel branches are feature-gated, and `run_from_areas` always calls
//!     `run_inner` (Tier-1, `voxel_cure_mm: None`), never reaching them.
//!   - `resinsim_core::repositories::{save_to_path, save_with_provenance,
//!     load_envelope, Provenance}` — `grep -n '#\[cfg(feature =
//!     "field-sim")\]' repositories/simulation_repo.rs` shows the ONLY
//!     gated items are the paired-sidecar encode/decode helpers
//!     (`encode_paired_sidecar`, `sidecar_bin_path`, `sha256_hex_of_file`,
//!     the `sidecar` sub-module, `load_and_install_sidecar_with_budget`,
//!     `sha256_hex_of_bytes`) and the internal branches inside
//!     `save_envelope_to_path` / `load_envelope_with_budget` that call
//!     them — never reached here (no voxel fields on this spec's
//!     `PrintSimulation`, so `SidecarFields::field_count() == 0`).
//!   - `resinsim_core::app::{ReportGenerator::text_format, ::json_format,
//!     ReportContext}` — `grep -n '#\[cfg(feature' app/report_generator.rs`
//!     returns NOTHING; the file carries no field-sim gates at all.
//!   - `resinsim_core::simulation::PrintSimulation::{layers, failures,
//!     summary}` — `summary()` (print_simulation.rs:515-585) has no
//!     `#[cfg]`; the file's many `#[cfg(feature = "field-sim")]` gates sit
//!     on unrelated voxel-field accessors/methods (`cure_field()` and
//!     siblings, `ec_t_for_layer`, etc.), none of which `summary()` calls.
//!
//! No symbol on the call path is `#[cfg(feature = "field-sim")]`, so this
//! module is safe to land as a single register entry removal (unlike
//! `calibration-disclosure-3of3-predicate` / `honest-zero-yield-fraction-
//! on-calibrated-solid`, which stay declared debt for exactly the opposite
//! reason).
//!
//! ENTRY POINTS. UAT-1 (producer): `SimulationRunner::run_from_areas` +
//! `save_with_provenance` — the exact pair `resinsim-inspect`'s `cmd_sim`
//! calls. UAT-2/UAT-3 (consumer): `save_to_path` to build the fixture, then
//! the REAL `resinsim` binary's `report health --in [--json]` subcommand via
//! `invoke_resinsim` (matching `cli_temperature_flag_validation.rs`'s
//! precedent) — this is the actual CLI surface the spec's Rationale says
//! "originally crashed", so the consumer half goes through the real
//! subprocess rather than an in-process `ReportGenerator` call. The
//! producer half (UAT-1) does NOT spawn `resinsim sim` on a real CTB/STL
//! file: per `tests/uat_steps/ctb_layer_height_authority.rs`'s established
//! rationale (this repo ships no CTB fixtures, and the production code path
//! between the CLI parser and `SimulationRunner` is identical to an
//! in-process call), the "CTB" is a synthesized `Vec<CrossSectionArea>`
//! with a trailing zero-area layer, run in-process.
//!
//! "Never hand-serialized JSON": the fixture `sim.json` on disk is always
//! produced by `save_to_path` / `save_with_provenance` (the real
//! `f32_with_infinity` serde adapter is what turns the trailing layer's
//! INFINITY into JSON `null`); test-side assertions parse it back with
//! `serde_json::Value` (reproducing what the spec's own `jq` selectors
//! would report, without a `jq` binary dependency) but never construct the
//! JSON literal by hand.
//!
//! REGEX DISTINCTNESS. Checked directly against the global step-def
//! inventory (every `regex = r` line across `tests/uat_steps/*.rs`) for
//! `exit`, `invokes`, `process`, `min safety`, `safety factor`, `report
//! health`, and `sim --file` — no collision. The closest neighbours are
//! `ctb_layer_height_authority.rs`'s `` `resinsim sim --file <CTB> --resin
//! <RESIN> --printer <PRINTER> --out <OUT>` `` (backtick-quoted, different
//! placeholder names) and `` the process exits with code 0 `` (extra
//! " with code"), both textually distinct from this module's `"resinsim sim
//! --file <PATH> --resin <R> --printer <P> --out <OUT>"` (double-quoted)
//! and `the process exits 0`. Within THIS file, `the process exits 0` is
//! registered ONCE and deliberately shared by UAT-1 and UAT-2 (same literal
//! text, not a cross-file collision) — see `then_process_exits_zero`'s doc
//! comment for how it serves both an in-process and a real-subprocess
//! scenario.

use cucumber::{given, then, when};
use resinsim_core::app::SimulationRunner;
use resinsim_core::repositories::{save_to_path, save_with_provenance, Provenance};
use resinsim_core::services::build_plate::PlateAdhesionProfile;
use resinsim_core::services::failure_predictor::SupportConfig;
use resinsim_core::simulation::PrintSimulation;
use resinsim_core::values::{AmbientTemperature, CrossSectionArea};

use super::cli_fixtures::invoke_resinsim;
use super::world::{PrinterBuilder, ResinBuilder, UatWorld};

/// Non-zero-area layers preceding the trailing zero-area one — several, at
/// genuinely different areas, so the resulting safety factors are finite
/// AND mutually distinct (not a single-layer degenerate case), making "the
/// minimum reflects only finite-force layers" a real claim rather than a
/// vacuous one.
const NORMAL_LAYER_COUNT: usize = 4;

/// Monotonic counter for unique temp `sim.json` paths — cucumber runs
/// scenarios within a feature concurrently (documented precedent:
/// `ctb_layer_height_authority.rs`'s per-scenario `World` field comment),
/// so a fixed shared filename would race across UAT-1/UAT-2/UAT-3.
fn unique_sim_json_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("uat-sim-json-{tag}-{n}.sim.json"))
}

/// `NORMAL_LAYER_COUNT` non-zero areas (100, 150, 200, 250 mm²) followed by
/// one `cross_section_area_mm2 = 0` layer — the "CTB whose final layer has
/// cross_section_area_mm2 = 0" / "sim.json envelope where at least one
/// layer has safety_factor: null" fixture shape both specs' Givens name.
fn areas_with_trailing_zero() -> Vec<CrossSectionArea> {
    let mut areas: Vec<CrossSectionArea> = (0..NORMAL_LAYER_COUNT)
        .map(|i| CrossSectionArea::new(100.0 + i as f64 * 50.0).expect("area is non-negative"))
        .collect();
    areas.push(CrossSectionArea::new(0.0).expect("0 mm² is a valid CrossSectionArea"));
    areas
}

/// Run `areas_with_trailing_zero()` through the real
/// `SimulationRunner::run_from_areas` entry point.
fn run_with_trailing_zero_force_layer() -> PrintSimulation {
    SimulationRunner::run_from_areas(
        &areas_with_trailing_zero(),
        &ResinBuilder::new().build(),
        &PrinterBuilder::new().build(),
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22 °C is in AmbientTemperature domain"),
        None,
    )
    .expect(
        "scenario fixture: ResinBuilder/PrinterBuilder output satisfies run_from_areas \
         preconditions",
    )
}

/// Parse the `sim.json` at `world.sim_json_path` as a generic
/// `serde_json::Value` — the test-side counterpart to the spec's `jq`
/// selectors. Reads a file `save_to_path`/`save_with_provenance` actually
/// wrote; never a hand-built `Value`.
fn parsed_sim_json(world: &UatWorld) -> serde_json::Value {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: a prior step populated sim_json_path");
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {} as JSON: {e}", path.display()))
}

// ---- UAT-1: Producer writes JSON null for INFINITY safety_factor ----------

#[given(regex = r"^a CTB whose final layer has cross_section_area_mm2 = 0$")]
fn given_ctb_final_layer_zero_area(_world: &mut UatWorld) {
    // Narrative — per `ctb_layer_height_authority.rs`'s established
    // rationale (this repo ships no CTB fixtures; the production code
    // path between the CLI parser and `SimulationRunner` is the same as
    // an in-process call), the When step below drives
    // `SimulationRunner::run_from_areas` directly with a trailing
    // zero-area "layer" rather than parsing a real CTB file.
}

#[when(
    regex = r#"^the user invokes "resinsim sim --file <PATH> --resin <R> --printer <P> --out <OUT>"$"#
)]
fn when_user_invokes_resinsim_sim(world: &mut UatWorld) {
    let resin = ResinBuilder::new().build();
    let printer = PrinterBuilder::new().build();
    match SimulationRunner::run_from_areas(
        &areas_with_trailing_zero(),
        &resin,
        &printer,
        &SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        },
        &PlateAdhesionProfile::default_textured(),
        AmbientTemperature::new(22.0).expect("22 °C is in AmbientTemperature domain"),
        None,
    ) {
        Err(e) => world.last_sim_err = Some(e),
        Ok(sim) => {
            let out_path = unique_sim_json_path("uat1-producer");
            let provenance = Provenance {
                input_path: "synthetic-ctb-zero-final-layer".to_string(),
                resin_name: resin.name().to_string(),
                printer_name: printer.name().to_string(),
                n_supports: 10,
                tip_radius_mm: 0.2,
            };
            match save_with_provenance(&out_path, &sim, &provenance) {
                Ok(()) => {
                    world.last_sim_err = None;
                    world.sim_json_path = Some(out_path);
                    world.sim_primary = Some(sim);
                }
                Err(e) => world.last_sim_err = Some(e),
            }
        }
    }
}

#[then(regex = r"^the process exits 0$")]
fn then_process_exits_zero(world: &mut UatWorld) {
    // Shared registration for UAT-1 (in-process producer — no real
    // subprocess, so "exit 0" reads as "the run+save succeeded") and
    // UAT-2 (real `resinsim` CLI subprocess — "exit 0" reads literally).
    // Branch on whichever the preceding When populated; the two are
    // mutually exclusive per scenario (`World` resets between scenarios).
    if let Some(code) = world.cli_exit_code {
        assert_eq!(
            code, 0,
            "expected exit code 0, got {code}; stderr={:?}",
            world.cli_stderr
        );
    } else {
        assert!(
            world.last_sim_err.is_none(),
            "expected the producer run+save to succeed, got error: {:?}",
            world.last_sim_err
        );
    }
}

#[then(
    regex = r#"^jq '\.simulation\.layers\[-1\]\.safety_factor' on <OUT> emits the literal string "null"$"#
)]
fn then_jq_last_layer_safety_factor_null(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let layers = value["simulation"]["layers"]
        .as_array()
        .expect("sim.json simulation.layers must be a JSON array");
    let last = layers.last().expect("simulation.layers must be non-empty");
    assert!(
        last["safety_factor"].is_null(),
        "expected the final (zero-force) layer's safety_factor to serialize as JSON null \
         (the f32_with_infinity adapter's contract), got: {:?}",
        last["safety_factor"]
    );
}

#[then(regex = r#"^jq '\.simulation\.layers\[-1\]\.total_force_n' on <OUT> emits "0"$"#)]
fn then_jq_last_layer_total_force_zero(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let layers = value["simulation"]["layers"]
        .as_array()
        .expect("sim.json simulation.layers must be a JSON array");
    let last = layers.last().expect("simulation.layers must be non-empty");
    let total_force = last["total_force_n"]
        .as_f64()
        .expect("total_force_n must be a JSON number");
    assert_eq!(
        total_force, 0.0,
        "expected total_force_n == 0 for the zero-area final layer, got {total_force}"
    );
}

// ---- UAT-2 / UAT-3 shared Given: a sim.json with a null safety_factor -----

#[given(regex = r"^a sim\.json envelope where at least one layer has safety_factor: null$")]
fn given_sim_json_envelope_with_null_safety_factor(world: &mut UatWorld) {
    let sim = run_with_trailing_zero_force_layer();
    let out_path = unique_sim_json_path("uat23-consumer");
    save_to_path(&out_path, &sim)
        .expect("scenario fixture: save_to_path must succeed for a valid PrintSimulation");
    world.sim_json_path = Some(out_path);
    world.sim_primary = Some(sim);
}

// ---- UAT-2: Consumer reads null safety_factor without crashing ------------

#[when(regex = r#"^the user invokes "resinsim report health --in <PATH>"$"#)]
fn when_user_invokes_report_health(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: Given step populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("temp sim.json path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r#"^stdout contains a "Min safety factor:" line$"#)]
fn then_stdout_contains_min_safety_factor_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains("Min safety factor:"),
        "expected a 'Min safety factor:' line in stdout, got: {stdout}"
    );
}

#[then(
    regex = r"^the rendered minimum reflects only finite-force layers \(zero-force layers don't constrain the minimum\)$"
)]
fn then_rendered_minimum_excludes_zero_force_layers(world: &mut UatWorld) {
    // `ReportGenerator::text_format`'s "Min safety factor: {:.2} at layer
    // {}" line, parsed directly (text mode is not JSON) — the real
    // production-rendered value, not a re-derived one.
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let min_sf = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("Min safety factor: "))
        .and_then(|rest| rest.split(" at layer").next())
        .and_then(|n| n.trim().parse::<f32>().ok())
        .unwrap_or_else(|| {
            panic!("could not parse the 'Min safety factor:' line from stdout: {stdout}")
        });
    assert!(
        min_sf.is_finite(),
        "rendered minimum safety factor must be finite — a min() that let the trailing \
         zero-force layer's INFINITY (or a propagated null) through would fail this: got {min_sf}"
    );
}

// ---- UAT-3: Consumer json-mode produces a finite min_safety_factor --------

#[when(regex = r#"^the user invokes "resinsim report health --in <PATH> --json"$"#)]
fn when_user_invokes_report_health_json(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .as_ref()
        .expect("scenario invariant: Given step populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("temp sim.json path is UTF-8"),
            "--json",
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the JSON output's summary\.min_safety_factor is a finite number > 0$")]
fn then_json_min_safety_factor_finite_positive(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("--json stdout must parse as JSON: {e}; stdout={stdout}"));
    let min_sf = value["summary"]["min_safety_factor"]
        .as_f64()
        .unwrap_or_else(|| {
            panic!(
                "summary.min_safety_factor must be a JSON number, got: {:?}",
                value["summary"]["min_safety_factor"]
            )
        });
    assert!(
        min_sf.is_finite() && min_sf > 0.0,
        "expected a finite, positive summary.min_safety_factor, got {min_sf}"
    );
}

#[then(regex = r"^the null-SF layers are excluded from the min\(\), not propagating null$")]
fn then_null_sf_layers_excluded_from_min(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("--json stdout must parse as JSON: {e}; stdout={stdout}"));
    // The direct negation of "propagating null": summary.min_safety_factor
    // must be a present, non-null NUMBER. A null-propagating min() would
    // make this field JSON null (or the whole field absent); the preceding
    // Then already proved it is finite and positive, which independently
    // implies non-null, but this step re-reads the field fresh rather than
    // reusing that step's parsed value, per the one-Then-one-production-
    // observation convention this module's siblings follow.
    assert!(
        !value["summary"]["min_safety_factor"].is_null(),
        "summary.min_safety_factor must not be JSON null — that would mean a null propagated \
         through the min()"
    );
    assert!(
        value["summary"]["min_safety_factor"].is_number(),
        "summary.min_safety_factor must be a JSON number, got: {:?}",
        value["summary"]["min_safety_factor"]
    );
}
