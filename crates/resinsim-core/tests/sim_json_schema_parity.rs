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
//! Four families of case (13 tests total, not "three" — this doc drifted
//! from the file's growth; restated 2026-08, schemas-v2-missing-optional-fields):
//!
//! - **Positive**: produce a known `SimulationEnvelope` via `save_to_path` /
//!   `save_with_provenance` / `save_stamped`, parse the JSON, validate
//!   against `v2.schema.json` — expect zero validation errors.
//! - **Negative (tampered type)**: tamper a single field's type (e.g.
//!   replace numeric `cure_depth_um` with a string), validate — expect a
//!   validation error. This is the ONLY case shape that can catch a missing
//!   or wrong-typed declaration, because `additionalProperties: true` means
//!   a positive test cannot see a field the schema doesn't declare — see
//!   `tampered_peel_shape_factor_type_fails_v2_schema` and
//!   `tampered_layer_height_provenance_ctb_um_type_fails_v2_schema`, both
//!   added red-first ahead of their v2.ts declarations.
//! - **Negative (table-driven)**: `tampered_tier2_layer_result_optional_fields_fail_v2_schema`
//!   loops five same-shape `Option<f32>` fields with one assertion body
//!   rather than five near-identical tests.
//! - **Discriminant**: setting `schema_version` to anything other than 2
//!   must fail (the v2 schema's const:2 enforces this).
//!
//! Positive injection tests on a DECLARED union
//! (`injected_layer_height_provenance_*`) additionally guard against
//! over-strictness — a hazard a plain negative cannot see, and one the
//! JSON-Schema-side `additionalProperties: true` softening does not
//! mitigate for `required` or declared types.

use boon::{Compiler, Schemas};
use resinsim_core::entities::{
    FailureEvent, FailureType, LayerResult, PrinterProfile, ResinProfile, Severity,
};
use resinsim_core::repositories::{save_stamped, save_with_provenance, EnvelopeStamp, Provenance};
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
        crack_front_fraction: None,
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
        compute_device: None,
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
fn stamped_ea_default_true_envelope_validates_against_v2_schema() {
    // sim-json-envelope-ea-default-flag: the schema edit is otherwise
    // untestable via the positive case alone, because `additionalProperties:
    // true` means the committed schema validates the new field whether or
    // not it declares it. This positive case at least confirms the happy
    // path stays green through the new field's addition; the negative case
    // below (tampered_ea_flag_type_fails_v2_schema) is what actually ties
    // the JSON Schema edit to CI.
    let dir = tmp_dir("positive-ea-stamped");
    let path = dir.join("stamped.sim.json");
    let sim = build_known_envelope();
    let prov = provenance();
    let stamp = EnvelopeStamp {
        provenance: Some(&prov),
        cure_kinetics_ea_is_default: Some(true),
    };
    save_stamped(&path, &sim, stamp).expect("save_stamped");
    let bytes = std::fs::read_to_string(&path).expect("read written envelope");
    let value: serde_json::Value = serde_json::from_str(&bytes).expect("parse envelope JSON");

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "envelope produced by save_stamped with cure_kinetics_ea_is_default=Some(true) \
             must validate against v2.schema.json. Validation errors:\n{err}"
        )
    });
}

#[test]
fn tampered_ea_flag_type_fails_v2_schema() {
    // The only assertion that can fail because of the v2.schema.json edit
    // for this field — `additionalProperties: true` means the positive
    // case above passes whether or not the schema declares
    // `cure_kinetics_ea_is_default`. This is what ties the schema edit to
    // CI. Modelled on tampered_field_type_fails_v2_schema above.
    let dir = tmp_dir("negative-ea-flag");
    let path = dir.join("tampered-ea-flag.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read written envelope"))
            .expect("parse envelope JSON");

    // Tamper: inject a wrong-typed cure_kinetics_ea_is_default (string
    // instead of boolean). The schema's declared `{"type": "boolean"}"`
    // requires validation to fail.
    value["cure_kinetics_ea_is_default"] = serde_json::Value::String("yes".into());

    let (schemas, id) = compile_v2_schema();
    let result = schemas.validate(&value, id);
    assert!(
        result.is_err(),
        "tampered cure_kinetics_ea_is_default (string instead of boolean) must fail \
         v2.schema.json validation (otherwise the schema doesn't declare the field's type)"
    );
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
fn tampered_peel_shape_factor_type_fails_v2_schema() {
    // schemas-v2-missing-optional-fields, red-first. `peel_shape_factor` is
    // undeclared on today's v2.ts/v2.schema.json — `additionalProperties:
    // true` means a positive test cannot see that gap, so this negative is
    // the only thing that ties the coming schema edit to the battery.
    // Modelled on tampered_ea_flag_type_fails_v2_schema.
    let dir = tmp_dir("negative-peel-shape-factor");
    let path = dir.join("tampered-peel-shape-factor.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read written envelope"))
            .expect("parse envelope JSON");

    // Tamper: inject a wrong-typed peel_shape_factor (string instead of
    // number) on layer 0.
    value["simulation"]["layers"][0]["peel_shape_factor"] =
        serde_json::Value::String("wide".into());

    let (schemas, id) = compile_v2_schema();
    let result = schemas.validate(&value, id);
    assert!(
        result.is_err(),
        "tampered peel_shape_factor (string instead of number) must fail v2.schema.json \
         validation (otherwise the schema doesn't declare the field's type)"
    );
}

