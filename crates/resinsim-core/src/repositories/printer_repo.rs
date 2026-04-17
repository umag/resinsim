use crate::entities::PrinterProfile;
use std::path::{Path, PathBuf};

/// Repository for loading printer profiles from TOML files.
pub struct PrinterProfileRepository {
    data_dir: PathBuf,
}

impl PrinterProfileRepository {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn load(&self, name: &str) -> Result<PrinterProfile, String> {
        let path = self.data_dir.join(format!("{name}.toml"));
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let profile: PrinterProfile = toml::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        profile
            .validate()
            .map_err(|e| format!("invalid printer profile {}: {e}", path.display()))?;
        Ok(profile)
    }

    pub fn list(&self) -> Result<Vec<String>, String> {
        let entries = std::fs::read_dir(&self.data_dir)
            .map_err(|e| format!("failed to read {}: {e}", self.data_dir.display()))?;

        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        names.sort();
        Ok(names)
    }

    pub fn load_or_default(&self, name: &str) -> PrinterProfile {
        self.load(name).unwrap_or_else(|_| PrinterProfile::generic_msla_4k())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PrinterProfileRepository {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let data_dir = Path::new(manifest_dir).join("../../data/printers");
        PrinterProfileRepository::new(&data_dir)
    }

    #[test]
    fn load_generic_msla() {
        let profile = repo().load("generic_msla_4k").unwrap();
        assert_eq!(profile.name, "Generic MSLA 4K");
        assert!((profile.led_power_mw_cm2 - 4.0).abs() < 0.01);
    }

    #[test]
    fn load_athena_ii() {
        let profile = repo().load("athena_ii").unwrap();
        assert_eq!(profile.name, "Athena II");
        assert!((profile.z_stiffness_n_per_mm - 1500.0).abs() < 0.01);
    }

    #[test]
    fn list_printers() {
        let names = repo().list().unwrap();
        assert!(names.contains(&"generic_msla_4k".to_string()));
        assert!(names.contains(&"athena_ii".to_string()));
    }
}
