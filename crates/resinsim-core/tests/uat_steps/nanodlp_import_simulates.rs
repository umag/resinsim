//! Step definitions for `spec/uat/nanodlp-import-simulates.md` UAT-1..UAT-2
//! (uat-unskip-a3-b, plan step 5). First consumer of
//! `fixtures::NanoDlpJobBuilder` (UAT-2's K=2 / 5-layer job); UAT-1 uses the
//! committed `mini.nanodlp` (M=3 slice PNGs).
//!
//! SYMBOL VERIFICATION. UAT-1's entry points — the binary's `sim --file
//! --out` (`cmd_sim`, main.rs:1814-1960) -> `io::sliced::parse_sliced` ->
//! `io::nanodlp::parse_nanodlp` -> `SimulationRunner` ->
//! `repositories::save_with_provenance` — are all default-features;
//! `grep -n '#\[cfg(feature' io/nanodlp.rs io/sliced.rs` returns nothing.
//! UAT-2's `io::sliced::parse_sliced` (the exact dispatcher `cmd_sim` calls)
//! is the same symbol, so this module's single in-process exception carries
//! the same guarantee.
//!
//! BAND CORRECTION (2026-08-03 re-derivation). This spec's `When` clauses
//! subprocess the real binary for UAT-1 (`resinsim sim --file ...`), so
//! UAT-1 follows the Band C CLI shape through `uat_steps/cli_fixtures.rs`.
//! UAT-2's When ("the job is imported") is the SOLE in-process exception
//! inside the whole nanodlp trio — it calls `io::sliced::parse_sliced`
//! directly rather than spawning a subprocess.
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory. The
//! two nearest neighbours are `ctb_layer_height_authority.rs`'s `` the user
//! invokes `resinsim sim --file <CTB> --resin <RESIN> --printer <PRINTER>
//! --out <OUT>` `` and `sim_json_roundtrips_zero_force_layer.rs`'s `` the
//! user invokes "resinsim sim --file <PATH> --resin <R> --printer <P> --out
//! <OUT>" `` (double-quoted) — both textually distinct from this module's
//! `` the user invokes `resinsim sim --file <job.nanodlp> --out
//! out.sim.json` `` (backtick-quoted, no --resin/--printer placeholders,
//! different literal filename). "the job is imported" / "the first K layers
//! use the support (bottom) exposure time" / "subsequent layers use the
//! normal cure time" have no collision anywhere in the tree. Confirmed by
//! `cargo uat` landing this module with the expected per-spec row (not by
//! assumption alone).
//!
//! `--out` is MANDATORY on every `resinsim sim` invocation here:
//! `default_sim_out_path` would otherwise write `mini.sim.json` INTO
//! `tests/fixtures/`, polluting the committed fixture directory.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::{mini_nanodlp_path, NanoDlpJobBuilder};
use super::world::UatWorld;

/// Copied from `sim_json_roundtrips_zero_force_layer.rs::unique_sim_json_path`
/// (same concurrency rationale — cucumber runs scenarios within a feature
/// concurrently) rather than lifted into `cli_fixtures.rs`, to avoid
/// touching that already-shipped sibling module for this issue.
fn unique_sim_json_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("uat-sim-json-{tag}-{n}.sim.json"))
}

/// Copied from `sim_json_roundtrips_zero_force_layer.rs::parsed_sim_json` —
/// reads the file the real `save_with_provenance` call actually wrote
/// (`world.sim_json_path`), never a hand-built `Value`.
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

// ---- UAT-1: sim accepts a .nanodlp and writes a per-layer sim.json --------

#[given(regex = r"^a \.nanodlp job with M slice PNGs and NanoDLP profile/slicer/plate JSON$")]
fn given_nanodlp_job_with_m_slices(world: &mut UatWorld) {
    // Committed mini.nanodlp: M = 3 slice PNGs, verified sufficient.
    world.nanodlp_fixture_path = Some(mini_nanodlp_path());
}

#[when(regex = r"^the user invokes `resinsim sim --file <job\.nanodlp> --out out\.sim\.json`$")]
fn when_user_invokes_resinsim_sim_out(world: &mut UatWorld) {
    let fixture = world
        .nanodlp_fixture_path
        .clone()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let data_dir = workspace_data_dir();
    let out_path = unique_sim_json_path("nanodlp-import-uat1");
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "athena_ii",
            "--data-dir",
            data_dir.to_str().expect("data dir path is UTF-8"),
            "--out",
            out_path.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
    if outcome.exit_code == 0 {
        world.sim_json_path = Some(out_path);
    }
}

#[then(regex = r#"^stderr reports "Producing sim\.json from <job>"$"#)]
fn then_stderr_reports_producing_sim_json(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let fixture = world
        .nanodlp_fixture_path
        .as_ref()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    // main.rs:1905-1910 `eprintln!("Producing sim.json from {} using resin
    // '{}' + printer '{}'...", input_path.display(), ...)`.
    let needle = format!("Producing sim.json from {}", fixture.display());
    assert!(
        stderr.contains(&needle),
        "stderr must report {needle:?}, got: {stderr}"
    );
}

#[then(regex = r"^a sim\.json is written with M per-layer results$")]
fn then_sim_json_written_with_m_results(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let layers = value["simulation"]["layers"]
        .as_array()
        .expect("sim.json simulation.layers must be a JSON array");
    // mini.nanodlp's M = 3 slice PNGs.
    assert_eq!(layers.len(), 3, "expected M=3 per-layer results, got {value}");
}

