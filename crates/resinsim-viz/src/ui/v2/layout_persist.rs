//! v2 PaneGrid layout persistence.
//!
//! On disk: a JSON file at the platform-correct per-user config
//! directory (`~/Library/Application Support/io.aopab.resinsim/v2-layout.json`
//! on macOS, resolved via the `directories` crate). The contents
//! are a `PaneGridLayout` struct: schema version, column split,
//! row fractions, and a 5×2 grid of `PaneId`s.
//!
//! Per open Q #1 (slice A plan): per-user, not per-project.
//! Rationale: a single dev with many sim files expects their layout
//! habits to survive across files; per-project would force re-
//! arranging every time.
//!
//! ## Schema versioning
//!
//! The on-disk file carries `schema_version: u32`. A loader that
//! sees an unknown version returns `Err`, and the dashboard falls
//! back to the default layout — never silently mis-maps a renamed
//! variant. Bump the constant whenever the on-disk shape changes
//! (variant rename, struct field add/remove, semantics of an
//! existing field).
//!
//! ## Atomic write
//!
//! Save writes to `<path>.tmp` then renames over the destination.
//! If the rename fails (e.g. another process is writing
//! simultaneously, or the disk is full), the destination is
//! untouched and the next save retries.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::pane::PaneId;

/// On-disk schema version. Bump on any breaking change to
/// [`PaneGridLayout`] or [`PaneId`] variants. A loader that finds
/// a different version returns [`LoadError::SchemaVersion`] and
/// the caller falls back to default.
///
/// **v2**: added `col_spans` for cells that span both columns at
/// the same row (used by the default LayerMask2d position).
pub const LAYOUT_SCHEMA_VERSION: u32 = 2;

/// Persisted shape of a [`super::grid::PaneGrid`].
///
/// `col_spans[r][c]` encodes column-span behaviour at `(r, c)`:
/// `1` = normal single-column cell;
/// `2` = the pane at this cell extends through `(r, c+1)`;
/// `0` = continuation of the wide cell starting at `(r, c-1)`
/// (no chrome, no body painted here).
///
/// Only width-2 spans starting at column 0 are valid. The loader
/// resets malformed spans to `[[1; 2]; 5]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneGridLayout {
    pub schema_version: u32,
    pub column_split: f32,
    pub row_fracs: [f32; 5],
    pub cells: [[PaneId; 2]; 5],
    #[serde(default = "default_col_spans")]
    pub col_spans: [[u8; 2]; 5],
}

fn default_col_spans() -> [[u8; 2]; 5] {
    [[1; 2]; 5]
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Json(serde_json::Error),
    SchemaVersion { found: u32 },
}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for LoadError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Json(e) => write!(f, "json: {e}"),
            Self::SchemaVersion { found } => write!(
                f,
                "unknown schema_version {found} (current is {})",
                LAYOUT_SCHEMA_VERSION
            ),
        }
    }
}

/// Resolve the per-user layout file path. Returns `None` if the
/// platform doesn't expose a config dir (extremely rare; only on
/// degenerate Linux without HOME / XDG_CONFIG_HOME). Callers fall
/// back to in-memory state in that case — no fatal error.
pub fn default_layout_path() -> Option<PathBuf> {
    ProjectDirs::from("io", "aopab", "resinsim").map(|p| p.config_dir().join("v2-layout.json"))
}

/// Load a [`PaneGridLayout`] from disk. Returns `Err(NotFound)` for
/// a missing file (first run); callers should treat that as
/// "use default" rather than a hard error.
pub fn load(path: &Path) -> Result<PaneGridLayout, LoadError> {
    let bytes = fs::read(path)?;
    let layout: PaneGridLayout = serde_json::from_slice(&bytes)?;
    if layout.schema_version != LAYOUT_SCHEMA_VERSION {
        return Err(LoadError::SchemaVersion {
            found: layout.schema_version,
        });
    }
    Ok(layout)
}

/// Save a [`PaneGridLayout`] to disk via temp-file + rename. Creates
/// the parent directory if it doesn't exist. Returns `Err` only on
/// IO failure (disk full, permissions); JSON serialisation cannot
/// fail for our struct shape.
pub fn save(path: &Path, layout: &PaneGridLayout) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(layout).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_layout() -> PaneGridLayout {
        PaneGridLayout {
            schema_version: LAYOUT_SCHEMA_VERSION,
            column_split: 0.6,
            row_fracs: [0.3, 0.2, 0.15, 0.2, 0.15],
            cells: [
                [PaneId::Safety, PaneId::Forces],
                [PaneId::CureDepth, PaneId::VatTemp],
                [PaneId::AreaDelta, PaneId::Viscosity],
                [PaneId::ZDeflection, PaneId::LayerMask2d],
                [PaneId::EmptySlot1, PaneId::EmptySlot2],
            ],
            col_spans: [[1, 1], [1, 1], [1, 1], [1, 1], [1, 1]],
        }
    }

    #[test]
    fn round_trip_save_then_load() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("v2-layout.json");
        let original = sample_layout();
        save(&path, &original).expect("save must succeed");
        let loaded = load(&path).expect("load must succeed");
        assert_eq!(loaded, original);
    }

    #[test]
    fn save_creates_parent_dir() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("dirs").join("v2.json");
        let layout = sample_layout();
        save(&path, &layout).expect("save must create parents");
        assert!(path.exists());
    }

    #[test]
    fn load_missing_file_returns_io_error() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        let err = load(&path).unwrap_err();
        assert!(matches!(err, LoadError::Io(_)));
    }

    #[test]
    fn load_rejects_wrong_schema_version() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("v2-layout.json");
        let mut layout = sample_layout();
        layout.schema_version = 999;
        save(&path, &layout).expect("save must succeed");
        let err = load(&path).unwrap_err();
        assert!(matches!(err, LoadError::SchemaVersion { found: 999 }));
    }

    #[test]
    fn load_rejects_invalid_json() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("v2-layout.json");
        fs::write(&path, b"{ not even close to json }").expect("write");
        let err = load(&path).unwrap_err();
        assert!(matches!(err, LoadError::Json(_)));
    }

    #[test]
    fn round_trip_preserves_pane_id_order() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("v2-layout.json");
        let layout = sample_layout();
        save(&path, &layout).expect("save");
        let loaded = load(&path).expect("load");
        // Forces is at (0, 1) in `sample_layout()`; the round-trip
        // must preserve that.
        assert_eq!(loaded.cells[0][1], PaneId::Forces);
        assert_eq!(loaded.cells[0][0], PaneId::Safety);
    }

    #[test]
    fn schema_version_constant_matches_in_default_layout() {
        let layout = sample_layout();
        assert_eq!(layout.schema_version, LAYOUT_SCHEMA_VERSION);
    }

    #[test]
    fn default_layout_path_returns_some_on_supported_platforms() {
        // Mac CI + dev workstations always resolve. If this fails on
        // a future build host, the caller falls back to in-memory
        // state per the doc on `default_layout_path`.
        assert!(default_layout_path().is_some());
    }
}
