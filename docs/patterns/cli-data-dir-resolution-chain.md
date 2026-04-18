---
issue: cli-profile-loader-bug
date: 2026-04-18
---

# Pattern: 4-stage data-dir resolution for CLI binaries

## Problem

A CLI binary ships with a companion data directory (config profiles, TOML
fixtures, seed data). The binary must locate the directory reliably across
three contexts that have different conventions:

1. **Development** — invoked via `cargo run` from the workspace root
2. **CI / test harness** — invoked from a subdirectory; `$CWD` varies
3. **Installed** — binary at `/usr/local/bin/foo` with `/usr/local/bin/data/`
   as a sibling, user's `$CWD` unrelated

No single strategy fits all three. A flag-only approach forces friction on
the common case; an env-only approach is invisible in shell history; an
implicit-only approach breaks when the binary moves.

## The pattern

Provide a fallback chain with four stages, stopping at the first that
resolves to an existing directory:

1. **Explicit flag** — `--data-dir <path>`. Highest precedence; most
   explicit; visible in shell history.
2. **Environment variable** — e.g. `RESINSIM_DATA_DIR`. Useful for CI and
   test harnesses that want to set once per session.
3. **CWD-relative** — `$CWD/data/`. Works for the "cd to workspace root
   and run" common case.
4. **Binary-sibling** — `<std::env::current_exe parent>/data/`. The
   deployment-mode fallback for installed binaries.

On miss, hard-error with a message that (i) lists every candidate tried and
(ii) suggests each remediation (flag, env). If the binary is commonly run
via `cargo run`, include a cargo-specific hint ("invoke from the workspace
root or pass --data-dir explicitly").

`env::current_exe()` can fail on some platforms (jails, stripped /proc).
Handle the error by **silently skipping stage 4**, not propagating — the
hard-error chain continues to the other stages.

## Rust reference implementation

```rust
pub fn resolve_data_dir(flag: Option<&Path>) -> Result<PathBuf, String> {
    // (a) flag
    if let Some(p) = flag && p.is_dir() { return Ok(p.to_path_buf()); }
    // (b) env
    if let Some(p) = std::env::var("MYBIN_DATA_DIR").ok().map(PathBuf::from)
        && p.is_dir() { return Ok(p); }
    // (c) CWD/data
    if let Some(p) = std::env::current_dir().ok().map(|c| c.join("data"))
        && p.is_dir() { return Ok(p); }
    // (d) binary/data — deployment mode, no-op during cargo dev
    if let Some(p) = std::env::current_exe().ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .map(|d| d.join("data"))
        && p.is_dir() { return Ok(p); }
    Err("…list candidates and remediation hints…".into())
}
```

## When to trigger resolution

A subtle-but-important consequence: **only call `resolve_data_dir` when
the invocation actually needs the data directory**. If the CLI has
subcommands or flag combinations that don't reference any data file,
don't call the resolver — a misconfigured env var or missing data
directory shouldn't fail an invocation that never would have read a
file.

In resinsim, this meant calling `resolve_data_dir` only when `--printer`
or `--resin` was supplied to the scalar `inspect` subcommands. Subcommands
invoked with scalar flags only stayed on built-in defaults with no
resolution attempted.

## First-party example

resinsim-inspect implements this pattern at
`crates/resinsim-inspect/src/profile_loader.rs`. Decision captured in
[ADR-0004: CLI profile loading](../adr/0004-cli-profile-loading.md).
Contract locked at [spec/uat/cli-profile-by-name-loading.md](../../spec/uat/cli-profile-by-name-loading.md).

## Trade-offs

- **Silent override of explicit over profile** — explicit scalar flags
  always win over profile-sourced defaults with no stderr notification.
  Scriptability > chattiness; users who want to audit which values came
  from where can diff two invocations with/without `--printer`.
- **Stage 4 is useless during `cargo run`** — `target/debug/<binary>` has
  no sibling `data/`. Stage 4 only matters for deployed binaries. The
  hard-error message should call this out explicitly.
