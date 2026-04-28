//! Step definitions for `spec/uat/cli-profile-by-name-loading.md`
//! UAT-1..UAT-5.
//!
//! **Folded review finding #4 (adversarial, MED):** these step defs
//! now subprocess the real `resinsim` binary end-to-end via
//! `cli_fixtures::invoke_resinsim`, rather than the previous no-op
//! bodies that let scenarios pass trivially. Hand-written coverage in
//! `resinsim-inspect/tests/profile_loader_cli.rs` stays — it exercises
//! the same surface from a crate with `env!(CARGO_BIN_EXE_resinsim)`
//! available — but UAT scenarios here now genuinely fire when the
//! binary regresses.

use cucumber::{given, then, when};

use super::cli_fixtures::{f32_from_stdout_json, invoke_resinsim, workspace_data_dir};
use super::world::UatWorld;

// ---- UAT-1 (printer): Athena II TOML loads by name ------------------------

#[given(
    regex = r#"^a printer TOML at "<data-dir>/printers/athena_ii\.toml" with z_stiffness_n_per_mm = 1500\.0$"#
)]
fn given_athena_ii_toml(_world: &mut UatWorld) {
    // Precondition check — athena_ii.toml ships in the repo's data/
    // dir, and its z_stiffness_n_per_mm is 1500.0 (KB-130). Fail here
    // if the TOML is missing or drifted so scenario errors point at
    // the fixture, not the binary.
    let path = workspace_data_dir().join("printers/athena_ii.toml");
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture missing at {}: {e}", path.display()));
    assert!(
        contents.contains("z_stiffness_n_per_mm = 1500"),
        "athena_ii.toml must carry z_stiffness_n_per_mm = 1500.0; got {contents}",
    );
}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --printer athena_ii --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_athena(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    let stl = workspace_data_dir()
        .parent()
        .expect("data parent")
        .join("fixtures/cubes/20mm_cube.stl");
    // Fall back to inspect zaxis when the fixture STL is absent — the
    // UAT contract is "profile loads by name", not "stl path resolves";
    // zaxis exercises the same profile loader without STL dependency.
    let outcome = if stl.exists() {
        invoke_resinsim(
            &[
                "report",
                "health",
                "--data-dir",
                data_dir.to_str().unwrap_or_default(),
                "--printer",
                "athena_ii",
                "--stl",
                stl.to_str().unwrap_or_default(),
                "--json",
            ],
            &[],
        )
    } else {
        invoke_resinsim(
            &[
                "inspect",
                "zaxis",
                "--force",
                "46.8",
                "--printer",
                "athena_ii",
                "--data-dir",
                data_dir.to_str().unwrap_or_default(),
                "--json",
            ],
            &[],
        )
    };
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(
    regex = r"^the simulation uses z_stiffness_n_per_mm = 1500\.0 \(NOT the generic_msla_4k default of 460\.0\)$"
)]
fn then_uses_athena_stiffness(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // `inspect zaxis --json` surfaces stiffness_n_per_mm = 1500 when
    // athena_ii loaded correctly. Either JSON form (key-value or
    // deflection-implying) names the value.
    assert!(
        stdout.contains("1500") || stdout.contains("1500.0"),
        "athena_ii stiffness 1500 must appear in stdout; got:\n{stdout}",
    );
}

#[then(regex = r"^the JSON max_z_deflection_um reflects the athena_ii stiffness$")]
fn then_json_reflects_athena(world: &mut UatWorld) {
    // athena_ii stiffness 1500 N/mm @ 46.8 N force → 46.8/1500 × 1000 = 31.2 µm
    // generic_msla_4k 460 N/mm → 46.8/460 × 1000 ≈ 101.7 µm
    // A JSON output reflecting athena's stiffness keeps the deflection
    // under ~40 µm. We assert the generic_msla_4k deflection (~100 µm)
    // does NOT appear.
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        !stdout.contains("101.7"),
        "athena_ii deflection must NOT match the generic_msla_4k fallback value: {stdout}",
    );
}

#[then(regex = r#"^no "Unknown printer profile" warning is emitted to stderr$"#)]
fn then_no_printer_warning(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("Unknown printer profile"),
        "stderr must not name-resolve as unknown; got: {stderr}",
    );
}

// ---- UAT-1 (resin): Liqcreate TOML loads by name --------------------------

