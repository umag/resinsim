---
issue: 10-build-plate-and-volume-cube
date: 2026-04-26
---

# ADR-0012: Optional `build_envelope_mm` on `PrinterProfile` — extends ADR-0005 Axis 1

## Status
Accepted

## Context

ADR-0005 split printer / resin / recipe into three axes. Axis 1
("hardware envelope") landed on `PrinterProfile` as ranges and
calibration data: `layer_height_range_um`, `exposure_range_sec`,
`lift_speed_range_mm_min`, `bottom_layer_count_max`, optical and
thermal parameters.

Build-volume dimensions (X / Y / max-Z extents in mm) were NOT
captured on `PrinterProfile`. They are also not in any other
domain entity. Today they enter the system only via the CTB header
(`SlicedFileInfo.bed_size_mm` carries X / Y; max-Z is absent from CTB
v3 entirely).

Issue 10 needs the build envelope to position the rendered build
plate at world Z = `envelope.max_z` and to outline the print volume
in the viz. CTB header data is insufficient because:

- Z extent is missing.
- The CTB file is optional: the user may run the viz without loading
  a CTB at all, in which case there is no envelope source.

## Decision

Add an OPTIONAL build-envelope field to `PrinterProfile`:

```rust
pub struct BuildEnvelope {
    pub width_mm: f32,
    pub depth_mm: f32,
    pub max_z_mm: f32,
}

pub struct PrinterProfile {
    // ... existing fields ...
    pub(crate) build_envelope_mm: Option<BuildEnvelope>,
}
```

### Why optional

A `PrinterProfile` may be authored before its dimensions are
confirmed. In v1 the human explicitly directed "skip athena ii for
now" — `data/printers/athena_ii.toml` ships without the field, and
viz falls back to CTB header bed_size_mm + sentinel max_z + a
warn-once message when athena_ii is the active profile.

Required fields would force a TODO marker that fails `validate()` —
the load would be blocked, breaking the existing athena_ii consumer
codepaths in core. An optional field keeps athena_ii loadable while
the dimensions are pending.

### Validation

When `Some(BuildEnvelope)`, all three fields must be positive and
finite. When `None`, no constraint. Validation lives next to the
existing `PrinterProfile::validate()` checks (defence-in-depth at
boundary AND run entry).

### TOML representation

```toml
# elegoo_mars5_ultra.toml — populated
[build_envelope_mm]
width_mm = 153.36
depth_mm = 77.76
max_z_mm = 165.0
```

```toml
# athena_ii.toml — field absent (Option::None)
# (no [build_envelope_mm] section)
```

Standard `serde` `Option<T>` round-trip handles both shapes.

### Populated profiles in v1

| Profile | width × depth × max_z | Source |
|---------|----------------------|--------|
| `elegoo_mars5_ultra` | 153.36 × 77.76 × 165 mm | Elegoo published specs |
| `generic_msla_4k` | 192 × 120 × 200 mm | Typical 8.9" 4K monoLCD class |
| `athena_ii` | (None) | Skipped per human direction |

## Consequences

- **Viz consumes the field.** `resinsim-viz` reads
  `ActivePrinterProfile.build_envelope_mm` to size the build plate
  and frame the camera (issue 10).
- **Sim does not consume the field yet.** Every other consumer of
  `PrinterProfile` continues to work unchanged; the optional field
  is dormant in v1 from sim's perspective.
- **Dormant-field risk.** If a future schema-tightening pass on
  `PrinterProfile` lands and breaks the field's serialisation, only
  the viz crate fails. Step 3 of the issue 10 plan adds explicit
  validation tests that exercise both `Some` and `None` paths so
  schema drift breaks loudly at unit-test time.
- **Future plumbing.** When the simulator gains plate-collision or
  near-edge detection, it can read this field instead of redefining
  it. Until that lands, the field is viz-only.

## Alternatives considered

- **Required field.** Rejected: would require populating every
  profile (including athena_ii) at issue-10 time, blocking the human's
  explicit "skip athena ii for now" direction.
- **Separate `BuildEnvelopeRepository`.** Rejected: ADR-0005 says
  hardware envelope data lives on `PrinterProfile`. A separate repo
  for one field is over-decomposed.
- **Read from CTB header only.** Rejected: CTB v3 has no max-Z
  field, and the viz must work without a CTB loaded.
