use crate::entities::ResinProfile;
use std::path::{Path, PathBuf};

/// Repository for loading resin profiles from TOML files.
pub struct ResinProfileRepository {
    data_dir: PathBuf,
}

impl ResinProfileRepository {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// Load a resin profile by name (filename without .toml extension).
    pub fn load(&self, name: &str) -> Result<ResinProfile, String> {
        let path = self.data_dir.join(format!("{name}.toml"));
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let profile: ResinProfile = toml::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        profile
            .validate()
            .map_err(|e| format!("invalid resin profile {}: {e}", path.display()))?;
        Ok(profile)
    }

    /// List available resin profile names.
    pub fn list(&self) -> Result<Vec<String>, String> {
        let entries = std::fs::read_dir(&self.data_dir)
            .map_err(|e| format!("failed to read {}: {e}", self.data_dir.display()))?;

        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        Ok(names)
    }

    /// Load a profile by name, falling back to generic_standard() if not found.
    pub fn load_or_default(&self, name: &str) -> ResinProfile {
        self.load(name)
            .unwrap_or_else(|_| ResinProfile::generic_standard())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> ResinProfileRepository {
        // Navigate from crate root to workspace data/
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let data_dir = Path::new(manifest_dir).join("../../data/resins");
        ResinProfileRepository::new(&data_dir)
    }

    #[test]
    fn load_generic_standard() {
        let profile = repo()
            .load("generic_standard")
            .expect("test fixture: generic_standard.toml ships with the crate under data/resins/");
        assert_eq!(profile.name, "Generic Standard");
        assert!((profile.penetration_depth_um - 170.0).abs() < 0.01);
        assert!((profile.critical_energy_mj_cm2 - 5.0).abs() < 0.01);
    }

    #[test]
    fn load_abs_like() {
        let profile = repo()
            .load("generic_abs_like")
            .expect("test fixture: generic_abs_like.toml ships with the crate under data/resins/");
        assert_eq!(profile.name, "Generic ABS-Like");
    }

    #[test]
    fn load_liqcreate() {
        let profile = repo().load("liqcreate_premium_black").expect(
            "test fixture: liqcreate_premium_black.toml ships with the crate under data/resins/",
        );
        assert_eq!(profile.name, "Liqcreate Premium Black");
        assert!((profile.viscosity_mpa_s - 300.0).abs() < 0.01);
    }

    #[test]
    fn load_not_found() {
        let result = repo().load("nonexistent_resin");
        assert!(result.is_err());
    }

    #[test]
    fn list_profiles() {
        let names = repo()
            .list()
            .expect("test fixture: data/resins/ ships with the crate and is readable");
        assert!(names.contains(&"generic_standard".to_string()));
        assert!(names.contains(&"generic_abs_like".to_string()));
        assert!(names.contains(&"liqcreate_premium_black".to_string()));
    }

    #[test]
    fn load_or_default_falls_back() {
        let profile = repo().load_or_default("nonexistent");
        assert_eq!(profile.name, "Generic Standard");
    }
}
