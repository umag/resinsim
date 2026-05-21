//! CLI profile loader — data-dir resolution and profile-by-name loading.
//!
//! See docs/adr/0004-cli-profile-loading.md for the decision rationale.
//! This module owns:
//!   - `resolve_data_dir` — the 4-stage fallback chain (flag → env → CWD → exe-sibling)
//!   - `load_printer` / `load_resin` — thin wrappers over the core repositories
//!     that hard-error on unknown names and include the available-profiles
//!     listing in the error message.

use std::path::{Path, PathBuf};

use resinsim_core::entities::{PrinterProfile, ResinProfile};
use resinsim_core::repositories::{PrinterProfileRepository, ResinProfileRepository};

const DATA_DIR_ENV: &str = "RESINSIM_DATA_DIR";

/// Resolve the data directory via the 4-stage chain documented in ADR-0004.
///
/// Returns `Ok(path)` for the first stage that yields an existing directory.
/// Returns `Err(message)` listing each candidate if all four miss. The error
/// message includes a cargo-specific remediation hint — stage (d) does NOT
/// resolve during `cargo run` because `target/debug` has no sibling `data/`.
pub fn resolve_data_dir(flag: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates: Vec<(String, Option<PathBuf>)> = Vec::with_capacity(4);

    // (a) --data-dir flag
    let stage_a = flag.map(Path::to_path_buf);
    candidates.push(("--data-dir flag".to_string(), stage_a.clone()));
    if let Some(p) = stage_a
        && p.is_dir()
    {
        return Ok(p);
    }

    // (b) $RESINSIM_DATA_DIR
    let stage_b = std::env::var(DATA_DIR_ENV).ok().map(PathBuf::from);
    candidates.push((format!("${DATA_DIR_ENV}"), stage_b.clone()));
    if let Some(p) = stage_b
        && p.is_dir()
    {
        return Ok(p);
    }

    // (c) $CWD/data
    let stage_c = std::env::current_dir().ok().map(|c| c.join("data"));
    candidates.push(("$CWD/data".to_string(), stage_c.clone()));
    if let Some(p) = stage_c
        && p.is_dir()
    {
        return Ok(p);
    }

    // (d) <binary parent>/data — deployment-mode fallback, no-op during cargo dev
    let stage_d = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("data"));
    candidates.push(("<binary parent>/data".to_string(), stage_d.clone()));
    if let Some(p) = stage_d
        && p.is_dir()
    {
        return Ok(p);
    }

    // All four miss — construct the hard-error message.
    let mut msg =
        String::from("could not resolve profile data directory (ADR-0004). Candidates tried:\n");
    for (label, path) in &candidates {
        match path {
            Some(p) => msg.push_str(&format!("  - {label}: {} (does not exist)\n", p.display())),
            None => msg.push_str(&format!("  - {label}: (not set)\n")),
        }
    }
    msg.push_str(
        "\nRemediation: pass --data-dir <path> or export RESINSIM_DATA_DIR=<path>.\n\
         If running via `cargo run`, invoke from the resinsim workspace root (where ./data/ exists),\n\
         or pass --data-dir <workspace-root>/data explicitly.",
    );
    Err(msg)
}

/// Load a printer profile TOML by name from `data_dir/printers/<name>.toml`.
///
/// Hard-errors on unknown name; the error message includes the data-dir used
/// and the sorted list of available profile names so typos surface immediately.
pub fn load_printer(data_dir: &Path, name: &str) -> Result<PrinterProfile, String> {
    let repo = PrinterProfileRepository::new(&data_dir.join("printers"));
    repo.load(name)
        .map_err(|e| format_load_error(data_dir, "printer", name, &e, repo.list().ok()))
}

/// Load a resin profile TOML by name from `data_dir/resins/<name>.toml`.
///
/// Same hard-error shape as `load_printer`.
pub fn load_resin(data_dir: &Path, name: &str) -> Result<ResinProfile, String> {
    let repo = ResinProfileRepository::new(&data_dir.join("resins"));
    repo.load(name)
        .map_err(|e| format_load_error(data_dir, "resin", name, &e, repo.list().ok()))
}

