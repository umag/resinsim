---
issue: 15-extract-resinsim-run
date: 2026-04-28
---

# Pattern: Optional `provenance` block for producer/consumer-split CLI pipelines

## Context

A canonical-interchange file (here `sim.json`) is produced by one CLI
subcommand and consumed by another. The aggregate payload (`PrintSimulation`)
is the load-bearing data, but the consumer also needs to render context
the user supplied to the producer (input path, profile names, support
config) — that context is run-time CLI state, not part of the aggregate.

Two design questions:
1. Where does the run-context metadata live? On the aggregate or in the
   envelope?
2. What does the consumer do when the metadata is absent (legacy
   producers, GUI side-effects)?

## Decision

- **Place run-context in an envelope-level `provenance` block, not on the
  aggregate.** Per ADR-0009 (repositories vs IO placement) the aggregate
  stays version-free for in-memory consumers; the envelope carries the
  IO-only metadata.
- **Make the block optional with `serde(default)`.** Producers that have
  the metadata populate it (CLI `resinsim sim`); producers that don't (GUI
  Save-Sim, older tooling) emit envelopes without it.
- **Consumers degrade gracefully.** Text mode renders `"(unknown)"`; JSON
  mode emits `null` for the missing fields. Hard-erroring would block
  legitimate GUI-Save-Sim → report-health flows.

## Shape

```rust
struct SimulationEnvelope {
    schema_version: u32,
    simulation: PrintSimulation,
    #[serde(default)]
    provenance: Option<Provenance>,
}

#[derive(Serialize, Deserialize)]
pub struct Provenance {
    pub input_path: String,
    pub resin_name: String,
    pub printer_name: String,
    pub n_supports: u32,
    pub tip_radius_mm: f32,
}
```

`ReportContext` on the consumer side mirrors the optional-ness:

```rust
pub struct ReportContext {
    pub stl_path: String,                // always populated (envelope path fallback)
    pub resin_name: Option<String>,      // None when provenance absent
    pub printer_name: Option<String>,
    pub n_supports: Option<u32>,
    pub tip_radius_mm: Option<f32>,
}
```

Text mode renders `Some(s)` as `s`, `None` as `"(unknown)"`. JSON mode
serialises `Option` directly (None → null).

## Trade-offs

- **Wire-shape variance.** The envelope can have or not have `provenance`,
  doubling the test surface (need round-trip tests for both). Issue 15's
  `report_health_json_emits_null_resin_for_envelope_without_provenance`
  and `report_health_text_uses_unknown_placeholder_for_envelope_without_provenance`
  cover this.
- **Validate the metadata.** Provenance is not the load-bearing payload but
  its values flow into user-facing surfaces. A `Provenance::validate()` after
  deserialise catches NaN / negative tip_radius / etc. before downstream
  consumers see the bogus value.

## Discipline

When adding new fields to the envelope:
- Aggregate fields (anything load-bearing) → `simulation: PrintSimulation`
- Run-context metadata → `provenance` (still bump schema_version on
  breaking shape changes, but adding optional provenance fields is
  additive per ADR-0015)

## See also

- ADR-0015 — sim.json canonical interchange.
- ADR-0009 — repositories vs IO placement.
- `docs/patterns/null-as-sentinel-for-non-finite-float-serde.md` — the
  Option-vs-null reasoning for individual fields.
