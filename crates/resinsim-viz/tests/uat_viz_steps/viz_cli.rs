//! Subprocess invocation of the `resinsim-viz` binary for viz UAT step
//! defs.
//!
//! DESIGN mirrors `resinsim-core`'s `tests/uat_steps/cli_fixtures.rs`
//! (`CliOutcome`, `env_remove("RUST_BACKTRACE")` hygiene) but the CODE is
//! NOT reused, deliberately: `cli_fixtures.rs`'s entire reason for
//! existing is that the `resinsim` binary lives in a DIFFERENT package
//! (`resinsim-inspect`), so `env!("CARGO_BIN_EXE_resinsim")` isn't
//! available to `uat_gherkin` (different package → not in its build
//! graph) — forcing a `cargo build` subprocess
//! (`ensure_resinsim_built`), a `current_exe()` parent-directory walk,
//! and an `RUSTC_BOOTSTRAP=1` workaround. This harness lives INSIDE
//! resinsim-viz, so `env!("CARGO_BIN_EXE_resinsim-viz")` resolves for
//! free, with cargo guaranteeing the binary is built and fresh before
//! this test binary runs at all — the same mechanism resinsim-inspect's
//! own CLI tests already use (`field_inspect_cli.rs`,
//! `report_health_time_cli.rs`, `thermal_cli_warnings.rs`). All of
//! `cli_fixtures.rs`'s ~100-line apparatus evaporates; this file is
//! ~40 lines.
//!
//! `env!("CARGO_BIN_EXE_resinsim-viz")` keeps the hyphen — cargo does not
//! sanitise bin names for this variable, and `env!` takes a string
//! literal, so the hyphen is fine as written. (A reflexive underscore
//! substitution here would produce a compile-time "environment variable
//! not defined" error that reads like the mechanism is unavailable, when
//! only the spelling is wrong.)

use std::path::Path;
use std::process::Command;

/// Result of a single `resinsim-viz` CLI invocation.
#[derive(Debug, Clone)]
pub struct CliOutcome {
    pub exit_code: i32,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
}

impl CliOutcome {
    pub fn stderr_contains(&self, needle: &str) -> bool {
        self.stderr.contains(needle)
    }
}

/// Invoke `resinsim-viz` with `args`, capturing stdout + stderr + exit
/// code. Every tier-B scenario opens a real window for a few seconds —
/// see docs/adr/0024-second-uat-harness-in-resinsim-viz.md for why that
/// is an accepted developer-machine cost (no CI in this repo today).
pub fn invoke_viz(args: &[&str]) -> CliOutcome {
    let bin = Path::new(env!("CARGO_BIN_EXE_resinsim-viz"));
    let mut cmd = Command::new(bin);
    cmd.args(args);
    // Same hygiene as cli_fixtures.rs::invoke_resinsim: clear
    // RUST_BACKTRACE so a panic backtrace under it can't accidentally
    // satisfy a stderr substring assertion by coincidence.
    cmd.env_remove("RUST_BACKTRACE");
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", bin.display()));
    CliOutcome {
        exit_code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}