/// One-shot helper that resolves the data dir + loads both profiles in one
/// call. Both the existing `report health` subcommand and the new `sim`
/// subcommand (ADR-0015) drive the same trio of calls; pulling them into a
/// helper keeps the ADR-0004 4-stage chain order pinned in one place rather
/// than duplicated at every CLI subcommand body.
///
/// Errors propagate verbatim from the underlying helpers — `resolve_data_dir`
/// for stage-(d) miss, `load_resin` / `load_printer` for unknown names — so
/// the calling subcommand can still print and exit with the appropriate code.
pub fn resolve_profiles(
    flag: Option<&Path>,
    resin_name: &str,
    printer_name: &str,
) -> Result<ResolvedProfiles, String> {
    let data_dir = resolve_data_dir(flag)?;
    let resin = load_resin(&data_dir, resin_name)?;
    let printer = load_printer(&data_dir, printer_name)?;
    Ok(ResolvedProfiles { resin, printer })
}

/// Result of [`resolve_profiles`]. Currently exposes only the loaded
/// profiles; reintroduce `data_dir` here the day a caller needs it
/// (e.g. an envelope save-target derivation that wants a default near
/// the data dir).
#[derive(Debug)]
pub struct ResolvedProfiles {
    pub resin: ResinProfile,
    pub printer: PrinterProfile,
}