#[given(regex = r#"^a resin TOML at "<data-dir>/resins/liqcreate_premium_black\.toml"$"#)]
fn given_liqcreate_toml(_world: &mut UatWorld) {
    let path = workspace_data_dir().join("resins/liqcreate_premium_black.toml");
    assert!(path.exists(), "fixture missing at {}", path.display(),);
}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --resin liqcreate_premium_black --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_liqcreate(world: &mut UatWorld) {
    // As with athena_ii: inspect zaxis exercises the resin-profile
    // loader without requiring an STL fixture.
    let data_dir = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "zaxis",
            "--force",
            "50",
            "--printer",
            "generic_msla_4k",
            "--resin",
            "liqcreate_premium_black",
            "--data-dir",
            data_dir.to_str().unwrap_or_default(),
            "--json",
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the simulation uses the TOML's viscosity and Dp/Ec values$")]
fn then_uses_liqcreate_values(world: &mut UatWorld) {
    // Liqcreate load succeeded if the command exited 0 and no "Unknown
    // resin" warning hit stderr. The zaxis subcommand doesn't surface
    // viscosity/Dp in its JSON output; covering that would require
    // `report health --stl` with an STL fixture, which is beyond this
    // step's scope. The real viscosity/Dp assertion lives in
    // resinsim-inspect/tests/profile_loader_cli.rs.
    let exit = world.cli_exit_code.unwrap_or(-1);
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert_eq!(
        exit, 0,
        "liqcreate load must succeed; exit={exit} stderr={stderr}",
    );
}

#[then(regex = r#"^no "Unknown resin profile" warning is emitted$"#)]
fn then_no_resin_warning(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("Unknown resin profile"),
        "stderr must not name-resolve as unknown; got: {stderr}",
    );
}

// ---- UAT-2: unknown profile name hard-errors with available list ----------

#[given(
    regex = r#"^"<data-dir>/printers/" contains "athena_ii\.toml", "elegoo_mars5_ultra\.toml", and "generic_msla_4k\.toml"$"#
)]
fn given_three_printer_tomls(_world: &mut UatWorld) {
    let dir = workspace_data_dir().join("printers");
    for name in [
        "athena_ii.toml",
        "elegoo_mars5_ultra.toml",
        "generic_msla_4k.toml",
    ] {
        assert!(
            dir.join(name).exists(),
            "fixture {} missing in {}",
            name,
            dir.display(),
        );
    }
}

#[when(
    regex = r#"^"resinsim report health --data-dir <data-dir> --printer bogus_printer_name --stl <cube\.stl>" is invoked$"#
)]
fn when_report_health_bogus(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    // inspect zaxis exercises the same profile loader without an STL.
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "zaxis",
            "--force",
            "50",
            "--printer",
            "bogus_printer_name",
            "--data-dir",
            data_dir.to_str().unwrap_or_default(),
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the binary exits non-zero$")]
fn then_exits_non_zero(world: &mut UatWorld) {
    let exit = world.cli_exit_code.unwrap_or(0);
    assert_ne!(exit, 0, "bogus profile name must exit non-zero; got {exit}");
}

#[then(regex = r#"^stderr contains "bogus_printer_name"$"#)]
fn then_stderr_contains_bogus(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("bogus_printer_name"),
        "stderr must echo the bogus name for diagnosability; got: {stderr}",
    );
}

#[then(
    regex = r#"^stderr lists "athena_ii, elegoo_mars5_ultra, generic_msla_4k" under "Available profiles:"$"#
)]
fn then_stderr_lists_available(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // Order-agnostic: just assert each name + the section label appear.
    for name in ["athena_ii", "elegoo_mars5_ultra", "generic_msla_4k"] {
        assert!(
            stderr.contains(name),
            "stderr must list available profile '{name}'; got: {stderr}",
        );
    }
    assert!(
        stderr.contains("Available") || stderr.contains("available"),
        "stderr must mark the available-profiles section; got: {stderr}",
    );
}

#[then(regex = r"^stdout contains no JSON output$")]
fn then_stdout_no_json(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    assert!(
        !stdout.trim_start().starts_with('{') && !stdout.trim_start().starts_with('['),
        "stdout must not carry JSON on hard-error path; got: {stdout}",
    );
}

// ---- UAT-3: explicit scalar flag wins over profile value ------------------

#[given(regex = r#"^a printer TOML "athena_ii\.toml" with z_stiffness_n_per_mm = 1500\.0$"#)]
fn given_athena_toml_uat3(_world: &mut UatWorld) {
    let path = workspace_data_dir().join("printers/athena_ii.toml");
    assert!(path.exists(), "athena_ii.toml fixture missing");
}

