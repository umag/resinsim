//! Step definitions for `spec/uat/cli-sim-producer-writes-sim-json.md`
//! UAT-1..UAT-6 (uat-unskip-c1, plan step 5).
//!
//! SYMBOL VERIFICATION. `resinsim-core::tests::uat_steps::cli_fixtures::
//! ensure_resinsim_built` (`cli_fixtures.rs:64-98`) builds the `resinsim`
//! binary with `--bin resinsim -p resinsim-inspect` and **no `--features`**
//! — the binary every scenario here subprocesses is always default-features,
//! byte-identical under `cargo uat` and `cargo uat-field-sim`. Every
//! production entry point these six scenarios touch is itself default-
//! features (`#[cfg]`-free) on that call path:
//!   - `cmd_sim` (main.rs:1820) — no `#[cfg]` on the function.
//!   - `profile_loader::resolve_profiles` (profile_loader.rs:176) and its
//!     callees `load_resin` / `load_printer` / `format_load_error` — no
//!     `#[cfg]` anywhere in `profile_loader.rs`.
//!   - `run_simulation_with_optional_voxel` (main.rs:1750) — its voxel
//!     branch is `#[cfg(feature = "field-sim")]`, but no scenario here
//!     passes `--voxel-cure-mm`, so `voxel_cure_mm` is always `None` and
//!     every invocation falls straight through to the Tier-1
//!     `SimulationRunner::run_auto` call, which is itself `#[cfg]`-free.
//!   - `save_stamped` (simulation_repo.rs:322) — `#[cfg]`-free; the
//!     sidecar-encode branch inside `save_envelope_to_path` is
//!     `#[cfg(feature = "field-sim")]` but is only reached when the
//!     `PrintSimulation` carries a voxel field, which Tier-1 runs never
//!     produce (same reasoning as `sim_json_roundtrips_zero_force_layer.rs`'s
//!     SYMBOL VERIFICATION block for the sidecar branch).
//!   - `default_sim_out_path` (main.rs:1799) — `#[cfg]`-free.
//!
//! So this module is safe to land as a single register-entry removal, same
//! shape as `sim_json_roundtrips_zero_force_layer.rs`.
//!
//! ENTRY POINT. Every scenario here drives the REAL `resinsim` binary via
//! `invoke_resinsim` (`cli_fixtures.rs`) — never an in-process call — since
//! the spec's own subject is the CLI subcommand's contract end to end
//! (argument parsing, the ADR-0015 atomic write, the stderr wrapper
//! strings). `--out` is passed on every invocation EXCEPT UAT-2, whose
//! entire subject is the derived default (documented exception at that
//! scenario's When). Every fixture path is rooted in its own unique
//! directory via `fixtures::unique_tmp_dir` — cucumber runs scenarios
//! within a feature concurrently, so a fixed filename would race.
//!
//! "Never hand-serialized JSON": every `sim.json` on disk here is produced
//! by a real `resinsim sim` subprocess; test-side assertions parse it back
//! with `serde_json::Value`, matching this file's sibling modules'
//! convention.
//!
//! REGEX DISTINCTNESS. Checked against the global step-def inventory
//! (`grep -rh 'regex = r' tests/uat_steps/*.rs`). This module's six Whens
//! are all backtick-delimited `` `resinsim sim --stl ...` `` — never
//! `--file` — with mutually distinct tails (UAT-1's full happy-path flags,
//! UAT-2's `.../widget.stl ...` without `--out`, UAT-3's
//! `--stl <PATH> ... --out /tmp/cube.sim.json`, UAT-4's literal
//! `/no/such/file.stl`, UAT-5's `--resin no_such_resin`, UAT-6's
//! `.../blocked/inner.sim.json`), so no two collide with each other or with
//! `ctb_layer_height_authority.rs`'s `` `resinsim sim --file <CTB> ...` ``
//! (different flag, different placeholder names) or
//! `sim_json_roundtrips_zero_force_layer.rs`'s double-quoted
//! `"resinsim sim --file <PATH> ..."`. The literal `...` in UAT-2/UAT-3/
//! UAT-6's spec text is escaped `\.\.\.` in the regex below — unescaped it
//! is a wildcard and could make two Whens ambiguous.
//!
//! `^the process exits with non-zero code$` (UAT-4/5/6, one shared
//! registration) is ONE WORD away from
//! `cli_temperature_flag_validation.rs:108`'s
//! `^the process exits with a non-zero code$` (that one keeps the article
//! "a"); a copy-paste that drops or adds it would produce a runtime
//! ambiguity, not a compile error. It is also textually distinct from
//! `:53`'s `^the process exits with a non-zero code \(2\)$`, `:163`'s
//! `^the process exits with code 2$`,
//! `sim_json_roundtrips_zero_force_layer.rs:204`'s
//! `^the process exits 0$`, `cli_profile_by_name_loading.rs:228`'s
//! `^the binary exits non-zero$` / `:372`'s
//! `^the binary exits successfully$`, `cli_requires_resin_for_recipe_fields.rs:170`'s
//! `^the subcommand exits 0$`, and the two `cli_inspect_field_slices_voxel_field.rs`
//! field-subcommand variants (`:83`, `:123`). This registration is ALSO
//! reused by `cli_sim_rejects_unknown_schema_version.rs` (pointer comment
//! there).
//!
//! STEPS OWNED ELSEWHERE. `^the process exits with code 0$` (UAT-1, UAT-3)
//! is owned by `ctb_layer_height_authority.rs:165`'s `then_exit_zero`,
//! generalised (uat-unskip-c1 step 3) with an observation-mode XOR guard so
//! it can serve both that module's in-process scenarios and this module's
//! CLI-subprocess ones. A second registration of the identical regex here
//! would be a runtime ambiguous-match error.