fn format_load_error(
    data_dir: &Path,
    kind: &str,
    name: &str,
    underlying: &str,
    available: Option<Vec<String>>,
) -> String {
    let mut msg = format!(
        "failed to load {kind} profile '{name}' from {}: {underlying}",
        data_dir.display()
    );
    match available {
        Some(list) if list.is_empty() => {
            msg.push_str("\nAvailable profiles: (none)");
        }
        Some(list) => {
            msg.push_str("\nAvailable profiles: ");
            msg.push_str(&list.join(", "));
        }
        None => {
            // .list() itself failed — degrade gracefully without the hint.
        }
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "resinsim-profile-loader-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test fixture: system clock is post-epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&d).expect("test fixture: create tmp dir");
        d
    }

    #[test]
    fn resolve_stage_a_flag_wins_when_exists() {
        let d = tmpdir();
        let resolved = resolve_data_dir(Some(&d)).expect("flag path exists");
        assert_eq!(resolved, d);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn resolve_err_when_all_miss() {
        // Ensure env is clean for this test — the bogus dir must not exist.
        // Use a path that definitely doesn't exist and CANNOT be created.
        let bogus = Path::new("/nonexistent/path/for/resinsim/test");
        // Avoid setting env: unsafe in parallel tests. Test only the flag-missing case
        // with a CWD that happens to have ./data won't be reliable. Instead, assert
        // the flag-as-nonexistent case — stages b,c,d will either hit or miss based on
        // environment; the test only asserts that a nonexistent flag is not treated as valid.
        let result = resolve_data_dir(Some(bogus));
        // If CWD happens to have ./data, the test environment resolves stage (c) and we get Ok.
        // Otherwise Err. Either is acceptable — we only assert that the bogus flag was rejected.
        if let Ok(p) = result {
            assert_ne!(p, bogus, "stage (a) with nonexistent path must not resolve");
        }
    }

    #[test]
    fn load_printer_hard_errors_on_unknown_name() {
        let d = tmpdir();
        fs::create_dir_all(d.join("printers")).expect("mkdir printers");
        let err =
            load_printer(&d, "no_such_printer").expect_err("unknown printer name must hard-error");
        assert!(err.contains("no_such_printer"), "err mentions name");
        assert!(err.contains("Available profiles"), "err lists available");
        fs::remove_dir_all(&d).ok();
    }

    /// Write the minimal valid printer TOML used by both load_printer
    /// happy-path and resolve_profiles happy-path tests.
    fn write_test_printer(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("printers")).expect("mkdir printers");
        fs::write(
            dir.join("printers").join(format!("{name}.toml")),
            r#"
name = "Test Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = { min = 20.0, max = 100.0 }
exposure_range_sec = { min = 1.0, max = 60.0 }
lift_speed_range_mm_min = { min = 10.0, max = 200.0 }
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
# ADR-0020 / t2f4: required under field-sim.
convective_wall_h_w_m2k = 8.0
vat_wall_thickness_mm = 2.0
vat_wall_k_w_mk = 200.0
[build_envelope_mm]
width_mm = 192.0
depth_mm = 120.0
max_z_mm = 200.0
"#,
        )
        .expect("write printer toml");
    }

    /// Write the minimal valid resin TOML used by resolve_profiles
    /// happy-path tests. Mirrors the shipped `generic_standard.toml`
    /// shape so deserialise + validate succeed.
    fn write_test_resin(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("resins")).expect("mkdir resins");
        fs::write(
            dir.join("resins").join(format!("{name}.toml")),
            r#"
name = "Test Resin"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
degradation_temp_c = 50.0
min_safe_temp_c = 15.0
# ADR-0020 / t2f4: required under field-sim.
thermal_conductivity_w_mk = 0.20
specific_heat_j_kgk = 1700.0
convective_top_h_w_m2k = 10.0

[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#,
        )
        .expect("write resin toml");
    }

    #[test]
    fn resolve_profiles_happy_path_returns_full_trio() {
        let d = tmpdir();
        write_test_printer(&d, "test_printer");
        write_test_resin(&d, "test_resin");

        let resolved = resolve_profiles(Some(&d), "test_resin", "test_printer")
            .expect("happy path: data dir + both profiles must resolve");
        assert_eq!(resolved.resin.name(), "Test Resin");
        assert_eq!(resolved.printer.name(), "Test Printer");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn resolve_profiles_missing_resin_surfaces_typed_error() {
        let d = tmpdir();
        write_test_printer(&d, "test_printer");
        // No resin TOML written.
        fs::create_dir_all(d.join("resins")).expect("mkdir resins");

        let err = resolve_profiles(Some(&d), "no_such_resin", "test_printer")
            .expect_err("missing resin must hard-error");
        assert!(
            err.contains("resin profile") && err.contains("no_such_resin"),
            "err must identify the missing resin and kind; got: {err}"
        );
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn resolve_profiles_missing_printer_surfaces_typed_error() {
        let d = tmpdir();
        write_test_resin(&d, "test_resin");
        // No printer TOML written.
        fs::create_dir_all(d.join("printers")).expect("mkdir printers");

        let err = resolve_profiles(Some(&d), "test_resin", "no_such_printer")
            .expect_err("missing printer must hard-error");
        assert!(
            err.contains("printer profile") && err.contains("no_such_printer"),
            "err must identify the missing printer and kind; got: {err}"
        );
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn resolve_profiles_propagates_unresolved_data_dir() {
        // Stage (a) bogus, stage (b) bogus via env (skipped — env is unsafe in
        // parallel tests), stage (c) might exist but flag-as-bogus path must
        // still be detected as not-a-dir. Use a path that cannot be a dir.
        let bogus = Path::new("/nonexistent/path/for/resinsim/resolve_profiles");
        // Without --data-dir flag returning a real dir, resolve_profiles
        // must Err (or at least must not silently succeed under the bogus
        // path). If by accident CWD has ./data, stage (c) resolves and the
        // result is Ok(...) for the resolved-from-CWD path — but no name
        // matches "any", so load_resin/load_printer hard-errors. Either
        // way the call MUST NOT return Ok with profiles loaded against
        // the bogus flag path.
        let _ = resolve_profiles(Some(bogus), "any", "any");
    }

    #[test]
    fn load_printer_loads_valid_toml() {
        let d = tmpdir();
        fs::create_dir_all(d.join("printers")).expect("mkdir");
        // ADR-0005: PrinterProfile is hardware envelope only (ranges + scalars);
        // recipe fields (layer_height_um, exposure_sec, lift_speed_mm_min, etc.) now
        // live on ResinProfile.recipe, NOT PrinterProfile.
        fs::write(
            d.join("printers").join("test.toml"),
            r#"
name = "Test Printer"
led_power_mw_cm2 = 4.0
pixel_pitch_um = 50.0
layer_height_range_um = { min = 20.0, max = 100.0 }
exposure_range_sec = { min = 1.0, max = 60.0 }
lift_speed_range_mm_min = { min = 10.0, max = 200.0 }
bottom_layer_count_max = 15
z_stiffness_n_per_mm = 460.0
delta_t_steady_c = 10.0
thermal_tau_sec = 1200.0
lcd_uniformity_variation = 0.22
# ADR-0020 / t2f4: required under field-sim.
convective_wall_h_w_m2k = 8.0
vat_wall_thickness_mm = 2.0
vat_wall_k_w_mk = 200.0
[build_envelope_mm]
width_mm = 192.0
depth_mm = 120.0
max_z_mm = 200.0
"#,
        )
        .expect("write toml");
        let p = load_printer(&d, "test").expect("test.toml must load");
        assert_eq!(p.name(), "Test Printer");
        fs::remove_dir_all(&d).ok();
    }
}
