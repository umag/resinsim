//! Bevy `Resource` newtype that bundles the resin + printer
//! repositories so panel systems can query them without taking on
//! `resinsim-core` repository types in their signatures.
//!
//! Constructed once at startup from a resolved data dir
//! (`data_dir::resolve_data_dir`) and inserted into the App. Systems
//! hold an `Option<Res<ProfileRepos>>` so a missing data dir keeps
//! the app running with empty pickers and a visible error string in
//! `SimulationResult.last_error`.

use bevy::prelude::Resource;
use resinsim_core::repositories::{PrinterProfileRepository, ResinProfileRepository};

#[derive(Resource)]
pub struct ProfileRepos {
    pub resin: ResinProfileRepository,
    pub printer: PrinterProfileRepository,
}

impl ProfileRepos {
    /// Construct from a resolved data dir. `<data_dir>/resins/` and
    /// `<data_dir>/printers/` are the conventional subdirectory layout
    /// (matches `resinsim-inspect` and the workspace's `data/` shipping
    /// fixtures).
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            resin: ResinProfileRepository::new(&data_dir.join("resins")),
            printer: PrinterProfileRepository::new(&data_dir.join("printers")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use std::path::PathBuf;

    /// Workspace `data/` directory — same convention used by
    /// `main.rs::tests::cube_fixture_path`.
    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    #[test]
    fn resource_roundtrip_via_app_world() {
        let mut app = App::new();
        app.insert_resource(ProfileRepos::new(&workspace_data_dir()));
        let stored = app
            .world()
            .get_resource::<ProfileRepos>()
            .expect("test fixture: ProfileRepos was just inserted as a resource");
        let resin_names = stored
            .resin
            .list()
            .expect("test fixture: data/resins/ ships with the workspace and is readable");
        assert!(
            resin_names.contains(&"generic_standard".to_string()),
            "shipped resin profile generic_standard must be listed; got {resin_names:?}"
        );
        let printer_names = stored
            .printer
            .list()
            .expect("test fixture: data/printers/ ships with the workspace and is readable");
        assert!(
            printer_names.contains(&"generic_msla_4k".to_string()),
            "shipped printer profile generic_msla_4k must be listed; got {printer_names:?}"
        );
    }
}
