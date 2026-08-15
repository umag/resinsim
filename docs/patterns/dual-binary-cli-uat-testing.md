---
issue: uat-unskip-cli-sim-rejects-tampered-sidecar
date: 2026-08-15
---

# Pattern: Dual-binary CLI UAT testing for feature-gated code paths

## Context

`ensure_resinsim_built` (cli_fixtures.rs) builds the `resinsim` binary
for CLI UAT step-def modules to subprocess. When a new module needs a
feature-gated binary (e.g. `--features resinsim-inspect/field-sim` for
sidecar producer/consumer paths), but existing modules assert
feature-off behavior (e.g. `cli_inspect_field_slices_voxel_field.rs`
asserts exit code 2 for the feature-off `inspect field` handler),
switching the single binary to feature-on breaks those assertions.

The initial approach — cfg-switching `resinsim_bin_path` to resolve a
field-sim binary under `#[cfg(feature = "field-sim")]` — broke every
existing feature-off CLI assertion under `cargo uat-field-sim`.

## Pattern

Build BOTH binaries in `ensure_resinsim_built`:

1. Default-features binary into `target/<profile>/resinsim` (unchanged).
2. Feature-variant binary into a config-scoped `target-uat-field-sim/
   <profile>/resinsim` via `--target-dir` + `--features`.

Provide two resolution functions:

- `resinsim_bin_path()` — always the default binary. All existing
  modules use this unchanged.
- `resinsim_field_sim_bin_path()` — the feature-variant binary,
  `#[cfg(feature = "field-sim")]` only.

And two invocation functions:

- `invoke_resinsim()` — default binary.
- `invoke_resinsim_field_sim()` — feature-variant binary.

The config-scoped `--target-dir` prevents `cargo uat` / `cargo
uat-field-sim` from sharing one binary path and flip-flopping rebuilds
(the SIGKILL hazard from
`docs/patterns/isolated-target-dir-for-concurrent-sessions.md`).

## When to use

- A new CLI step-def module needs production code paths that are
  `#[cfg(feature = "...")]`-gated in the subprocessed binary.
- Existing ungated CLI modules assert behavior specific to the
  default-features binary.

## See also

- `docs/patterns/isolated-target-dir-for-concurrent-sessions.md` —
  the APFS-cloned target dir pattern this extends
- `docs/patterns/band-membership-by-symbol.md` — how to verify which
  feature gate applies before writing steps
- `crates/resinsim-core/tests/uat_steps/cli_fixtures.rs` — the
  implementation