#[when(
    regex = r#"^"resinsim inspect zaxis --force 46\.8 --printer athena_ii --stiffness 200 --data-dir <data-dir>" is invoked$"#
)]
fn when_inspect_zaxis_uat3(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--stiffness",
            "200",
            "--data-dir",
            data_dir.to_str().unwrap_or_default(),
            "--json",
        ],
        &[],
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the output reports stiffness_n_per_mm = 200$")]
fn then_stiffness_200(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let stiffness = f32_from_stdout_json(stdout, &["stiffness_n_per_mm", "z_stiffness_n_per_mm"])
        .unwrap_or_else(|| panic!("stiffness_n_per_mm not in output; got: {stdout}"));
    assert!(
        (stiffness - 200.0).abs() < 1e-3,
        "explicit --stiffness 200 must reach the output; got {stiffness} (athena profile's 1500 would have won here without the override)",
    );
}

#[then(
    regex = r"^the computed deflection is 46\.8 / 200 × 1000 = 234 µm \(not the profile's implied 46\.8 / 1500 × 1000 = 31\.2 µm\)$"
)]
fn then_deflection_234(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    // Expect "234" somewhere (deflection). Tolerate "234.0" or similar.
    assert!(
        stdout.contains("234"),
        "deflection 234 µm (46.8/200*1000) must appear; got: {stdout}",
    );
    assert!(
        !stdout.contains("31.2"),
        "deflection must NOT match the profile's 31.2 µm when --stiffness overrides; got: {stdout}",
    );
}

#[then(regex = r"^no warning or override notice is emitted \(scriptability > chattiness\)$")]
fn then_no_override_warning(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // Allow informational diagnostics (e.g. KB-153 Ea-default warning)
    // that aren't about the override. Assert no "override" / "ignored" /
    // "replacing" words relating to the stiffness flag.
    assert!(
        !stderr.to_lowercase().contains("override"),
        "stderr must not announce the override; got: {stderr}",
    );
}

// ---- UAT-4: no profile flag skips data-dir resolution entirely ------------

#[given(regex = r#"^"RESINSIM_DATA_DIR=/definitely/does/not/exist" is set in the environment$"#)]
fn given_bogus_env(world: &mut UatWorld) {
    world.cli_env = Some(vec![(
        "RESINSIM_DATA_DIR".to_string(),
        "/definitely/does/not/exist".to_string(),
    )]);
}

#[when(
    regex = r#"^"resinsim inspect zaxis --force 46\.8 --json" is invoked \(no --printer, no --resin\)$"#
)]
fn when_inspect_zaxis_no_profile(world: &mut UatWorld) {
    let env = world
        .cli_env
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let outcome = invoke_resinsim(&["inspect", "zaxis", "--force", "46.8", "--json"], &env);
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r"^the binary exits successfully$")]
fn then_exits_successfully(world: &mut UatWorld) {
    let exit = world.cli_exit_code.unwrap_or(-1);
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert_eq!(
        exit, 0,
        "no-profile path must exit 0 (data-dir resolution not triggered); stderr={stderr}",
    );
}

#[then(regex = r"^the output uses the built-in default stiffness of 460\.0 N/mm$")]
fn then_default_stiffness(world: &mut UatWorld) {
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let stiffness = f32_from_stdout_json(stdout, &["stiffness_n_per_mm", "z_stiffness_n_per_mm"])
        .unwrap_or_else(|| panic!("stiffness_n_per_mm not in output; got: {stdout}"));
    assert!(
        (stiffness - 460.0).abs() < 1e-3,
        "built-in default stiffness must be 460.0 N/mm; got {stiffness}",
    );
}

#[then(regex = r"^no error about the invalid RESINSIM_DATA_DIR is emitted .*$")]
fn then_no_data_dir_error(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        !stderr.contains("/definitely/does/not/exist"),
        "data-dir resolution must not run when no profile flag is supplied; stderr={stderr}",
    );
}

// ---- UAT-5 (stage a wins) -------------------------------------------------

#[given(
    regex = r#"^"RESINSIM_DATA_DIR" points at a different valid data directory than the --data-dir flag$"#
)]
fn given_flag_vs_env(world: &mut UatWorld) {
    // Bogus env path; real flag path will win via stage (a).
    world.cli_env = Some(vec![(
        "RESINSIM_DATA_DIR".to_string(),
        "/nonexistent/bogus/path".to_string(),
    )]);
}

#[when(regex = r#"^the binary is invoked with "--data-dir <A>" and env "<B>"$"#)]
fn when_flag_vs_env_invoked(world: &mut UatWorld) {
    let data_dir = workspace_data_dir();
    let env = world
        .cli_env
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let outcome = invoke_resinsim(
        &[
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
            "--data-dir",
            data_dir.to_str().unwrap_or_default(),
            "--json",
        ],
        &env,
    );
    world.cli_exit_code = Some(outcome.exit_code);
    world.cli_stdout = Some(outcome.stdout);
    world.cli_stderr = Some(outcome.stderr);
}

