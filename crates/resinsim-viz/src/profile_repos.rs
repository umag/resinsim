//! Bevy `Resource` newtype that wraps the pure
//! `resinsim_core::app::ProfileRepos` so panel systems can query it
//! without taking on `resinsim-core` repository types directly in their
//! signatures.
//!
//! Constructed once at startup from a resolved data dir
//! (`data_dir::resolve_data_dir`) and inserted into the App. Systems hold
//! an `Option<Res<ProfileRepos>>` so a missing data dir keeps the app
//! running with empty pickers and a visible error string in
//! `SimulationResult.last_error`.
//!
//! ADR-0010 layering rule: this newtype lives on the viz side because
//! `#[derive(Resource)]` is a Bevy concern. The pure data shape
//! (`resinsim_core::app::ProfileRepos`) lives in core so the CLI can
//! consume the same construct without taking on Bevy.

use bevy::prelude::Resource;
use resinsim_core::app::ProfileRepos as CoreProfileRepos;

#[derive(Resource)]
pub struct ProfileRepos(pub CoreProfileRepos);

impl ProfileRepos {
    /// Construct from a resolved data dir; delegates to
    /// [`resinsim_core::app::ProfileRepos::new`] for the actual layout
    /// convention (`<data_dir>/resins/`, `<data_dir>/printers/`).
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self(CoreProfileRepos::new(data_dir))
    }
}

impl std::ops::Deref for ProfileRepos {
    type Target = CoreProfileRepos;
    fn deref(&self) -> &CoreProfileRepos {
        &self.0
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