use cucumber::{given, then, when};

use super::cli_fixtures::{invoke_resinsim, workspace_data_dir};
use super::fixtures::unique_tmp_dir;
use super::world::UatWorld;

/// UAT-3's pre-existing-content sentinel — module-level so both the Given
/// that writes it and the Then that asserts its absence share one literal.
const SENTINEL: &str = "arbitrary pre-existing content — uat-unskip-c1 UAT-3 sentinel";

/// Parse the `sim.json` at `world.sim_json_path` as a generic
/// `serde_json::Value` — reads a file the real binary actually wrote;
/// never a hand-built `Value`. Same idiom as
/// `sim_json_roundtrips_zero_force_layer.rs::parsed_sim_json`.
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

/// Copy `data/test_cube.stl` to `<dir>/<name>` — the one fixture-copy site
/// every scenario below shares (never a second literal STL path).
fn copy_test_cube(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dest = dir.join(name);
    std::fs::copy(workspace_data_dir().join("test_cube.stl"), &dest)
        .unwrap_or_else(|e| panic!("copy test_cube.stl to {}: {e}", dest.display()));
    dest
}

// ---- UAT-1: Happy path ------------------------------------------------------

#[given(regex = r"^a sliced or STL input file$")]
fn given_sliced_or_stl_input_file(world: &mut UatWorld) {
    let dir = unique_tmp_dir("producer-uat1");
    let input = copy_test_cube(&dir, "cube.stl");
    world.cli_tmp_dir = Some(dir);
    world.sim_input_path = Some(input);
}

#[given(regex = r"^shipped resin and printer profiles$")]
fn given_shipped_resin_and_printer_profiles(_world: &mut UatWorld) {
    let data = workspace_data_dir();
    let resin = data.join("resins").join("generic_standard.toml");
    let printer = data.join("printers").join("generic_msla_4k.toml");
    assert!(
        resin.is_file(),
        "expected shipped resin profile at {}",
        resin.display()
    );
    assert!(
        printer.is_file(),
        "expected shipped printer profile at {}",
        printer.display()
    );
}

#[when(
    regex = r"^the user invokes `resinsim sim --stl <PATH> --resin generic_standard --printer generic_msla_4k --out cube\.sim\.json`$"
)]
fn when_invoke_sim_uat1(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("Given populated cli_tmp_dir");
    let input = world
        .sim_input_path
        .clone()
        .expect("Given populated sim_input_path");
    let out = dir.join("cube.sim.json");
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
    world.sim_json_path = Some(out);
}

// `Then the process exits with code 0` — served by
// ctb_layer_height_authority.rs's generalised then_exit_zero; no
// registration here (UAT-1, UAT-3).

#[then(regex = r"^cube\.sim\.json exists at the requested path$")]
fn then_cube_sim_json_exists(world: &mut UatWorld) {
    let out = world
        .sim_json_path
        .clone()
        .expect("When populated sim_json_path");
    assert!(out.is_file(), "expected {} to exist", out.display());
    // Second observation: main.rs:1966-1971's
    // `"Wrote {} layers to {} in {}."` stderr line names the same path.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let out_str = out.to_str().expect("out path is UTF-8");
    assert!(
        stderr.contains("Wrote ") && stderr.contains(out_str),
        "expected stderr's 'Wrote N layers to <path>' line to name {out_str}, got: {stderr}"
    );
}

