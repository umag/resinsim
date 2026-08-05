//! Step definitions for
//! `spec/uat/cli-report-health-layer-height-provenance.md` UAT-1..UAT-3
//! (uat-unskip-c2, plan step 3). Promotes the spec's three previously-
//! `Scenario (proposed):` fences to executable Gherkin in the same change
//! that lands this module (see the spec's diff: `` ```gherkin `` tags,
//! `Scenario:` keyword, joined Given lines, `<PATH>` -> `<PROVENANCE_PATH>`
//! rename).
//!
//! SYMBOL VERIFICATION. Every scenario drives `resinsim report health --in
//! <PROVENANCE_PATH>` — `cmd_report_health` (main.rs:1687) -> `load_envelope`
//! -> `ReportGenerator::text_format` (report_generator.rs:55), which reads
//! `sim.layer_height_provenance()` (print_simulation.rs:433, `#[cfg]`-free)
//! and renders via `values::layer_height_provenance::render_text_summary`
//! (layer_height_provenance.rs:261) plus `report_generator.rs`'s own
//! `MismatchKind::Variable` detail-line arm (report_generator.rs:110-115).
//! `values::layer_height_provenance` and `values::layer_height_seq` are
//! re-exported UNGATED from `values/mod.rs` (zero `cfg(feature` occurrences
//! in either module) — unlike the gated `StrainField`/`StressField`
//! re-exports increment A2 found in the same file. `ensure_resinsim_built`
//! (`cli_fixtures.rs:64-98`) builds the subprocessed binary with no
//! `--features`, so the binary under test is byte-identical under `cargo
//! uat` and `cargo uat-field-sim`.
//!
//! ENTRY POINT. Every scenario's Given produces a REAL envelope via a real
//! `resinsim sim` subprocess (`invoke_resinsim`, `cli_fixtures.rs`) against
//! shipped profiles, then INJECTS the scenario's
//! `simulation.layer_height_provenance` object through a parsed
//! `serde_json::Value` and writes it back with `to_string_pretty` — never a
//! hand-serialised envelope. This is the technique
//! `cli_sim_rejects_unknown_schema_version.rs` established
//! (`produce_real_sim_json` + `read_json`/`write_json`), not the spec's own
//! stale "Implementation notes" section (removed by this change), which
//! proposed assembling a fixture inline with `serde_json::json!`.
//!
//! WHY THE INJECTION IS SAFE (pre-empting a "fix" the module doc must head
//! off). `layer_height_provenance` is `#[serde(default,
//! skip_serializing_if = "Option::is_none")]` on `PrintSimulation`
//! (print_simulation.rs:140) — STL-path producer runs omit it entirely, so
//! injecting it is populating a field the real producer simply doesn't set
//! on this call path, not corrupting one it did set.
//! `PrintSimulation::validate` (print_simulation.rs:470-485) checks the
//! recipe, the printer, and per-layer index contiguity — it does NOT
//! cross-check `layer_height_provenance` against `layers.len()`. So
//! UAT-1's injected `layer_count=4492` on a real ~200-334-layer STL
//! envelope loads and validates cleanly; this is a deliberate
//! producer/consumer-shape simulation (the whole point of the spec — CTB
//! producers exist that this STL-only test tree cannot itself produce), not
//! a bug. Empirically probed against the real binary before this module was
//! written: exit 0 on all three injected shapes.
//!
//! "Never hand-serialized JSON": the envelope's OTHER fields (schema_version,
//! provenance, the full layers array, cure_kinetics_ea_is_default) are 100%
//! real `resinsim sim` output; only `simulation.layer_height_provenance` is
//! synthesised, and even that follows the wire shape
//! `LayerHeightProvenanceWire` documents
//! (`values/layer_height_provenance.rs`) rather than a guessed shape.
//!
//! ASSERTION LITERALS, copied from production, not retyped. µ is U+00B5;
//! the range dash in UAT-2 is an EN DASH, U+2013 (`"30.000–50.000 µm"`), not
//! a hyphen; the mismatch-suffix marker is `" ⚠"` (U+26A0) with its
//! significant leading space. `render_text_summary`
//! (layer_height_provenance.rs:261) emits `CTB layer_height: {ctb:.3} µm
//! (recipe: {recipe:.3} µm){suffix}` (uniform) and `CTB layer_height:
//! {min:.3}–{max:.3} µm (variable; mean {mean:.3} µm, recipe {recipe:.3}
//! µm){suffix}` (variable); `report_generator.rs`'s `MismatchKind::Variable`
//! arm adds the `    ⚠ adaptive slicing — recipe would imply N layers vs
//! the CTB's M …` detail line only when `mismatch.kind == Variable`, which
//! is why UAT-2's injected object must carry `mismatch: {"kind": "variable",
//! "recipe_layers_for_same_z": N}` and UAT-1/UAT-3's must OMIT `mismatch`
//! entirely (its absence is what keeps the " ⚠" suffix off). mean([30, 40,
//! 50, 40, 30]) is exactly 38.0. All three literals were verified against
//! the real binary via a throwaway probe (three injected shapes, three real
//! `report health --in` invocations) before this module was written, not
//! assumed from the source.
//!
//! FIXTURE TECHNIQUE NOTE — the legacy arm (UAT-3: `ctb_um` present,
//! `layer_count` absent) reconstructs a single-layer series in
//! `LayerHeightProvenance`'s custom `Deserialize`
//! (layer_height_provenance.rs:406-410), which is why UAT-3 renders through
//! the exact same uniform branch as UAT-1 and the two scenarios share one
//! render-line Then registration.
//!
//! REGEX DISTINCTNESS. The shared When, `` `resinsim report health --in
//! <PROVENANCE_PATH>` `` (backtick-delimited), uses the placeholder
//! `<PROVENANCE_PATH>` — NOT `<PATH>` — specifically because
//! `` `resinsim report health --in <PATH>` `` (same backtick delimiters) is
//! already registered by `cli_sim_rejects_unknown_schema_version.rs`.
//! Cucumber's regex match is exact-string-anchored (`^...$`), so reusing
//! `<PATH>` here would silently bind these three scenarios to that other
//! module's step function and its `World` expectations (a spec landing on
//! the wrong step-def is a distinctness edit to a never-executed *proposed*
//! scenario, not a weakening — the scenario's own Given always populates a
//! provenance-injected envelope regardless of the placeholder's name; only
//! the SPEC TEXT changed). None of the Then regexes below ("stdout reports
//! the CTB layer_height as ...", "stdout contains \"...\"", "stdout does
//! NOT contain the \" ⚠\" suffix") collide with any existing registration —
//! checked against the global step-def inventory (`grep -rh 'regex = r'
//! tests/uat_steps/*.rs`). `Then the process exits with code 0` (all three
//! scenarios) is `ctb_layer_height_authority.rs`'s shared registration —
//! reused, not re-registered; this module's Givens never populate
//! `world.sim_primary` / `world.last_sim_err`.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

