//! CLI subprocess fixtures for UAT step defs.
//!
//! The `resinsim` binary lives in the `resinsim-inspect` package, so
//! `env!("CARGO_BIN_EXE_resinsim")` isn't available to the uat_gherkin
//! test binary (different package → not in its build graph). This
//! module resolves the binary via a cargo subprocess build at harness
//! startup + a `current_exe()`-based path walk at invocation time, so
//! CLI UAT scenarios can exercise the real binary end-to-end without
//! cross-package binary artifact dependencies.
//!
//! Usage: call [`ensure_resinsim_built`] once from `tests/uat_gherkin.rs::main()`
//! (before cucumber runs). Step defs call [`invoke_resinsim`] per scenario.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Resolve the repo root from `CARGO_MANIFEST_DIR`.
fn repo_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("CARGO_MANIFEST_DIR has crate + workspace + repo ancestors")
            .to_path_buf()
    })
}

/// Path to the workspace's `data/` directory (contains printers/ +
/// resins/ TOML fixtures).
pub fn workspace_data_dir() -> PathBuf {
    repo_root().join("data")
}

/// Resolve the path to the `resinsim` binary by walking up from the
/// current test executable to `target/<profile>/resinsim`. Tests run
/// via `cargo test` place the test binary at
/// `target/<profile>/deps/uat_gherkin-<hash>`, so the binary is two
/// parents up and named `resinsim`.
pub fn resinsim_bin_path() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let target_dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("test binary is under target/<profile>/deps/");
    target_dir.join("resinsim")
}

/// Build the `resinsim` binary once per test run. Called from
/// `tests/uat_gherkin.rs::main()` so CLI step defs find the binary
/// when they invoke it. cargo's build cache makes the warm-path nearly
/// instant; the first run bears the full compile cost.
pub fn ensure_resinsim_built() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let manifest = repo_root().join("Cargo.toml");
        let status = Command::new(env!("CARGO"))
            .args([
                "build",
                "--quiet",
                "--bin",
                "resinsim",
                "-p",
                "resinsim-inspect",
                "--manifest-path",
            ])
            .arg(&manifest)
            .status()
            .unwrap_or_else(|e| {
                panic!(
                    "failed to invoke `cargo build --bin resinsim` at {}: {e}",
                    manifest.display()
                )
            });
        assert!(
            status.success(),
            "`cargo build --bin resinsim` failed with exit {status:?}",
        );
        let bin = resinsim_bin_path();
        assert!(
            bin.exists(),
            "resinsim binary missing at {} after successful cargo build",
            bin.display(),
        );
    });
}

/// Result of a single CLI invocation.
#[derive(Debug, Clone)]
pub struct CliOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutcome {
    pub fn stdout_contains(&self, needle: &str) -> bool {
        self.stdout.contains(needle)
    }

    pub fn stderr_contains(&self, needle: &str) -> bool {
        self.stderr.contains(needle)
    }
}

/// Invoke the resinsim binary with `args` and optional `env` overrides.
/// Captures stdout + stderr + exit code. `env_override` entries are
/// applied on top of the inherited process env; pass `(key, "")` to
/// unset in shells where an empty value reads as unset (most do).
///
/// To unset a var explicitly (semantic difference from `""`), use
/// [`invoke_resinsim_with_unset`].
pub fn invoke_resinsim(args: &[&str], env_override: &[(&str, &str)]) -> CliOutcome {
    let bin = resinsim_bin_path();
    let mut cmd = Command::new(&bin);
    cmd.args(args);
    for (key, value) in env_override {
        cmd.env(key, value);
    }
    // Clear RUST_BACKTRACE so a panic message under it doesn't
    // accidentally match our `does NOT produce a Rust panic / stack trace`
    // assertion by printing "stack backtrace" via env rather than panic.
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

/// Like [`invoke_resinsim`] but explicitly UNSETS env vars named in
/// `env_unset` before invocation. Used for scenarios that assert
/// `RESINSIM_DATA_DIR is unset`.
pub fn invoke_resinsim_with_unset(
    args: &[&str],
    env_override: &[(&str, &str)],
    env_unset: &[&str],
) -> CliOutcome {
    let bin = resinsim_bin_path();
    let mut cmd = Command::new(&bin);
    cmd.args(args);
    for (key, value) in env_override {
        cmd.env(key, value);
    }
    for key in env_unset {
        cmd.env_remove(key);
    }
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
