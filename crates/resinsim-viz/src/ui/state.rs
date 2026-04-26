//! Picker state + profile cache + the `run_block_reason` pure helper.
//!
//! Cached profiles (`loaded_resin` / `loaded_printer`) refresh only
//! on selection-change so the egui render path never touches disk.
//! See ADR-0011 for the cache rationale.

use bevy::prelude::*;
use resinsim_core::entities::{PrinterProfile, ResinProfile};

use crate::profile_repos::ProfileRepos;
use crate::sim::RunSimRequest;

#[derive(Resource, Default)]
pub struct PickerState {
    pub selected_resin: Option<String>,
    pub selected_printer: Option<String>,
    pub available_resins: Vec<String>,
    pub available_printers: Vec<String>,
    /// Cached profile loaded from `selected_resin`. Refreshed by
    /// `refresh_loaded_profiles` on selection change; never read
    /// directly by panel render code without going through this.
    pub loaded_resin: Option<ResinProfile>,
    pub loaded_printer: Option<PrinterProfile>,
}

/// Populate `available_*` from the repos. Preserves a current
/// selection when it still appears in the new listing; clears it
/// otherwise so the ComboBox doesn't dangle on a removed name.
pub fn refresh_listings(state: &mut PickerState, repos: &ProfileRepos) -> Result<(), String> {
    let resin_names = repos.resin.list()?;
    let printer_names = repos.printer.list()?;
    if let Some(sel) = &state.selected_resin
        && !resin_names.contains(sel)
    {
        state.selected_resin = None;
        state.loaded_resin = None;
    }
    if let Some(sel) = &state.selected_printer
        && !printer_names.contains(sel)
    {
        state.selected_printer = None;
        state.loaded_printer = None;
    }
    state.available_resins = resin_names;
    state.available_printers = printer_names;
    Ok(())
}

/// Idempotent profile cache: loads a fresh profile only when the
/// selected name differs from the cached one. Equal names mean no
/// mutation — preventing `is_changed`/refresh ping-pong even though
/// the system writes through `&mut`.
pub fn refresh_loaded_profiles(state: &mut PickerState, repos: &ProfileRepos) {
    let resin_needs = state.selected_resin.as_deref()
        != state.loaded_resin.as_ref().map(|r| r.name());
    if resin_needs {
        state.loaded_resin = match &state.selected_resin {
            Some(name) => repos.resin.load(name).ok(),
            None => None,
        };
    }
    let printer_needs = state.selected_printer.as_deref()
        != state.loaded_printer.as_ref().map(|p| p.name());
    if printer_needs {
        state.loaded_printer = match &state.selected_printer {
            Some(name) => repos.printer.load(name).ok(),
            None => None,
        };
    }
}

impl PickerState {
    /// Build a `RunSimRequest` from current selections; `None` when
    /// either is unset (Run button disabled in that state).
    pub fn to_run_request(&self) -> Option<RunSimRequest> {
        let resin = self.selected_resin.as_ref()?.clone();
        let printer = self.selected_printer.as_ref()?.clone();
        Some(RunSimRequest { resin, printer })
    }
}