#[then(
    regex = r"^the file is valid JSON with top-level fields schema_version, simulation, provenance$"
)]
fn then_file_is_valid_json_with_top_level_fields(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    for key in ["schema_version", "simulation", "provenance"] {
        assert!(
            value.get(key).is_some(),
            "expected top-level field {key:?} in the produced envelope, got: {value}"
        );
    }
}

#[then(regex = r"^schema_version equals 2$")]
fn then_schema_version_equals_2(world: &mut UatWorld) {
    // simulation_repo.rs:59 CURRENT_SCHEMA_VERSION is the authority; ADR-0019
    // bumped it 1 -> 2 as a deliberate clean break. The spec literal was
    // corrected to match (uat-unskip-c1 step 2) rather than weakened.
    let value = parsed_sim_json(world);
    assert_eq!(
        value["schema_version"], 2,
        "expected schema_version == CURRENT_SCHEMA_VERSION (2), got: {}",
        value["schema_version"]
    );
}

#[then(regex = r"^provenance\.input_path equals the input path$")]
fn then_provenance_input_path_equals(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let expected = world
        .sim_input_path
        .as_ref()
        .expect("When populated sim_input_path");
    let expected_str = expected.to_str().expect("input path is UTF-8");
    assert_eq!(
        value["provenance"]["input_path"].as_str(),
        Some(expected_str),
        "expected provenance.input_path to equal the exact --stl argument {expected_str:?}, got: {}",
        value["provenance"]["input_path"]
    );
    // Second observation: main.rs:1911-1916's "Producing sim.json from {}
    // using resin '{}' + printer '{}'..." stderr line names the same path.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains(&format!("Producing sim.json from {expected_str}")),
        "expected stderr's producing-line to name {expected_str}, got: {stderr}"
    );
}

#[then(regex = r#"^provenance\.resin_name equals "Generic Standard"$"#)]
fn then_provenance_resin_name_equals(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    assert_eq!(
        value["provenance"]["resin_name"].as_str(),
        Some("Generic Standard"),
        "got: {}",
        value["provenance"]["resin_name"]
    );
    // Second observation: the same stderr line carries `using resin
    // 'Generic Standard'`.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("using resin 'Generic Standard'"),
        "expected stderr to name the resin, got: {stderr}"
    );
}

#[then(regex = r#"^provenance\.printer_name equals "Generic MSLA 4K"$"#)]
fn then_provenance_printer_name_equals(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    assert_eq!(
        value["provenance"]["printer_name"].as_str(),
        Some("Generic MSLA 4K"),
        "got: {}",
        value["provenance"]["printer_name"]
    );
    // Second observation: the same stderr line carries `+ printer 'Generic
    // MSLA 4K'`.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("+ printer 'Generic MSLA 4K'"),
        "expected stderr to name the printer, got: {stderr}"
    );
}

