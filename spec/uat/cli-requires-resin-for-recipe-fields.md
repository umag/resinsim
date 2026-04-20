---
issue: resin-recipe-model
date: 2026-04-21
---

# UAT: `resinsim inspect zaxis` sources layer_height from resin recipe

## UAT-1: `--resin` supplies layer_height for zaxis subcommand

**Rationale.** Before ADR-0005, `resinsim inspect zaxis --printer <name>` sourced
`layer_height_um` from the printer profile. ADR-0005 moved `layer_height_um` onto
`ResinProfile.recipe`, so the CLI subcommand grew a `--resin <name>` flag with
the same profile-sourced-default precedence (ADR-0004 §Decision(c)):
explicit `--layer-height` → resin profile → built-in default (50.0 µm).

This UAT locks the new CLI contract: passing `--resin` sources layer_height from
the resin's recipe; omitting both `--resin` and `--layer-height` falls back to
50.0 µm (no silent error, no panic).

**Scenario (profile-sourced):**

Given the user invokes
  `resinsim inspect zaxis --force 50 --resin generic_standard --data-dir <dir> --json`
When the binary resolves the profile (ADR-0004 4-stage data-dir chain)
Then `layer_height` in the JSON output equals `50.0` (from
     `ResinProfile::generic_standard().recipe().layer_height_um()`)
  And no error is printed to stderr

**Scenario (explicit flag wins over resin):**

Given the same command with `--layer-height 30.0` also supplied
When the binary runs
Then `layer_height` in the JSON output equals `30.0`
  And the resin's recipe value is ignored in favour of the explicit flag

## UAT-2: No `--resin`, no `--layer-height` — built-in default applies

**Rationale.** For existing users who invoke `inspect zaxis --printer <name>`
without `--resin` (matching the pre-ADR-0005 CLI), the subcommand must still
return a sensible result rather than erroring out. The built-in 50 µm default
is appropriate: zaxis is a scalar inspector, and 50 µm is the MSLA 4K norm.

**Scenario:**

Given `resinsim inspect zaxis --force 50 --printer generic_msla_4k --data-dir <dir> --json`
  with no `--resin` or `--layer-height` flag
When the binary runs
Then `layer_height` in the JSON output equals `50.0` (the built-in default)
  And no error is printed to stderr
  And the subcommand exits 0