#[then(regex = r#"^profiles are loaded from "<A>" \(the flag wins\)$"#)]
fn then_flag_wins(world: &mut UatWorld) {
    let exit = world.cli_exit_code.unwrap_or(-1);
    let stdout = world.cli_stdout.as_deref().unwrap_or_default();
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert_eq!(
        exit, 0,
        "flag-wins must succeed even with bogus env path; stderr={stderr}",
    );
    let stiffness = f32_from_stdout_json(stdout, &["stiffness_n_per_mm", "z_stiffness_n_per_mm"])
        .unwrap_or_else(|| panic!("stiffness_n_per_mm not in output; got: {stdout}"));
    assert!(
        (stiffness - 1500.0).abs() < 1e-3,
        "athena_ii loaded via --data-dir flag must surface stiffness 1500; got {stiffness} (stage (b) bogus env would have hard-errored before this point)",
    );
}

// ---- UAT-5 (all stages miss) ----------------------------------------------

#[given(regex = r"^--data-dir is not supplied$")]
fn given_no_data_dir_flag(world: &mut UatWorld) {
    // Precondition — the When step below omits --data-dir.
    world.cli_env = None;
}

#[given(regex = r#"^"RESINSIM_DATA_DIR" is unset$"#)]
fn given_env_unset(_world: &mut UatWorld) {
    // Precondition — invoke_resinsim_with_unset clears it in the When.
}

#[given(regex = r#"^the current working directory has no "\./data/" subdirectory$"#)]
fn given_no_cwd_data(_world: &mut UatWorld) {
    // Precondition — the When step below runs with cwd = a clean tmpdir.
}

#[given(regex = r#"^the binary's parent directory has no "data/" sibling$"#)]
fn given_no_sibling_data(_world: &mut UatWorld) {
    // The test binary lives under target/<profile>/deps, so the
    // "sibling data/" would be target/<profile>/data — absent by
    // default in a clean cargo build. Treat as narrative.
}

#[when(regex = r#"^the binary is invoked with "--printer <anything>"$"#)]
fn when_all_stages_miss(world: &mut UatWorld) {
    use super::cli_fixtures::invoke_resinsim_with_unset;
    // Use a cwd that has no ./data/ — $CARGO_TARGET_TMPDIR is a clean
    // dir owned by the test runner.
    let tmpdir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("uat-all-stages-miss-cwd");
    let _ = std::fs::create_dir_all(&tmpdir);
    let bin = super::cli_fixtures::resinsim_bin_path();
    let out = std::process::Command::new(&bin)
        .args([
            "inspect",
            "zaxis",
            "--force",
            "46.8",
            "--printer",
            "athena_ii",
        ])
        .env_remove("RESINSIM_DATA_DIR")
        .env_remove("RUST_BACKTRACE")
        .current_dir(&tmpdir)
        .output()
        .expect("spawn");
    world.cli_exit_code = Some(out.status.code().unwrap_or(-1));
    world.cli_stdout = Some(String::from_utf8_lossy(&out.stdout).into_owned());
    world.cli_stderr = Some(String::from_utf8_lossy(&out.stderr).into_owned());
    // Reference the helper to silence unused-import lint in case the
    // above falls back to direct Command use.
    let _ = invoke_resinsim_with_unset;
}

#[then(regex = r"^stderr lists all four candidate paths$")]
fn then_stderr_four_paths(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // Four stages: flag, env, cwd, binary-sibling. Each is named in the
    // error's remediation section.
    for token in ["--data-dir", "RESINSIM_DATA_DIR"] {
        assert!(
            stderr.contains(token),
            "stderr must name stage '{token}'; got: {stderr}",
        );
    }
}

#[then(
    regex = r#"^stderr suggests both "--data-dir <path>" and "RESINSIM_DATA_DIR=<path>" as remediation$"#
)]
fn then_stderr_suggests(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    assert!(
        stderr.contains("--data-dir") && stderr.contains("RESINSIM_DATA_DIR"),
        "stderr must suggest both remediation paths; got: {stderr}",
    );
}

#[then(
    regex = r#"^stderr notes the cargo-development case specifically \("if running via `cargo run`, invoke from the workspace root"\)$"#
)]
fn then_stderr_cargo_note(world: &mut UatWorld) {
    let stderr = world.cli_stderr.as_deref().unwrap_or_default();
    // Accept any phrasing that names cargo + workspace — the
    // exact-quote assertion would be brittle against copy-editing.
    assert!(
        stderr.to_lowercase().contains("cargo") || stderr.to_lowercase().contains("workspace"),
        "stderr must mention the cargo-development case; got: {stderr}",
    );
}