#[then(regex = r"^the simulation block has non-empty layers array$")]
fn then_simulation_block_has_nonempty_layers(world: &mut UatWorld) {
    let value = parsed_sim_json(world);
    let layers = value["simulation"]["layers"]
        .as_array()
        .expect("simulation.layers must be a JSON array");
    assert!(!layers.is_empty(), "expected a non-empty layers array");
    // Second observation: parse N out of stderr's "Wrote {N} layers to ..."
    // line and assert it equals layers.len().
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let wrote_n = stderr
        .lines()
        .find_map(|l| l.trim().strip_prefix("Wrote "))
        .and_then(|rest| rest.split(" layers to").next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or_else(|| {
            panic!("could not parse layer count from stderr's 'Wrote N layers to' line: {stderr}")
        });
    assert_eq!(
        wrote_n,
        layers.len(),
        "stderr's reported layer count ({wrote_n}) must equal simulation.layers.len() ({})",
        layers.len()
    );
}

// ---- UAT-2: Default --out derived from input stem ---------------------------

#[given(regex = r"^a sliced input at /tmp/work/widget\.stl$")]
fn given_sliced_input_at_widget_stl(world: &mut UatWorld) {
    // Spec's `/tmp/work` is illustrative; substituted with a
    // CARGO_TARGET_TMPDIR-rooted unique dir (same idiom as
    // cli_profile_by_name_loading.rs's `<data-dir>` substitution). This
    // scenario needs its OWN unique dir — its entire subject is the
    // derived default `--out`, which must not collide with a sibling
    // scenario's `cube.sim.json`.
    let dir = unique_tmp_dir("producer-uat2");
    let input = copy_test_cube(&dir, "widget.stl");
    world.cli_tmp_dir = Some(dir);
    world.sim_input_path = Some(input);
}

#[when(
    regex = r"^the user invokes `resinsim sim --stl /tmp/work/widget\.stl \.\.\.` without --out$"
)]
fn when_invoke_sim_uat2_no_out(world: &mut UatWorld) {
    let input = world
        .sim_input_path
        .clone()
        .expect("Given populated sim_input_path");
    let data = workspace_data_dir();
    // Deliberately NO --out — documented exception to the always-pass-
    // --out rule (module doc); this scenario's whole subject is the
    // derived default.
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the process produces /tmp/work/widget\.sim\.json$")]
fn then_process_produces_widget_sim_json(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("Given populated cli_tmp_dir");
    // default_sim_out_path (main.rs:1799-1817): input parent + <stem>.sim.json.
    let expected = dir.join("widget.sim.json");
    assert!(
        expected.is_file(),
        "expected the derived default output {} to exist",
        expected.display()
    );
    // Second observation: the "Wrote {} layers to {} in {}." stderr line
    // names this exact derived path — the assertion does not merely
    // re-implement the derivation rule.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    let expected_str = expected.to_str().expect("expected path is UTF-8");
    assert!(
        stderr.contains(expected_str),
        "expected stderr to name the derived default path {expected_str}, got: {stderr}"
    );
    // Cross-check: <uniq>/cube.sim.json does NOT exist — proves the stem
    // came from the input filename (widget), not a constant.
    let not_cube = dir.join("cube.sim.json");
    assert!(
        !not_cube.exists(),
        "cube.sim.json must not exist in this scenario's directory — the derived stem must \
         come from the input filename, not a hardcoded constant"
    );
}

// ---- UAT-3: Existing --out overwritten silently -----------------------------

#[given(regex = r"^an existing file at /tmp/cube\.sim\.json with arbitrary content$")]
fn given_existing_file_at_cube_sim_json(world: &mut UatWorld) {
    let dir = unique_tmp_dir("producer-uat3");
    let target = dir.join("cube.sim.json");
    std::fs::write(&target, SENTINEL)
        .unwrap_or_else(|e| panic!("write sentinel to {}: {e}", target.display()));
    world.cli_tmp_dir = Some(dir);
    world.sim_json_path = Some(target);
}

#[when(regex = r"^the user invokes `resinsim sim --stl <PATH> \.\.\. --out /tmp/cube\.sim\.json`$")]
fn when_invoke_sim_uat3_overwrite(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("Given populated cli_tmp_dir");
    let input = copy_test_cube(&dir, "cube.stl");
    world.sim_input_path = Some(input.clone());
    let out = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path (the pre-existing target)");
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with code 0` — served by
// ctb_layer_height_authority.rs's generalised then_exit_zero (UAT-1, UAT-3).

#[then(
    regex = r"^/tmp/cube\.sim\.json contains the freshly produced envelope \(the old content is gone\)$"
)]
fn then_cube_sim_json_freshly_produced(world: &mut UatWorld) {
    let out = world
        .sim_json_path
        .clone()
        .expect("Given populated sim_json_path");
    let bytes = std::fs::read_to_string(&out)
        .unwrap_or_else(|e| panic!("read {}: {e}", out.display()));
    // (a) the sentinel string is absent from the new bytes.
    assert!(
        !bytes.contains(SENTINEL),
        "expected the pre-existing sentinel content to be gone from {}",
        out.display()
    );
    // (b) the bytes parse as JSON with schema_version == 2 and a
    // provenance block whose input_path equals the --stl argument.
    let value: serde_json::Value = serde_json::from_str(&bytes)
        .unwrap_or_else(|e| panic!("parse {} as JSON: {e}", out.display()));
    assert_eq!(
        value["schema_version"], 2,
        "got: {}",
        value["schema_version"]
    );
    let expected_input = world
        .sim_input_path
        .as_ref()
        .expect("When populated sim_input_path");
    let expected_input_str = expected_input.to_str().expect("input path is UTF-8");
    assert_eq!(
        value["provenance"]["input_path"].as_str(),
        Some(expected_input_str),
        "got: {}",
        value["provenance"]["input_path"]
    );
    // (c) no orphan <out>.tmp sibling survives (simulation_repo.rs's
    // tmp_sibling appends ".tmp" to the file name; ADR-0015 atomic-write
    // contract).
    let mut tmp_name = out
        .file_name()
        .expect("out path has a file name")
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp_sibling = out.with_file_name(tmp_name);
    assert!(
        !tmp_sibling.exists(),
        "expected no orphan .tmp sibling at {}",
        tmp_sibling.display()
    );
}

// ---- UAT-4: Missing input file hard-errors ----------------------------------

#[when(regex = r"^the user invokes `resinsim sim --stl /no/such/file\.stl --out /tmp/x\.sim\.json`$")]
fn when_invoke_sim_uat4_missing_stl(world: &mut UatWorld) {
    let dir = unique_tmp_dir("producer-uat4");
    let out = dir.join("x.sim.json");
    world.cli_tmp_dir = Some(dir);
    world.sim_json_path = Some(out.clone());
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            "/no/such/file.stl",
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the process exits with non-zero code$")]
fn then_exits_with_non_zero_code(world: &mut UatWorld) {
    // New registration (uat-unskip-c1) — shared by this module's UAT-4/
    // UAT-5/UAT-6 and by cli_sim_rejects_unknown_schema_version.rs's four
    // scenarios (pointer comment there). See the module's REGEX
    // DISTINCTNESS block for the one-word-different neighbour.
    let exit = world.cli_exit_code.expect("When populated cli_exit_code");
    assert_ne!(
        exit, 0,
        "expected a non-zero exit code; stderr={:?}",
        world.cli_stderr
    );
}

#[then(regex = r#"^stderr mentions "/no/such/file\.stl"$"#)]
fn then_stderr_mentions_no_such_file(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("/no/such/file.stl"),
        "expected stderr to mention the missing path, got: {stderr}"
    );
    // Discrimination cross-check: the run-failure branch fired (main.rs
    // "Error producing sim.json from {}: {e}"), not the profile-resolution
    // branch (main.rs:1866-1869) or the out-parent branch
    // (main.rs:1902-1906).
    assert!(
        stderr.contains("Error producing sim.json from"),
        "expected the run-failure wrapper to have fired, got: {stderr}"
    );
    assert!(
        !stderr.contains("cannot prepare --out parent directory"),
        "must not be the out-parent branch: {stderr}"
    );
    assert!(
        !stderr.contains("Available profiles"),
        "must not be the profile-resolution branch: {stderr}"
    );
    // <uniq>/x.sim.json was NOT created.
    let out = world
        .sim_json_path
        .as_ref()
        .expect("When populated sim_json_path");
    assert!(
        !out.exists(),
        "expected {} to not be created on failure",
        out.display()
    );
}

// ---- UAT-5: Unknown --resin hard-errors with available list ----------------

#[when(
    regex = r"^the user invokes `resinsim sim --stl <PATH> --resin no_such_resin --printer generic_msla_4k --out /tmp/x\.sim\.json`$"
)]
fn when_invoke_sim_uat5_unknown_resin(world: &mut UatWorld) {
    let dir = unique_tmp_dir("producer-uat5");
    let input = copy_test_cube(&dir, "cube.stl");
    let out = dir.join("x.sim.json");
    world.cli_tmp_dir = Some(dir);
    world.sim_input_path = Some(input.clone());
    world.sim_json_path = Some(out.clone());
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "no_such_resin",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with non-zero code` — shared registration, see
// UAT-4's `then_exits_with_non_zero_code`.

#[then(regex = r#"^stderr contains "no_such_resin"$"#)]
fn then_stderr_contains_no_such_resin(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("no_such_resin"),
        "expected stderr to echo the bogus resin name, got: {stderr}"
    );
    // Discrimination: resolve_profiles loads the resin FIRST
    // (profile_loader.rs:181-183), so the resin branch must be the one
    // that fired, not the printer branch.
    assert!(
        stderr.contains("failed to load resin profile"),
        "expected the resin-load branch to fire first, got: {stderr}"
    );
    assert!(
        !stderr.contains("failed to load printer profile"),
        "printer branch must not fire when the resin name is already unknown: {stderr}"
    );
}

#[then(regex = r#"^stderr contains "Available profiles"$"#)]
fn then_stderr_contains_available_profiles(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("Available profiles"),
        "expected stderr to carry the available-profiles hint, got: {stderr}"
    );
    // Second observation: the listed names are the real
    // ResinProfileRepository::list() output, not a fixed string. Distinct
    // from cli_profile_by_name_loading.rs:244's printer-side assertion.
    assert!(
        stderr.contains("generic_standard"),
        "expected the real resin profile list to include generic_standard, got: {stderr}"
    );
}