#[then(regex = r"^each layer result carries a peel_force_n and a cross_section_area_mm2$")]
fn then_each_layer_carries_peel_force_and_area(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let layers = value["simulation"]["layers"]
        .as_array()
        .expect("sim.json simulation.layers must be a JSON array");
    for (i, l) in layers.iter().enumerate() {
        assert!(
            l["peel_force_n"].is_number(),
            "layer {i} missing a numeric peel_force_n: {l}"
        );
        let area = l["cross_section_area_mm2"].as_f64().unwrap_or_else(|| {
            panic!("layer {i} missing a numeric cross_section_area_mm2: {l}")
        });
        // mini's layers are 0.08 / 0.04 / 0.02 mm² — pinned by
        // src/io/nanodlp.rs::parse_fixture_layer_areas_and_z.
        assert!(
            area > 0.0,
            "layer {i} cross_section_area_mm2 must be > 0.0, got {area}"
        );
    }
}

#[then(regex = r"^the reported layer count equals the NanoDLP LayersCount$")]
fn then_reported_layer_count_equals_layers_count(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // cmd_sim's own `eprintln!("Wrote {} layers to {} in {}.", ...)`
    // (main.rs:1954).
    let stderr_count: usize = stderr
        .lines()
        .find_map(|l| l.strip_prefix("Wrote "))
        .and_then(|rest| rest.split(" layers to ").next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or_else(|| panic!("could not parse 'Wrote N layers to' from stderr: {stderr}"));

    // Second, independent production observation:
    // `inspect layers --file <job> --json` -> info.total_layers
    // (main.rs:1994-1997, sourced from SlicedFileInfo.total_layers, which
    // nanodlp.rs:272 sets from plate.layers_count).
    let fixture = world
        .nanodlp_fixture_path
        .as_ref()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "layers",
            "--file",
            fixture.to_str().expect("fixture path is UTF-8"),
            "--json",
        ],
        &[],
    );
    assert_eq!(
        outcome.exit_code, 0,
        "inspect layers --json must succeed; stderr={}",
        outcome.stderr
    );
    let info: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|e| panic!("inspect layers --json stdout must parse: {e}"));
    let total_layers = info["info"]["total_layers"]
        .as_u64()
        .unwrap_or_else(|| panic!("info.total_layers must be a JSON number, got {info}"));

    // Third surface: the sim.json actually written.
    let sim_value = parsed_sim_json(world);
    let sim_layers_len = sim_value["simulation"]["layers"]
        .as_array()
        .expect("sim.json simulation.layers must be a JSON array")
        .len();

    assert_eq!(
        stderr_count, total_layers as usize,
        "cmd_sim's stderr layer count must equal inspect layers --json info.total_layers"
    );
    assert_eq!(
        sim_layers_len, total_layers as usize,
        "sim.json layers length must equal inspect layers --json info.total_layers"
    );
}

// ---- UAT-2: NanoDLP recipe maps profile.json exposure and speeds ---------

#[given(regex = r"^a \.nanodlp whose profile\.json sets SupportLayerNumber = K$")]
fn given_nanodlp_support_layer_number_k(world: &mut UatWorld) {
    // K=2 / 5-layer job: mini.nanodlp's K=1-of-3 would make "the first K
    // layers" a single-element (degenerate) assertion, and
    // src/io/nanodlp.rs::parse_fixture_bottom_layer_gets_support_exposure
    // already pins that degenerate case. This builder call makes both "the
    // first K" and "subsequent" genuinely multi-layer claims.
    let job = NanoDlpJobBuilder::new()
        .with_support_layer_number(2)
        .with_lit_pixel_counts([16u64, 12, 8, 6, 4])
        .build("nanodlp-import-uat2");
    world.nanodlp_fixture_path = Some(job.path);
    world.imported_recipe_exposures = Some((
        job.support_layer_number,
        job.support_exposure_sec,
        job.normal_exposure_sec,
    ));
}

#[when(regex = r"^the job is imported$")]
fn when_the_job_is_imported(world: &mut UatWorld) {
    let fixture = world
        .nanodlp_fixture_path
        .clone()
        .expect("scenario invariant: Given step populated nanodlp_fixture_path");
    // The exact dispatcher cmd_sim itself calls — in-process, not a
    // subprocess, per this module's SYMBOL VERIFICATION note above.
    let (_info, layers) = resinsim_core::io::sliced::parse_sliced(&fixture)
        .unwrap_or_else(|e| panic!("synthesised nanodlp job must parse: {e}"));
    world.imported_layers = Some(layers);
}

#[then(regex = r"^the first K layers use the support \(bottom\) exposure time$")]
fn then_first_k_layers_use_support_exposure(world: &mut UatWorld) {
    let layers = world
        .imported_layers
        .as_ref()
        .expect("scenario invariant: When step populated imported_layers");
    let (k, support_exposure, _normal_exposure) = world
        .imported_recipe_exposures
        .expect("scenario invariant: Given step populated imported_recipe_exposures");
    let k = k as usize;
    assert!(
        k > 0 && k < layers.len(),
        "fixture invariant: K must be strictly between 0 and layer count for a \
         non-vacuous split; K={k}, layers={}",
        layers.len()
    );
    for l in &layers[..k] {
        assert_eq!(
            l.exposure_sec, support_exposure,
            "layer {} (< K={k}) must use the recorded support exposure",
            l.index
        );
    }
}

#[then(regex = r"^subsequent layers use the normal cure time$")]
fn then_subsequent_layers_use_normal_cure_time(world: &mut UatWorld) {
    let layers = world
        .imported_layers
        .as_ref()
        .expect("scenario invariant: When step populated imported_layers");
    let (k, _support_exposure, normal_exposure) = world
        .imported_recipe_exposures
        .expect("scenario invariant: Given step populated imported_recipe_exposures");
    let k = k as usize;
    for l in &layers[k..] {
        assert_eq!(
            l.exposure_sec, normal_exposure,
            "layer {} (>= K={k}) must use the recorded normal cure time",
            l.index
        );
    }
}
