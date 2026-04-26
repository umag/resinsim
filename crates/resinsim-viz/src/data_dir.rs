//! 4-stage data-directory resolver for resinsim-viz.
//!
//! Mirrors `resinsim-inspect`'s `profile_loader::resolve_data_dir`
//! (ADR-0004) verbatim — flag → `RESINSIM_DATA_DIR` env → `$CWD/data` →
//! exe-sibling `data/`. The viz crate cannot depend on
//! `resinsim-inspect` per ADR-0010 (one-way layering: viz → core only),
//! so the chain is duplicated here. See `docs/adr/0011-egui-control-panels.md`
//! for the duplication rationale; the pattern reference lives at
//! `docs/patterns/cli-data-dir-resolution-chain.md`.

use std::path::{Path, PathBuf};

const DATA_DIR_ENV: &str = "RESINSIM_DATA_DIR";

/// Resolve the data directory via the 4-stage chain (ADR-0004 / ADR-0011).
///
/// Returns `Ok(path)` for the first stage that yields an existing
/// directory. Returns `Err(message)` listing each candidate on
/// total miss; the message includes a cargo-specific remediation hint.
pub fn resolve_data_dir(flag: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates: Vec<(String, Option<PathBuf>)> = Vec::with_capacity(4);

    let stage_a = flag.map(Path::to_path_buf);
    candidates.push(("--data-dir flag".to_string(), stage_a.clone()));
    if let Some(p) = stage_a
        && p.is_dir()
    {
        return Ok(p);
    }

    let stage_b = std::env::var(DATA_DIR_ENV).ok().map(PathBuf::from);
    candidates.push((format!("${DATA_DIR_ENV}"), stage_b.clone()));
    if let Some(p) = stage_b
        && p.is_dir()
    {
        return Ok(p);
    }

    let stage_c = std::env::current_dir().ok().map(|c| c.join("data"));
    candidates.push(("$CWD/data".to_string(), stage_c.clone()));
    if let Some(p) = stage_c
        && p.is_dir()
    {
        return Ok(p);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_a_flag_wins_when_dir_exists() {
        let tmp = tempfile::tempdir().expect("test fixture: tempdir creation");
        let resolved = resolve_data_dir(Some(tmp.path()))
            .expect("test fixture: flag pointing at an existing tempdir resolves");
        assert_eq!(resolved, tmp.path());
    }

    #[test]
    fn nonexistent_flag_does_not_resolve_to_flag_path() {
        // Stages b/c/d may or may not resolve depending on the test
        // environment (env var set, CWD has ./data, etc.). Either way,
        // the bogus flag must not be returned as the resolution.
        let bogus = Path::new("/definitely/does/not/exist/resinsim-viz-test");
        match resolve_data_dir(Some(bogus)) {
            Ok(p) => assert_ne!(
                p, bogus,
                "stage (a) with a nonexistent path must not resolve to it"
            ),
            Err(msg) => {
                assert!(
                    msg.contains("--data-dir flag"),
                    "error must list the flag stage; got: {msg}"
                );
                assert!(
                    msg.contains("RESINSIM_DATA_DIR"),
                    "error must list the env stage; got: {msg}"
                );
            }
        }
    }

    #[test]
    fn err_message_lists_all_four_stages() {
        // Construct a definitely-not-resolving call: bogus flag.
        // The error path runs only when none of (b)(c)(d) resolved
        // either — most CI environments satisfy that. Skip when the
        // local env happens to resolve one of them.
        let bogus = Path::new("/nonexistent/resinsim-viz-stages-test");
        let Err(msg) = resolve_data_dir(Some(bogus)) else {
            return; // env resolves a later stage; not a failure
        };
        assert!(msg.contains("--data-dir flag"));
        assert!(msg.contains("RESINSIM_DATA_DIR"));
        assert!(msg.contains("$CWD/data"));
        assert!(msg.contains("<binary parent>/data"));
        assert!(msg.contains("Remediation:"));
    }
}
