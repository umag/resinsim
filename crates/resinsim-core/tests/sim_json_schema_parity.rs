//! Drift-guard parity test between Rust serde output and the canonical
//! zod-derived JSON Schema (`schemas/sim-json/v2.schema.json`).
//!
//! Per ADR-0015 the load-bearing risk is silent shape drift between two
//! sources of truth: Rust's `#[derive(Serialize)]` on `PrintSimulation` /
//! `SimulationEnvelope`, and the canonical zod schema in
//! `schemas/sim-json/v2.ts` (regenerated to `v2.schema.json`). Both must
//! produce / accept the same byte shape; this test fails CI loudly the
//! moment they disagree.
//!
//! ADR-0019 / t2f3.5: schema bumped 1 → 2 (clean break, no v1 compat).
//! The historical `v1.{ts,schema.json}` lives under `schemas/sim-json/archive/`.
//!
//! Three cases:
//!
//! - **Positive**: produce a known `SimulationEnvelope` via `save_to_path`,
//!   parse the JSON, validate against `v2.schema.json` — expect zero
//!   validation errors.
//! - **Negative**: tamper a single field (replace numeric `cure_depth_um`
//!   with a string), validate — expect a validation error.
//! - **Discriminant**: setting `schema_version` to anything other than 2
//!   must fail (the v2 schema's const:2 enforces this).

use boon::{Compiler, Schemas};
use resinsim_core::entities::{
    FailureEvent, FailureType, LayerResult, PrinterProfile, ResinProfile, Severity,
};
use resinsim_core::repositories::{save_with_provenance, Provenance};
use resinsim_core::simulation::PrintSimulation;
use std::path::{Path, PathBuf};

/// Inline LayerResult fixture — pub(crate) helpers in
/// `simulation/print_simulation.rs` are not visible to integration tests,
/// so we synthesise the same shape locally.
fn make_layer(index: u32, force_n: f32, safety_factor: f32, vat_temp_c: f32) -> LayerResult {
    LayerResult {
        index,
        cure_depth_um: 100.0,
        peel_force_n: force_n,
        suction_force_n: 0.0,
        base_force_n: 0.0,
        peel_shape_factor: None,
        total_force_n: force_n,
        support_capacity_n: force_n * safety_factor,
        safety_factor,
        cross_section_area_mm2: 100.0,
        area_delta_mm2: 0.0,
        vat_temperature_c: vat_temp_c,
        viscosity_mpa_s: 200.0,
        z_deflection_um: 2.0,
        effective_layer_height_um: 48.0,
        worst_cure_depth_um: 100.0,
        strain_magnitude_max: None,
        stress_von_mises_max_mpa: None,
        strain_gradient_max_frac: None,
        voxel_yield_fraction: None,
    }
}

/// Workspace-relative path to the canonical JSON Schema. Resolved from
/// `CARGO_MANIFEST_DIR` (which points at `crates/resinsim-core/`) so the
/// test runs from any nextest CWD.
fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("schemas")
        .join("sim-json")
        .join("v2.schema.json")
        .canonicalize()
        .expect("test fixture: schemas/sim-json/v2.schema.json exists at workspace root")
}

fn build_known_envelope() -> PrintSimulation {
    let recipe = ResinProfile::generic_standard().recipe().clone();
    let printer = PrinterProfile::generic_msla_4k();
    let mut sim = PrintSimulation::new(recipe, printer);
    sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
        .expect("test fixture: index 0 matches layer count 0");
    sim.add_layer(
        make_layer(1, 20.0, 0.8, 22.5),
        vec![FailureEvent {
            layer: 1,
            failure_type: FailureType::SupportOverload,
            severity: Severity::Critical,
            message: "fixture-only".into(),
        }],
    )
    .expect("test fixture: index 1 matches layer count 1");
    sim.add_layer(make_layer(2, 10.0, 2.0, 23.0), vec![])
        .expect("test fixture: index 2 matches layer count 2");
    sim
}

fn provenance() -> Provenance {
    Provenance {
        input_path: "fixture/path.ctb".into(),
        resin_name: "Generic Standard".into(),
        printer_name: "Linear Test Printer".into(),
        n_supports: 20,
        tip_radius_mm: 0.2,
    }
}

fn tmp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("sim-json-parity-{label}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("test setup: must create test_dir");
    dir
}

fn compile_v2_schema() -> (Schemas, boon::SchemaIndex) {
    let mut compiler = Compiler::new();
    let schema_url = format!(
        "file://{}",
        schema_path().to_str().expect("schema path is utf-8")
    );
    let mut schemas = Schemas::new();
    let id = compiler
        .compile(&schema_url, &mut schemas)
        .expect("v2.schema.json must compile");
    (schemas, id)
}

#[test]
fn fresh_envelope_validates_against_v2_schema() {
    let dir = tmp_dir("positive");
    let path = dir.join("known.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let bytes = std::fs::read_to_string(&path).expect("read written envelope");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse envelope JSON");

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "envelope produced by save_with_provenance must validate against v2.schema.json — \
             this means the Rust serde shape and schemas/sim-json/v2.ts have drifted. \
             Validation errors:\n{err}"
        )
    });
}

#[test]
fn envelope_validates_without_provenance() {
    // Optional `provenance` is allowed to be absent — covers the GUI
    // Save-Sim path that doesn't carry run-context metadata.
    let dir = tmp_dir("optional-provenance");
    let path = dir.join("no_provenance.sim.json");
    let sim = build_known_envelope();
    resinsim_core::repositories::save_to_path(&path, &sim).expect("save_to_path");
    let bytes = std::fs::read_to_string(&path).expect("read written envelope");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse envelope JSON");

    let (schemas, id) = compile_v2_schema();
    schemas
        .validate(&value, id)
        .expect("envelope without provenance must still validate");
}

#[test]
fn tampered_field_type_fails_v2_schema() {
    let dir = tmp_dir("negative");
    let path = dir.join("tampered.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read written envelope"))
            .expect("parse envelope JSON");

    // Tamper: replace numeric cure_depth_um on layer 0 with a string. The
    // schema's `LayerResultV2.cure_depth_um` requires `type: "number"` so
    // validation must fail.
    value["simulation"]["layers"][0]["cure_depth_um"] =
        serde_json::Value::String("not-a-number".into());

    let (schemas, id) = compile_v2_schema();
    let result = schemas.validate(&value, id);
    assert!(
        result.is_err(),
        "tampered cure_depth_um must fail v2.schema.json validation \
         (otherwise the schema is too loose to catch real drift)"
    );
}

#[test]
fn unknown_schema_version_fails_v2_schema() {
    // The literal(2) discriminator in v2.ts produces a `const: 2` JSON
    // Schema constraint. Any other schema_version (1, 3, 999) must fail
    // validation so consumers branching on the discriminant don't silently
    // mis-interpret a future shape.
    let dir = tmp_dir("future-version");
    let path = dir.join("future.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read written envelope"))
            .expect("parse envelope JSON");
    value["schema_version"] = serde_json::Value::Number(999.into());

    let (schemas, id) = compile_v2_schema();
    let result = schemas.validate(&value, id);
    assert!(
        result.is_err(),
        "schema_version=999 must fail v2.schema.json validation (const:2 enforces the discriminant)"
    );
}