// ---- UAT-6: Atomic write — partial failures don't corrupt existing target --

#[given(regex = r#"^an unrelated file at /tmp/safe\.sim\.json with content "previous"$"#)]
fn given_unrelated_file_safe_sim_json(world: &mut UatWorld) {
    let dir = unique_tmp_dir("producer-uat6");
    let safe = dir.join("safe.sim.json");
    std::fs::write(&safe, "previous")
        .unwrap_or_else(|e| panic!("write {}: {e}", safe.display()));
    world.cli_tmp_dir = Some(dir);
}

#[given(
    regex = r"^a path /tmp/blocked/inner\.sim\.json whose parent /tmp/blocked is not a directory$"
)]
fn given_blocked_parent_path(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("prior Given populated cli_tmp_dir");
    let blocked = dir.join("blocked");
    std::fs::write(&blocked, "not a directory")
        .unwrap_or_else(|e| panic!("write {}: {e}", blocked.display()));
}

#[when(
    regex = r"^the user invokes `resinsim sim --stl <PATH> \.\.\. --out /tmp/blocked/inner\.sim\.json`$"
)]
fn when_invoke_sim_uat6_blocked_parent(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("Given populated cli_tmp_dir");
    let input = copy_test_cube(&dir, "cube.stl");
    world.sim_input_path = Some(input.clone());
    let out = dir.join("blocked").join("inner.sim.json");
    world.sim_json_path = Some(out.clone());
    let data = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "sim",
            "--stl",
            input.to_str().expect("input path is UTF-8"),
            "--resin",
            "generic_standard",
            "--printer",
            "generic_msla_4k",
            "--data-dir",
            data.to_str().expect("data dir path is UTF-8"),
            "--out",
            out.to_str().expect("out path is UTF-8"),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

// `Then the process exits with non-zero code` — shared registration, see
// UAT-4's `then_exits_with_non_zero_code`.

#[then(regex = r#"^/tmp/safe\.sim\.json still contains "previous"$"#)]
fn then_safe_sim_json_still_contains_previous(world: &mut UatWorld) {
    let dir = world
        .cli_tmp_dir
        .clone()
        .expect("Given populated cli_tmp_dir");
    let safe = dir.join("safe.sim.json");
    let bytes = std::fs::read_to_string(&safe)
        .unwrap_or_else(|e| panic!("read {}: {e}", safe.display()));
    assert_eq!(
        bytes, "previous",
        "expected byte-identical unrelated-file content, got: {bytes:?}"
    );
    // Discrimination cross-check: the early create_dir_all probe fired
    // BEFORE any compute (main.rs:1902-1906), not a later write-failure
    // branch.
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("cannot prepare --out parent directory"),
        "expected the early out-parent probe's message, got: {stderr}"
    );
    assert!(
        !stderr.contains("Wrote ") && !stderr.contains("Error writing sim.json to"),
        "must not have reached compute or the write-failure branch: {stderr}"
    );
    // <uniq>/blocked is still a regular file with its original bytes.
    let blocked = dir.join("blocked");
    assert!(
        blocked.is_file(),
        "expected {} to remain a regular file",
        blocked.display()
    );
    let blocked_bytes = std::fs::read_to_string(&blocked)
        .unwrap_or_else(|e| panic!("read {}: {e}", blocked.display()));
    assert_eq!(
        blocked_bytes, "not a directory",
        "the blocked file must be untouched"
    );
    // No inner.sim.json / .tmp exists anywhere under <uniq>. `blocked` is a
    // regular file (not a directory), so nothing could have been created
    // inside it — a shallow scan of the unique dir itself is exhaustive.
    for entry in
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|e| panic!("read_dir entry in {}: {e}", dir.display()));
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            name != "inner.sim.json" && !name.ends_with(".tmp"),
            "no inner.sim.json or .tmp file may exist under {}, found {name}",
            dir.display()
        );
    }
}
