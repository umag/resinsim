---
issue: cli-profile-loader-bug
date: 2026-04-18
---

# ADR-0004: CLI profile loading — 4-stage data-dir resolution and explicit-wins precedence

## Status
Accepted. Builds on: none. Followed by: ADR-0005 (three-axis printer/resin/recipe split),
which extends the name-based loading contract to cover the new `[recipe]` table
on `ResinProfile` and the hardware-envelope range fields on `PrinterProfile`.

## Context

The `resinsim-inspect` binary accepts profile names via `--printer <name>`
and `--resin <name>` on its `report health` subcommand. Historically these
flags were resolved by a hardcoded `match` in `cmd_report_health` against a
fixed list of factory methods (`generic_msla_4k`, `elegoo_mars5_ultra`,
`generic_standard`, `elegoo_ceramic_grey_v2`). Any profile that lives only as
a TOML file under `data/printers/` or `data/resins/` — notably `athena_ii`,
`liqcreate_premium_black`, `generic_abs_like` — silently fell back to the
factory default with a stderr warning that was easy to miss when piping
stdout JSON to `jq`.

The `resinsim-core` crate already exposes the right abstraction:
`PrinterProfileRepository::load(name)` and `ResinProfileRepository::load(name)`
read any TOML under a given data directory. The binary was bypassing these
repositories. This ADR captures the decision to restore the repository
pattern and extend name-based profile loading to the scalar `inspect`
subcommands as an explicit-wins convenience layer.

## Decision

1. **4-stage data-dir resolution.** The binary resolves the profile data
   directory via a fallback chain, stopping at the first stage that yields
   an existing directory:

   - (a) `--data-dir <path>` CLI flag (per subcommand)
   - (b) `$RESINSIM_DATA_DIR` environment variable
   - (c) `$CWD/data/` relative to the current working directory
   - (d) `data/` as a sibling of the binary, resolved via
     `std::env::current_exe()`

   **Stage (d) is a deployment-mode fallback.** It does NOT resolve during
   cargo-driven development: `target/debug/resinsim` has no sibling
   `target/debug/data/`. Developers invoking `cargo run` must rely on
   stages (a)–(c). The hard-error message below includes a cargo-specific
   remediation hint for this case.

   If none of the four stages resolves and data-dir resolution is required
   (see §Decision(b) below), the binary hard-errors with a message listing
   each candidate and whether it existed, plus the remediation hint.
   `env::current_exe()` failures at stage (d) are handled silently —
   the stage is skipped, not propagated.

2. **Resolution is only triggered when a profile is named.** Data-dir
   resolution runs if and only if the invocation supplies `--printer` or
   `--resin` on a subcommand that accepts them. Subcommands invoked without
   either flag stay on built-in constants and skip resolution entirely.
   No hard error fires for missing data-dir when no profile is named.
   This preserves the no-regression property for scripts that use the
   scalar inspect subcommands with explicit physics arguments only.

3. **Explicit scalar flags always win.** When a profile is loaded and the
   user ALSO supplies an explicit scalar flag (e.g. `--printer athena_ii
   --stiffness 1234`), the explicit value wins. No warning is emitted on
   override — scriptability is favoured over chattiness. The precedence
   order for any scalar `--foo`:

   ```
   explicit --foo value    → use as-is (always wins)
   else if profile loaded  → use the corresponding profile field
   else                    → use built-in default constant
   ```

4. **Unknown profile name is a hard error.** When data-dir resolves but
   `name.toml` is not present, the binary emits an error that (i) states
   the data-dir that was used and (ii) lists the profile names that ARE
   available via `PrinterProfileRepository::list()` /
   `ResinProfileRepository::list()`. This surfaces typos without forcing
   the user to `ls data/printers/` manually.

## Consequences

- `cmd_report_health` no longer needs the hardcoded match. Any TOML under
  `data/printers/` or `data/resins/` is usable by name — adding a new
  profile is a pure data change with no code change.
- Scalar `inspect` subcommands (`cure`, `force`, `thermal`, `zaxis`) accept
  optional `--printer` / `--resin` / `--data-dir` flags. Users calibrating
  against a specific printer/resin combination can now name the profile and
  only override the scalars they want to vary.
- `--stiffness`, `--layer-height`, `--exposure`, `--lift-cycle`,
  `--delta-t`, `--tau`, `--viscosity`, `--ea`, `--sigma`, `--tensile`,
  `--speed`, `--ref-speed`, `--dp`, `--ec` become `Option<f32>` at the clap
  layer. The built-in fallback constants move from clap defaults into the
  handler. `--help` text notes the constant via a doc comment.
- `inspect cure` uses `required_unless_present = "resin"` on `--dp` and
  `--ec` to keep the existing "either scalars or profile" contract clear at
  parse time.
- The `generic_msla_4k` / `generic_standard` factory methods on
  `PrinterProfile` / `ResinProfile` remain in the core crate (needed by
  tests and direct library callers) but become dead code for CLI callers.
  Removal is deferred — a follow-up cleanup, if desired.

## Alternatives considered

- **Flag-only resolution (`--data-dir` is mandatory when `--printer`/`--resin`
  is set).** Rejected. Forces every invocation to supply the path even
  when a conventional location would work. High friction for the common
  case of running from the workspace root where `$CWD/data` is the answer.

- **Env-var-only resolution.** Rejected. Env vars are invisible in shell
  history and CI logs; debugging a misconfigured `RESINSIM_DATA_DIR` is
  harder than debugging a visible `--data-dir` argument. Env vars work
  well as one layer of the chain but not as the only layer.

- **`load_or_default(name)` silent fallback.** Rejected. This was effectively
  the old behaviour (factory fallback when match missed); the whole
  motivation for this ADR was the resulting silent misalignment (the
  6cm-cube test showed Athena II silently replaced with generic_msla_4k
  stiffness, giving a physics result that looked like Athena but was the
  default). Hard-error is the correct behaviour when the user named a
  specific profile.

- **Global clap args** (`--data-dir` / `--printer` / `--resin` on the
  top-level `Cli` struct so they apply to every subcommand). Rejected.
  Would attach the flags to subcommands that don't use profiles
  (`inspect athena`, `inspect layers`) creating noise in their `--help`
  output. Per-subcommand flags are more verbose but clearer about which
  subcommands support the profile convenience layer.

- **Emit stderr notification on explicit-over-profile override.** Rejected
  (judgment call — see round-1 UX finding). Developers scripting against
  the CLI can pipe stderr selectively, but the default should not add
  chatter. The precedence is documented in this ADR and in `--help` text;
  users who want to inspect which values came from where can diff two
  invocations or read the resolved config back from the JSON output.