/// Pure helper: human-readable reason the Run button is disabled,
/// or `None` when ready. Lists exactly the missing items joined
/// with commas — natural single-item phrasing, full list on
/// 2+ blockers.
pub fn run_block_reason(picker: &PickerState, has_ctb: bool) -> Option<String> {
    let mut missing: Vec<&'static str> = Vec::with_capacity(3);
    if !has_ctb {
        missing.push("drag in a .ctb file");
    }
    if picker.selected_resin.is_none() {
        missing.push("pick a resin profile");
    }
    if picker.selected_printer.is_none() {
        missing.push("pick a printer profile");
    }
    if missing.is_empty() {
        None
    } else {
        // Capitalise first letter of joined sentence.
        let joined = missing.join(", ");
        let mut chars = joined.chars();
        let cap = match chars.next() {
            Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
            None => String::new(),
        };
        Some(cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_data_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
    }

    fn shipped_repos() -> ProfileRepos {
        ProfileRepos::new(&workspace_data_dir())
    }

    #[test]
    fn refresh_listings_populates_from_shipped_data() {
        let mut state = PickerState::default();
        refresh_listings(&mut state, &shipped_repos())
            .expect("test fixture: shipped data/ resolves");
        assert!(state.available_resins.contains(&"generic_standard".to_string()));
        assert!(
            state
                .available_printers
                .contains(&"generic_msla_4k".to_string())
        );
    }

    #[test]
    fn refresh_listings_clears_selection_when_name_disappears() {
        let mut state = PickerState {
            selected_resin: Some("ghost_resin".to_string()),
            ..Default::default()
        };
        refresh_listings(&mut state, &shipped_repos())
            .expect("test fixture: shipped data/ resolves");
        assert!(
            state.selected_resin.is_none(),
            "selection must clear when the name is no longer in the listing"
        );
    }

    #[test]
    fn refresh_loaded_profiles_loads_on_selection_change() {
        let mut state = PickerState {
            selected_resin: Some("generic_standard".to_string()),
            selected_printer: Some("generic_msla_4k".to_string()),
            ..Default::default()
        };
        refresh_loaded_profiles(&mut state, &shipped_repos());
        assert_eq!(
            state.loaded_resin.as_ref().map(|r| r.name()),
            Some("Generic Standard")
        );
        assert_eq!(
            state.loaded_printer.as_ref().map(|p| p.name()),
            Some("Generic MSLA 4K")
        );
    }

    #[test]
    fn refresh_loaded_profiles_is_idempotent() {
        let mut state = PickerState {
            selected_resin: Some("generic_standard".to_string()),
            selected_printer: Some("generic_msla_4k".to_string()),
            ..Default::default()
        };
        refresh_loaded_profiles(&mut state, &shipped_repos());
        // Capture pointers — Rust doesn't expose stable allocator
        // identity for cloned values, so just re-run and assert the
        // names still match (no clobbering or no-op confusion).
        let resin_name_before = state.loaded_resin.as_ref().map(|r| r.name().to_string());
        let printer_name_before = state.loaded_printer.as_ref().map(|p| p.name().to_string());
        refresh_loaded_profiles(&mut state, &shipped_repos());
        let resin_name_after = state.loaded_resin.as_ref().map(|r| r.name().to_string());
        let printer_name_after = state.loaded_printer.as_ref().map(|p| p.name().to_string());
        assert_eq!(resin_name_before, resin_name_after);
        assert_eq!(printer_name_before, printer_name_after);
    }

    #[test]
    fn refresh_loaded_profiles_clears_when_selection_unset() {
        let mut state = PickerState {
            selected_resin: Some("generic_standard".to_string()),
            ..Default::default()
        };
        refresh_loaded_profiles(&mut state, &shipped_repos());
        assert!(state.loaded_resin.is_some());
        state.selected_resin = None;
        refresh_loaded_profiles(&mut state, &shipped_repos());
        assert!(
            state.loaded_resin.is_none(),
            "loaded_resin must clear when selected_resin is set back to None"
        );
    }

    #[test]
    fn to_run_request_some_only_when_both_selected() {
        let mut state = PickerState::default();
        assert!(state.to_run_request().is_none());
        state.selected_resin = Some("a".into());
        assert!(state.to_run_request().is_none());
        state.selected_printer = Some("b".into());
        let req = state
            .to_run_request()
            .expect("both selections set — must produce a RunSimRequest");
        assert_eq!(req.resin, "a");
        assert_eq!(req.printer, "b");
    }

    #[test]
    fn run_block_reason_ready_when_all_set() {
        let state = PickerState {
            selected_resin: Some("a".into()),
            selected_printer: Some("b".into()),
            ..Default::default()
        };
        assert!(run_block_reason(&state, true).is_none());
    }

    #[test]
    fn run_block_reason_lists_only_missing_ctb() {
        let state = PickerState {
            selected_resin: Some("a".into()),
            selected_printer: Some("b".into()),
            ..Default::default()
        };
        let reason = run_block_reason(&state, false)
            .expect("missing CTB only — must produce a reason");
        assert!(reason.starts_with("Drag"));
        assert!(reason.contains(".ctb"));
        assert!(!reason.contains("resin"));
        assert!(!reason.contains("printer"));
    }

    #[test]
    fn run_block_reason_lists_only_missing_resin() {
        let state = PickerState {
            selected_printer: Some("b".into()),
            ..Default::default()
        };
        let reason = run_block_reason(&state, true)
            .expect("missing resin only — must produce a reason");
        assert!(reason.starts_with("Pick a resin"));
        assert!(!reason.contains("printer"));
        assert!(!reason.contains(".ctb"));
    }

    #[test]
    fn run_block_reason_lists_only_missing_printer() {
        let state = PickerState {
            selected_resin: Some("a".into()),
            ..Default::default()
        };
        let reason = run_block_reason(&state, true)
            .expect("missing printer only — must produce a reason");
        assert!(reason.starts_with("Pick a printer"));
        assert!(!reason.contains("resin"));
        assert!(!reason.contains(".ctb"));
    }

    #[test]
    fn run_block_reason_lists_all_missing() {
        let state = PickerState::default();
        let reason = run_block_reason(&state, false)
            .expect("nothing set — must produce a reason");
        assert!(reason.contains(".ctb"));
        assert!(reason.contains("resin"));
        assert!(reason.contains("printer"));
        assert!(reason.contains(", "));
    }
}
