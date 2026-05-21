---
issue: t2f4-thermal-diffusion (discovered)
relates: t2f2-light-crosstalk-convolution (the affected feature)
date: 2026-05-21
status: anti-pattern
---

# Anti-pattern: TOML inline keys after a [table] header silently nest into that table

## Context

TOML's scoping rule: once a `[table.header]` line appears in a file,
all subsequent `key = value` lines are interpreted as
`table.header.key = value` UNTIL another table header or end-of-file
changes the scope. This is well-documented but easy to forget when
APPENDING new top-level fields to an existing TOML.

When the target Rust struct has `#[serde(deny_unknown_fields)]`, the
mistake is caught at parse time with a clear error. When the struct
ACCEPTS unknown fields (the default serde behaviour), the misplaced
keys are silently DROPPED — they parse fine, but they don't end up
in the deserialised value.

## The mistake

`data/printers/elegoo_mars5_ultra.toml` placed
`crosstalk_sigma_xy_um = 8.0` and `crosstalk_sigma_z_um = 40.0`
AFTER the `[build_envelope_mm]` block:

```toml
# ... top-level scalars ...

[build_envelope_mm]
width_mm = 153.36
depth_mm = 77.76
max_z_mm = 165.0

crosstalk_sigma_xy_um = 8.0  # WRONG — parsed as build_envelope_mm.crosstalk_sigma_xy_um
crosstalk_sigma_z_um = 40.0  # WRONG — likewise
```

Verified via `tomllib.load(...)` in Python:

```python
{'top-level keys': ['name', 'led_power_mw_cm2', ..., 'build_envelope_mm'],
 'build_envelope_mm subkeys': ['width_mm', 'depth_mm', 'max_z_mm',
                                'crosstalk_sigma_xy_um',  # silently nested!
                                'crosstalk_sigma_z_um']}
```

`PrinterProfile` does NOT have `#[serde(deny_unknown_fields)]`, so
the nested keys parsed successfully into `BuildEnvelope` (which
ignores them), and `PrinterProfile.crosstalk_sigma_xy_um` /
`PrinterProfile.crosstalk_sigma_z_um` were `None` for every Mars 5
Ultra run from 2026-05-19 through 2026-05-21.

**Impact**: t2f2 light crosstalk was silently disabled on Mars 5
Ultra for ~2 days. Mitigated 2026-05-21 by restructuring the TOML.

## The fix

Move all top-level scalar keys BEFORE any `[table]` block:

```toml
# ... all top-level scalars including crosstalk_sigma_*, thermal_*, etc. ...
crosstalk_sigma_xy_um = 8.0
crosstalk_sigma_z_um = 40.0

[build_envelope_mm]
width_mm = 153.36
depth_mm = 77.76
max_z_mm = 165.0
```

Per the TOML spec: once you enter a table, you can only emit subkeys
of that table OR start a new table. There is no "exit table back to
top-level" syntax. Order matters.

## Detection

Three options:

1. **Parse-time enforcement via serde.** Add
   `#[serde(deny_unknown_fields)]` on `PrinterProfile` AND on every
   nested table struct (`BuildEnvelope`, ...). Catches the bug at
   load time with a clear error: "unknown field
   `crosstalk_sigma_xy_um` for `BuildEnvelope`, expected one of
   `width_mm`, `depth_mm`, `max_z_mm`". **Trade-off**: blocks future
   field-additions until every TOML migrates — interferes with
   cross-feature TOML interchange. ResinSim chose NOT to enforce
   (per the cross-feature interchange pattern from t2f4 step 2);
   the round-trip test below is the fallback.

2. **Round-trip test.** For each shipped TOML, parse it via
   `toml::from_str` then check that each `Some` field documented in
   the struct ALSO appears as a top-level key in the parsed
   `toml::Value::Table` representation. Add to test fixtures of any
   new TOML. Cheap, catches future mistakes.

3. **Linter on TOML structure.** A pre-commit hook that runs `python3
   -c "import tomllib; ..."` on every TOML in `data/` and reports
   unexpected nesting. Higher-friction than option 2.

ResinSim uses options 1 (lenient — `deny_unknown_fields` off) +
implicit-2 (the validate-time check on `Some(...)` fields catches
*some* of these because a `None` value flags as missing under
field-sim). The detection is INDIRECT: had the t2f4 work not
restructured the TOML to add new fields BEFORE `[build_envelope_mm]`,
the crosstalk-nested bug would not have been discovered.

## See also

- TOML spec §"Tables" — scoping rules
- `docs/patterns/required-under-feature-via-option-plus-validate.md`
  — the cross-feature TOML interchange pattern that justifies
  keeping `deny_unknown_fields` off
- Restored configuration: `data/printers/elegoo_mars5_ultra.toml`
  (the t2f4 commit comment documents the find)
