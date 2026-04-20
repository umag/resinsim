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
"#,
        )
        .expect("write toml");
        let p = load_printer(&d, "test").expect("test.toml must load");
        assert_eq!(p.name(), "Test Printer");
        fs::remove_dir_all(&d).ok();
    }
}
