//! Migration-completeness invariant (ADR-0005 plan step 10).
//!
//! Every `.toml` file shipped under `data/printers/` and `data/resins/` must load
//! cleanly through its respective repository. A partial migration (e.g. 3 of 4 resin
//! TOMLs updated to include `[recipe]`, one forgotten) would surface here before the
//! broken file reaches a user.
//!
//! Extends ADR-0004's name-based loading contract to completeness: "every shipped
//! TOML file must load". Catches silent drift in data/.

use std::path::{Path, PathBuf};

use resinsim_core::repositories::{PrinterProfileRepository, ResinProfileRepository};

fn workspace_data_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/resinsim-core; data/ is at ../../data/
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .canonicalize()
        .expect("test fixture: workspace data/ directory must exist")
}

fn toml_stems_in(dir: &Path) -> Vec<String> {
    let mut stems = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}")) {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            stems.push(stem.to_string());
        }
    }
    stems.sort();
    stems
}

#[test]
fn every_shipped_printer_toml_loads_by_name() {
    let printers_dir = workspace_data_dir().join("printers");
    let repo = PrinterProfileRepository::new(&printers_dir);
    let stems = toml_stems_in(&printers_dir);
    assert!(
        !stems.is_empty(),
        "data/printers/ must ship at least one TOML (found 0)"
    );
    for stem in &stems {
        repo.load(stem).unwrap_or_else(|e| {
            panic!(
                "shipped printer TOML '{stem}' failed to load: {e}\n\
                 ADR-0005 migration invariant: every file under data/printers/ must be loadable"
            )
        });
    }
}

#[test]
fn every_shipped_resin_toml_loads_by_name() {
    let resins_dir = workspace_data_dir().join("resins");
    let repo = ResinProfileRepository::new(&resins_dir);
    let stems = toml_stems_in(&resins_dir);
    assert!(
        !stems.is_empty(),
        "data/resins/ must ship at least one TOML (found 0)"
    );
    for stem in &stems {
        repo.load(stem).unwrap_or_else(|e| {
            panic!(
                "shipped resin TOML '{stem}' failed to load: {e}\n\
                 ADR-0005 migration invariant: every file under data/resins/ must be loadable \
                 (and must include a [recipe] table per ADR-0005 Consequences)"
            )
        });
    }
}