#[test]
fn tampered_layer_height_provenance_ctb_um_type_fails_v2_schema() {
    // schemas-v2-missing-optional-fields, red-first. `layer_height_provenance`
    // is undeclared on today's v2.ts/v2.schema.json — same rationale as
    // tampered_peel_shape_factor_type_fails_v2_schema above. Injects the
    // whole uniform-shape provenance object (literal copied from
    // cli_report_health_layer_height_provenance.rs's UAT-1
    // given_uniform_provenance, not invented) then tampers `ctb_um`.
    let dir = tmp_dir("negative-layer-height-provenance");
    let path = dir.join("tampered-layer-height-provenance.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read written envelope"))
            .expect("parse envelope JSON");

    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_um": "wide",
        "layer_count": 4492,
        "recipe_um": 40.0,
    });

    let (schemas, id) = compile_v2_schema();
    let result = schemas.validate(&value, id);
    assert!(
        result.is_err(),
        "tampered layer_height_provenance.ctb_um (string instead of number) must fail \
         v2.schema.json validation (otherwise the schema doesn't declare the field's shape)"
    );
}

/// Save + read + parse a known envelope, for the NEW provenance /
/// peel_shape_factor positive tests only. The six pre-existing tests above
/// keep their own inline boilerplate — this helper is additive, not a
/// refactor of them.
fn save_and_read_known_envelope(label: &str) -> serde_json::Value {
    let dir = tmp_dir(label);
    let path = dir.join("known.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let bytes = std::fs::read_to_string(&path).expect("read written envelope");
    serde_json::from_str(&bytes).expect("parse envelope JSON")
}

#[test]
fn injected_layer_height_provenance_uniform_validates_v2_schema() {
    // Over-strictness guard: a positive test on an UNDECLARED field was
    // vacuous under additionalProperties:true; now that
    // layer_height_provenance is a union with required keys, this is the
    // only thing that catches an over-strict declaration. Literal copied
    // from cli_report_health_layer_height_provenance.rs's UAT-1
    // given_uniform_provenance — not invented.
    let mut value = save_and_read_known_envelope("positive-provenance-uniform");
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_um": 40.0,
        "layer_count": 4492,
        "recipe_um": 40.0,
    });

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "uniform layer_height_provenance (ctb_um + layer_count + recipe_um) must validate \
             against v2.schema.json. Validation errors:\n{err}"
        )
    });
}

#[test]
fn injected_layer_height_provenance_legacy_shape_validates_v2_schema() {
    // Legacy shape: ctb_um present, layer_count ABSENT — the Rust reader's
    // fall-through (layer_height_provenance.rs:406-410) reconstructs this as
    // a single-layer series and UAT-3 exercises it. A required layer_count
    // would wrongly reject this real accepted shape; that's exactly the
    // over-strictness this positive catches. Literal copied from UAT-3's
    // given_legacy_provenance.
    let mut value = save_and_read_known_envelope("positive-provenance-legacy");
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_um": 40.0,
        "recipe_um": 40.0,
    });

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "legacy layer_height_provenance (ctb_um + recipe_um, no layer_count) must validate \
             against v2.schema.json. Validation errors:\n{err}"
        )
    });
}

#[test]
fn injected_layer_height_provenance_variable_validates_v2_schema() {
    // Variable / adaptive-slicing shape with a variable-kind mismatch.
    // Literal copied from UAT-2's given_variable_provenance_with_mismatch.
    let mut value = save_and_read_known_envelope("positive-provenance-variable");
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_layer_heights_um": [30.0, 40.0, 50.0, 40.0, 30.0],
        "recipe_um": 30.0,
        "mismatch": {"kind": "variable", "recipe_layers_for_same_z": 6},
    });

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "variable layer_height_provenance with a variable-kind mismatch must validate \
             against v2.schema.json. Validation errors:\n{err}"
        )
    });
}

#[test]
fn injected_peel_shape_factor_validates_v2_schema() {
    let mut value = save_and_read_known_envelope("positive-peel-shape-factor");
    value["simulation"]["layers"][0]["peel_shape_factor"] = serde_json::json!(0.85);

    let (schemas, id) = compile_v2_schema();
    schemas.validate(&value, id).unwrap_or_else(|err| {
        panic!(
            "peel_shape_factor=0.85 on layer 0 must validate against v2.schema.json. \
             Validation errors:\n{err}"
        )
    });
}

#[test]
fn tampered_tier2_layer_result_optional_fields_fail_v2_schema() {
    // schemas-v2-missing-optional-fields, scope B (step 10). All five are
    // plain Option<f32> with an unambiguous JSON-number wire shape
    // (layer_result.rs), so one table-driven negative covers all five
    // without five near-identical flat tests. Field name goes in the
    // assert! MESSAGE literal, not matched against boon's error Display
    // (unpinned format).
    const FIELDS: &[&str] = &[
        "strain_magnitude_max",
        "stress_von_mises_max_mpa",
        "strain_gradient_max_frac",
        "voxel_yield_fraction",
        "crack_front_fraction",
    ];
    let dir = tmp_dir("negative-tier2-optional-fields");
    let path = dir.join("known.sim.json");
    let sim = build_known_envelope();
    save_with_provenance(&path, &sim, &provenance()).expect("save_with_provenance");
    let base_bytes = std::fs::read_to_string(&path).expect("read written envelope");

    for field in FIELDS {
        let mut value: serde_json::Value =
            serde_json::from_str(&base_bytes).expect("parse envelope JSON");
        value["simulation"]["layers"][0][*field] = serde_json::Value::String("bad".into());

        let (schemas, id) = compile_v2_schema();
        let result = schemas.validate(&value, id);
        assert!(
            result.is_err(),
            "tampered {field} (string instead of number) must fail v2.schema.json validation"
        );
    }
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
