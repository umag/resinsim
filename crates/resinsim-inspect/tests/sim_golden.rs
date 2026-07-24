//! Golden-file regression tests for sim.json output.
//!
//! Per ADR-0015 the on-disk envelope is the canonical interchange format —
//! drift in field names, float formatting, key ordering, or required-field
//! shape would break downstream consumers (resinsim-viz `--load-sim`,
//! `resinsim report health --in`, the zod schema in
//! `schemas/sim-json/v1.ts`).
//!
//! Three default-suite cases mirror the plan's required coverage:
//!
//! - `baseline`: a 5-layer synthetic cube against shipped profiles.
//! - `zero_layers`: an empty PrintSimulation; envelope must serialise with an
//!   empty `layers` vec, and `report health --in` must render a sensible
//!   (mostly-zero) report without panicking.
//! - `single_layer`: a 1-layer aggregate; same expectations.
//!
//! Float-instability note: the baseline case uses synthesised LayerResults
//! with deterministic values rather than running the full physics pipeline,
//! so the golden bytes are stable across hardware. Physics drift is caught
//! separately by the env-gated CTB pipeline test (see
//! `RESINSIM_SLICED_FIXTURE` at `report_health_time_cli::report_health_sliced_ctb_json_shape`).
//!
//! Regenerating the goldens after an intentional schema change:
//!
//! ```sh
//! RESINSIM_REGENERATE_SIM_GOLDEN=1 cargo nextest run --no-capture sim_golden
//! ```
//!
//! Inspect the diff before committing — `git diff fixtures/sim_golden/` should
//! show only the change you intended.

use resinsim_core::entities::{LayerResult, PrinterProfile, ResinProfile};
use resinsim_core::repositories::{save_with_provenance, Provenance};
use resinsim_core::simulation::PrintSimulation;
use std::path::{Path, PathBuf};

const REGENERATE_ENV: &str = "RESINSIM_REGENERATE_SIM_GOLDEN";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sim_golden")
}

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("sim-golden-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: create tmp dir");
    dir
}

/// Synthesise a deterministic LayerResult for the baseline fixture. Values
/// are explicit constants — no float arithmetic that would be hardware-
/// dependent — so the produced sim.json bytes are stable across runs.
fn synth_layer(index: u32) -> LayerResult {
    LayerResult {
        index,
        cure_depth_um: 100.0,
        peel_force_n: 1.5,
        suction_force_n: 0.0,
        base_force_n: 0.0,
        peel_shape_factor: None,
        total_force_n: 1.5,
        support_capacity_n: 30.0,
        safety_factor: 20.0,
        cross_section_area_mm2: 100.0,
        area_delta_mm2: 0.0,
        vat_temperature_c: 22.0,
        viscosity_mpa_s: 200.0,
        z_deflection_um: 2.0,
        effective_layer_height_um: 48.0,
        worst_cure_depth_um: 100.0,
        strain_magnitude_max: None,
        stress_von_mises_max_mpa: None,
        strain_gradient_max_frac: None,
        voxel_yield_fraction: None,
        crack_front_fraction: None,
    }
}

fn baseline_fixture() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    for i in 0..5 {
        sim.add_layer(synth_layer(i), vec![])
            .expect("test fixture: sequential add_layer");
    }
    sim
}

fn zero_layer_fixture() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    PrintSimulation::new(recipe, printer)
}

fn single_layer_fixture() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    sim.add_layer(synth_layer(0), vec![])
        .expect("test fixture: index 0");
    sim
}

fn fixed_provenance() -> Provenance {
    Provenance {
        input_path: "fixtures/synthetic.ctb".into(),
        resin_name: "Generic Standard".into(),
        printer_name: "Generic MSLA 4K".into(),
        n_supports: 20,
        tip_radius_mm: 0.2,
    }
}

fn assert_or_regenerate(label: &str, sim: &PrintSimulation) {
    let dir = tmp_dir(label);
    let produced = dir.join(format!("{label}.sim.json"));
    save_with_provenance(&produced, sim, &fixed_provenance())
        .expect("save_with_provenance must succeed");
    let actual = std::fs::read_to_string(&produced).expect("read produced");

    let golden_path = fixtures_dir().join(format!("{label}.sim.json"));
    if std::env::var(REGENERATE_ENV).is_ok() {
        std::fs::create_dir_all(fixtures_dir()).expect("mkdir fixtures");
        std::fs::write(&golden_path, &actual).expect("write golden");
        eprintln!(
            "regenerated {} ({} bytes)",
            golden_path.display(),
            actual.len()
        );
        return;
    }

    let expected = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
        panic!(
            "missing golden fixture {} ({e}). \
             Regenerate with `{REGENERATE_ENV}=1 cargo nextest run --no-capture sim_golden`.",
            golden_path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "byte drift between produced sim.json and golden {}. \
         If the change is intentional, regenerate via \
         `{REGENERATE_ENV}=1 cargo nextest run --no-capture sim_golden`.",
        golden_path.display()
    );
}

#[test]
fn baseline_envelope_matches_golden() {
    assert_or_regenerate("baseline", &baseline_fixture());
}

#[test]
fn zero_layer_envelope_matches_golden() {
    assert_or_regenerate("zero_layers", &zero_layer_fixture());
}

#[test]
fn single_layer_envelope_matches_golden() {
    assert_or_regenerate("single_layer", &single_layer_fixture());
}

/// Optional env-gated 10000-layer smoke. Builds a synthetic 10000-layer
/// envelope (no CTB needed), serialises it via save_with_provenance, asserts
/// completion within a sane budget. Default-skipped to keep the suite fast.
#[test]
fn large_envelope_serialises_within_budget() {
    if std::env::var("RESINSIM_LARGE_SMOKE").is_err() {
        return;
    }
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    for i in 0..10_000u32 {
        sim.add_layer(synth_layer(i), vec![])
            .expect("test fixture: sequential add_layer");
    }
    let dir = tmp_dir("large_smoke");
    let path = dir.join("10k.sim.json");
    let start = std::time::Instant::now();
    save_with_provenance(&path, &sim, &fixed_provenance())
        .expect("save_with_provenance must succeed at 10k layers");
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "10k-layer save took {elapsed:?}; serialisation should remain quick"
    );
    let bytes = std::fs::metadata(&path)
        .expect("envelope file must exist")
        .len();
    assert!(
        bytes > 1_000_000,
        "10k-layer envelope must be > 1MB; got {bytes}"
    );
}