/// Produce a real `sim.json` via a real `resinsim sim` subprocess against
/// shipped profiles (never hand-serialised), into a fresh `unique_tmp_dir`.
/// Same technique as `cli_sim_rejects_unknown_schema_version.rs`'s
/// `produce_real_sim_json`; duplicated locally (not shared) because that
/// module's helper is private to it — see `docs/patterns/anti/fixture-copy-
/// of-shared-builder.md`'s scope: this is a subprocess-invocation helper,
/// not a hand-copied resin/printer TOML literal, so the anti-pattern does
/// not apply.
fn produce_real_sim_json(tag: &str) -> std::path::PathBuf {
    let dir = unique_tmp_dir(tag);
    let data = workspace_data_dir();
    let stl = data.join("test_cube.stl");
    let out = dir.join("cube.sim.json");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            stl.to_str().expect("workspace STL path is UTF-8"),
            "--printer",
            "elegoo_mars5_ultra",
            "--resin",
            "elegoo_ceramic_grey_v2",
            "--n-supports",
            "0",
            "--data-dir",
            data.to_str().expect("workspace data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    assert!(
        outcome.exit_code == 0 && out.is_file(),
        "scenario fixture: real `resinsim sim` run must succeed; exit={} stderr={}",
        outcome.exit_code,
        outcome.stderr
    );
    out
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {} as JSON: {e}", path.display()))
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) {
    let text =
        serde_json::to_string_pretty(value).expect("serde_json::Value -> String cannot fail");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

// ---- UAT-1: Uniform shape (common case) ------------------------------------

#[given(
    regex = r"^a sim\.json file whose simulation\.layer_height_provenance carries ctb_um=40\.0, layer_count=4492, recipe_um=40\.0$"
)]
fn given_uniform_provenance(world: &mut UatWorld) {
    let path = produce_real_sim_json("provenance-uat1");
    let mut value = read_json(&path);
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_um": 40.0,
        "layer_count": 4492,
        "recipe_um": 40.0,
        // `mismatch` deliberately OMITTED — its absence is what keeps the
        // " ⚠" suffix off (LayerHeightProvenance::render_text_summary).
    });
    write_json(&path, &value);
    world.sim_json_path = Some(path);
}

