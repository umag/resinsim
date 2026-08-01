---
issue: t2f6-field-inspector
date: 2026-07-28
---

# UAT: `resinsim inspect field` — voxel field slice inspector CLI contract

**Scope note.** `cargo uat` runs against a default-features build (no
`field-sim`), so every scenario below targets surfaces that are
executable feature-off: the visible-subcommand contract, the
feature-off actionable-error handler, and parse-time `--slice` /
`--field` validation (both are plain data / functions with no
`field-sim` dependency — ADR-0023). The voxel-data happy path (a real
slice rendered against a populated `sim.json` + sidecar) is covered by
the feature-on integration suite
(`crates/resinsim-inspect/tests/field_inspect_cli.rs`), which `cargo
uat` structurally cannot reach.

## UAT-1: `field` subcommand stays listed in `inspect --help` feature-off

**Rationale.** ADR-0023's feature-off UX decision deliberately diverges
from the `--voxel-cure-mm` bare-unknown-flag precedent: a whole missing
subcommand is a discoverability dead end, so `field` stays in the clap
tree and lists in `--help` in every build.

```gherkin
Scenario: UAT-1 field subcommand listed in inspect --help feature-off
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect --help"
  Then stdout lists the field subcommand
```

## UAT-2: `inspect field --help` lists every flag feature-off

```gherkin
Scenario: UAT-2 inspect field --help lists every flag feature-off
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --help"
  Then stdout lists every field subcommand flag
```

## UAT-3: feature-off handler exits 2 naming the feature and the rebuild command

**Rationale.** The handler body is the ONLY `#[cfg]`-split part of the
subcommand (plan step 6); its feature-off branch must name the exact
Cargo feature and rebuild command so a user hitting this hits an
actionable message, not a dead end.

```gherkin
Scenario: UAT-3 field handler exits 2 naming field-sim and rebuild command
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --in x.sim.json --field cure --slice z=0"
  Then the field subcommand exits with code 2
  And stderr names the field-sim Cargo feature
  And stderr names the exact rebuild command
  And the field subcommand's stdout stays empty
```

## UAT-4: the feature-off error is byte-for-byte the same shape under `--json`

**Rationale.** Review-ux binding condition: error behaviour under
`--json` must match the sibling convention exactly — prose to stderr,
empty stdout, nonzero exit — with NO special JSON error envelope.

```gherkin
Scenario: UAT-4 feature-off error unchanged under --json
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --in x.sim.json --field cure --slice z=0 --json"
  Then the field subcommand exits with code 2
  And the field subcommand's stdout stays empty
  And stderr names the field-sim Cargo feature
```

## UAT-5: a malformed `--slice` spec is rejected at parse time, before the handler body runs

**Rationale.** `parse_slice_spec` is plain data with no `field-sim`
dependency (mirrors `parse_voxel_cure_mm`'s parse-time-validation
register), so a malformed spec must be rejected identically in every
build — the rejection happens inside clap, before the feature-off
handler body is ever reached.

```gherkin
Scenario: UAT-5 malformed --slice rejected at parse time feature-off
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --in x.sim.json --field cure --slice not-a-valid-spec"
  Then the field subcommand exits with a nonzero code
  And the field subcommand does not panic
```

## UAT-6: a negative `--slice` value is rejected at parse time with the constraint explained

```gherkin
Scenario: UAT-6 negative --slice value rejected at parse time
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --in x.sim.json --field cure --slice z=-1"
  Then the field subcommand exits with a nonzero code
  And stderr explains the non-negative constraint on the slice value
```

## UAT-7: an unknown `--field` value is rejected by clap

```gherkin
Scenario: UAT-7 unknown --field value rejected by clap
  Given the default feature-off resinsim build
  When the user invokes the field inspector as "resinsim inspect field --in x.sim.json --field not-a-real-field --slice z=0"
  Then the field subcommand exits with a nonzero code
  And stderr names the invalid field value
```
