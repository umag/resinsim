---
issue: cli-profile-loader-bug
date: 2026-04-18
---

# UAT: CLI name-based profile loading (ADR-0004)

## UAT-1: TOML-by-name loading (happy path)

**Rationale.** Before this issue, `report health --printer <name>` only
recognised names with a factory method on `PrinterProfile`
(`generic_msla_4k`, `elegoo_mars5_ultra`). Any TOML profile under
`data/printers/` that lacked a factory method — notably `athena_ii.toml`
— was silently ignored: the binary printed an easy-to-miss stderr warning
and used `generic_msla_4k` anyway. This led to physics results that
looked like Athena II but carried generic_msla_4k's Z-stiffness.

ADR-0004 restored the `PrinterProfileRepository::load(name)` pattern and
extended the same name-based loading to the scalar inspect subcommands.
This UAT locks the contract so a future refactor cannot regress to a
hardcoded factory-method match.

**Scenario (printer):**

Given a printer TOML at `<data-dir>/printers/athena_ii.toml` with
  `z_stiffness_n_per_mm = 1500.0`
When `resinsim report health --data-dir <data-dir> --printer athena_ii
  --stl <cube.stl>` is invoked
Then the simulation uses `z_stiffness_n_per_mm = 1500.0` (NOT the
  generic_msla_4k default of 460.0)
  And the JSON `max_z_deflection_um` reflects the athena_ii stiffness
  And no "Unknown printer profile" warning is emitted to stderr

**Scenario (resin):**

Given a resin TOML at `<data-dir>/resins/liqcreate_premium_black.toml`
When `resinsim report health --data-dir <data-dir>
  --resin liqcreate_premium_black --stl <cube.stl>` is invoked
Then the simulation uses the TOML's viscosity and Dp/Ec values
  And no "Unknown resin profile" warning is emitted

## UAT-2: Hard error on unknown profile name

**Rationale.** Silent fallback (the old behaviour) hid user typos.
ADR-0004 §Decision(d) requires that unknown names hard-error with the
list of profiles that ARE available, so the typo surfaces immediately.

**Scenario:**

Given `<data-dir>/printers/` contains `athena_ii.toml`,
  `elegoo_mars5_ultra.toml`, and `generic_msla_4k.toml`
When `resinsim report health --data-dir <data-dir>
  --printer bogus_printer_name --stl <cube.stl>` is invoked
Then the binary exits non-zero
  And stderr contains `bogus_printer_name`
  And stderr lists `athena_ii, elegoo_mars5_ultra, generic_msla_4k`
    under `Available profiles:`
  And stdout contains no JSON output

## UAT-3: Explicit scalar flag overrides profile value

**Rationale.** Users calibrating a specific printer but varying one
parameter (e.g. testing "what if Athena II had k=200 like a consumer
Mars?") should be able to override one scalar without re-entering the
entire profile. ADR-0004 §Decision(c): explicit scalar flag always wins.

**Scenario:**

Given a printer TOML `athena_ii.toml` with
  `z_stiffness_n_per_mm = 1500.0`
When `resinsim inspect zaxis --force 46.8 --printer athena_ii
  --stiffness 200 --data-dir <data-dir>` is invoked
Then the output reports `stiffness_n_per_mm = 200`
  And the computed deflection is `46.8 / 200 × 1000 = 234 µm`
    (not the profile's implied `46.8 / 1500 × 1000 = 31.2 µm`)
  And no warning or override notice is emitted (scriptability >
    chattiness)

## UAT-4: No profile flag → no resolution attempted

**Rationale.** ADR-0004 §Decision(b): data-dir resolution is triggered
ONLY when `--printer` or `--resin` is supplied. Subcommands invoked with
scalars only must proceed unchanged — no data-dir lookup, no hard error,
no change in behaviour from pre-ADR-0004.

**Scenario:**

Given `RESINSIM_DATA_DIR=/definitely/does/not/exist` is set in the
  environment
When `resinsim inspect zaxis --force 46.8 --json` is invoked
  (no `--printer`, no `--resin`)
Then the binary exits successfully
  And the output uses the built-in default stiffness of 460.0 N/mm
  And no error about the invalid RESINSIM_DATA_DIR is emitted
  (proving data-dir resolution was never attempted — the bogus env var
  would have triggered a hard error if resolution had run)

## UAT-5: Data-dir resolution chain (ADR-0004 §Decision(a))

**Rationale.** The 4-stage fallback lets developers, CI pipelines, and
deployed binaries all find the profile data without explicit
configuration. Each stage must be orderly: flag beats env beats CWD
beats binary-sibling.

**Scenario (stage a wins):**

Given `RESINSIM_DATA_DIR` points at a different valid data directory
  than the `--data-dir` flag
When the binary is invoked with `--data-dir <A>` and env `<B>`
Then profiles are loaded from `<A>` (the flag wins)

**Scenario (all stages miss):**

Given `--data-dir` is not supplied
  And `RESINSIM_DATA_DIR` is unset
  And the current working directory has no `./data/` subdirectory
  And the binary's parent directory has no `data/` sibling
When the binary is invoked with `--printer <anything>`
Then the binary exits non-zero
  And stderr lists all four candidate paths
  And stderr suggests both `--data-dir <path>` and
    `RESINSIM_DATA_DIR=<path>` as remediation
  And stderr notes the cargo-development case specifically
    ("if running via `cargo run`, invoke from the workspace root")