// Shared When for all three scenarios in this spec.
#[when(regex = r"^the user invokes `resinsim report health --in <PROVENANCE_PATH>`$")]
fn when_invoke_report_health(world: &mut UatWorld) {
    let path = world
        .sim_json_path
        .clone()
        .expect("scenario invariant: Given populated sim_json_path");
    let outcome = invoke_resinsim(
        &[
            "report",
            "health",
            "--in",
            path.to_str().expect("sim.json path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with code 0` — ctb_layer_height_authority.rs's
// generalised then_exit_zero; no registration here (all three scenarios).

// Shared render-line Then for UAT-1 and UAT-3 (one registration, two
// occurrences) — both scenarios render the uniform form.
#[then(regex = r#"^stdout reports the CTB layer_height as "40\.000 µm \(recipe: 40\.000 µm\)"$"#)]
fn then_stdout_reports_uniform_line(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains("CTB layer_height: 40.000 µm (recipe: 40.000 µm)"),
        "expected the uniform render_text_summary line, got:\n{stdout}"
    );
}

#[then(regex = r#"^stdout does NOT contain the " ⚠" suffix$"#)]
fn then_stdout_does_not_contain_warn_suffix(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        !stdout.contains(" ⚠"),
        "expected NO ' ⚠' mismatch suffix (proves the no-mismatch arm ran, not merely that a \
         layer-height line exists), got:\n{stdout}"
    );
}

// ---- UAT-2: Variable shape with mismatch -----------------------------------

#[given(
    regex = r"^a sim\.json file whose simulation\.layer_height_provenance carries ctb_layer_heights_um=\[30\.0, 40\.0, 50\.0, 40\.0, 30\.0\], recipe_um=30\.0, mismatch with kind=variable$"
)]
fn given_variable_provenance_with_mismatch(world: &mut UatWorld) {
    let path = produce_real_sim_json("provenance-uat2");
    let mut value = read_json(&path);
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_layer_heights_um": [30.0, 40.0, 50.0, 40.0, 30.0],
        "recipe_um": 30.0,
        // recipe_layers_for_same_z is informational-only for the rendered
        // text (render_text_summary never reads it); report_generator.rs's
        // detail line does read it, so a real value keeps that line honest
        // rather than a placeholder. total_z_um = 190.0, / recipe 30.0 =
        // 6.33 -> round = 6 (matches LayerHeightProvenance::reconcile's own
        // rounding for this input, verified via a throwaway probe).
        "mismatch": {"kind": "variable", "recipe_layers_for_same_z": 6},
    });
    write_json(&path, &value);
    world.sim_json_path = Some(path);
}

// Shared When — see UAT-1's when_invoke_report_health.

// `Then the process exits with code 0` — shared registration, see UAT-1.

#[then(regex = r"^stdout reports the CTB layer_height as a variable range$")]
fn then_stdout_reports_variable_range(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // Real discrimination, not a bare substring: the rendered line must
    // carry the variable-form marker AND the en-dash range — proves the
    // Variable arm rendered, not the Uniform arm coincidentally containing
    // similar text.
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("CTB layer_height:"))
        .unwrap_or_else(|| panic!("expected a 'CTB layer_height:' line in stdout, got:\n{stdout}"));
    assert!(
        line.contains("variable;") && line.contains('–'),
        "expected the variable-form render (\"variable;\" + en dash U+2013), got: {line:?}"
    );
}

#[then(regex = r#"^stdout contains "30\.000–50\.000 µm"$"#)]
fn then_stdout_contains_range(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        // EN DASH, U+2013 — copied from layer_height_provenance.rs, not
        // retyped as a hyphen.
        stdout.contains("30.000–50.000 µm"),
        "expected the min–max range with the en dash, got:\n{stdout}"
    );
}

#[then(regex = r#"^stdout contains "mean 38\.000 µm"$"#)]
fn then_stdout_contains_mean(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        // mean([30.0, 40.0, 50.0, 40.0, 30.0]) is exactly 38.0.
        stdout.contains("mean 38.000 µm"),
        "expected the mean summary, got:\n{stdout}"
    );
}

#[then(regex = r#"^stdout contains the " ⚠" suffix$"#)]
fn then_stdout_contains_warn_suffix(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        stdout.contains(" ⚠"),
        "expected the ' ⚠' mismatch suffix, got:\n{stdout}"
    );
}

#[then(regex = r#"^stdout contains "adaptive slicing"$"#)]
fn then_stdout_contains_adaptive_slicing(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        // report_generator.rs's MismatchKind::Variable detail line only —
        // fires when `mismatch.kind == "variable"` is present.
        stdout.contains("adaptive slicing"),
        "expected the report_generator.rs Variable-arm detail line, got:\n{stdout}"
    );
}

// ---- UAT-3: Legacy single-`ctb_um` shape still loads -----------------------

#[given(
    regex = r"^a sim\.json file whose simulation\.layer_height_provenance carries ctb_um=40\.0, recipe_um=40\.0 \(no layer_count, no Vec\)$"
)]
fn given_legacy_provenance(world: &mut UatWorld) {
    let path = produce_real_sim_json("provenance-uat3");
    let mut value = read_json(&path);
    value["simulation"]["layer_height_provenance"] = serde_json::json!({
        "ctb_um": 40.0,
        "recipe_um": 40.0,
        // Deliberately no "layer_count" and no "ctb_layer_heights_um" — the
        // legacy shape LayerHeightProvenanceWire's Deserialize reconstructs
        // as a single-layer series (layer_height_provenance.rs:406-410).
    });
    write_json(&path, &value);
    world.sim_json_path = Some(path);
}

// Shared When — see UAT-1's when_invoke_report_health.

// `Then the process exits with code 0` — shared registration, see UAT-1.

// Shared render-line Then with UAT-1 — see then_stdout_reports_uniform_line.
