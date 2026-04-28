---
issue: 15-extract-resinsim-run (sim.json canonical interchange)
date: 2026-04-28
---

# ADR-0015: sim.json as canonical simulation interchange format

## Status

Accepted (issue 15 implementation, 2026-04-28).

## Context

Issue 15 ("Extract resinsim-run CLI from resinsim-viz") originally proposed
extracting the simulation runner into a new `resinsim-run` binary. During
planning the scope shifted: the runner already lives in `resinsim-core`
(see ADR-0010); the load-bearing question was not "where does the runner
live" but "what is the on-disk shape that ties producer and consumer
together?"

Three downstream consumers of simulation output are converging:

1. **`resinsim-viz --load-sim`** today reads a JSON file produced by the
   GUI's Save-Sim or by the standalone `synth_sim_from_ctb` example.
2. **`resinsim report health`** today builds the simulation in-process
   from CTB + profiles.
3. **LLM tooling** (planned) will consume simulation output for
   risk-assessment summarisation and needs a stable, schema-typed
   producer.

Without a canonical schema each consumer accumulates drift. The previous
shape was the bare `PrintSimulation` aggregate JSON — useful, but with no
version discriminant and no mechanism to reject future-shape envelopes.

## Decision

**`sim.json` is the canonical simulation interchange format for resinsim,
with a typed `SimulationEnvelope` wrapper, an optional `Provenance` block,
a zod 4 canonical schema, and a Rust↔TS parity test.**

### On-disk shape

```jsonc
{
  "schema_version": 1,
  "simulation": { /* PrintSimulation aggregate */ },
  "provenance": {
    "input_path": "model.ctb",
    "resin_name": "Generic Standard",
    "printer_name": "Generic MSLA 4K",
    "n_supports": 20,
    "tip_radius_mm": 0.2
  }
}
```

- `schema_version` is the discriminant. Loaders reject unknown values with
  a typed error rather than parsing as if it were the current version.
- `simulation` is the `PrintSimulation` aggregate's serde projection. The
  aggregate stays version-free for in-memory consumers (per ADR-0009; the
  schema lives in the IO layer).
- `provenance` is optional. Producers that have run-context metadata
  (the `resinsim sim` CLI subcommand) populate it; producers that don't
  (the GUI Save-Sim path) omit it. Consumers degrade gracefully to
  placeholder strings when absent.

### Producer / consumer split

- **Producer**: `resinsim sim --stl|--file ... --resin ... --printer ... --out PATH.sim.json`.
  Calls `save_with_provenance` from `resinsim-core::repositories`.
- **Consumers**:
  - `resinsim report health --in PATH.sim.json` — text/JSON report rendering.
  - `resinsim-viz --load-sim PATH.sim.json` — heatmap / plot rendering.
  - Future LLM tooling — schema-typed via `schemas/sim-json/v1.ts`.

Pipeline:

```sh
resinsim sim --file model.ctb --resin generic_standard \
    --printer generic_msla_4k --out model.sim.json
resinsim report health --in model.sim.json
```

### Atomic write

`save_to_path` and `save_with_provenance` stage at `<out>.tmp` then
`std::fs::rename` to `<out>`. POSIX rename on the same filesystem is
atomic; a partial write cannot corrupt an existing `<out>` from a
downstream consumer's perspective. Windows semantics for cross-volume
rename are eventually-consistent — not in scope (resinsim is dev-platform
Linux/macOS today).

### Default `--out` overwrite

`resinsim sim --out PATH.sim.json` silently overwrites an existing
PATH.sim.json. POSIX default; matches `cp`. No `--force` flag in v1; if
the user wants no-overwrite semantics they manage the file lifecycle
themselves.

### Canonical zod schema

`schemas/sim-json/v1.ts` is the canonical TypeScript-side source for
downstream LLM tooling and zod-aware consumers.
`schemas/sim-json/v1.schema.json` is the cross-language JSON Schema
bridge — hand-aligned with `v1.ts` for now (see "Drift posture" below).
`crates/resinsim-core/tests/sim_json_schema_parity.rs` validates fresh
serde output against `v1.schema.json`; drift between Rust serde and the
JSON Schema fails CI immediately. The parity test is the load-bearing
guard for the Rust producer.

`schemas/sim-json/package.json` pins `zod 4.0.0` exactly (no caret/tilde)
so a zod patch version bump cannot silently change `v1.schema.json`'s
shape if/when the regenerate script lands in CI.

### Drift posture (two surfaces)

1. **Rust serde ↔ `v1.schema.json`** is automated. The parity test
   produces an envelope, validates it against the committed
   `v1.schema.json`, and fails on any field-shape mismatch.
2. **`v1.ts` ↔ `v1.schema.json`** is currently author-enforced.
   `regenerate-schema.ts` is committed as an advisory tool — authors
   editing `v1.ts` either (a) run `npm install && npm run
   regenerate-schema` to update `v1.schema.json`, OR (b) hand-align
   `v1.schema.json` directly. Until a Node toolchain is part of CI, the
   parity test catches v1.ts→Rust drift transitively (because v1.ts
   isn't consumed by Rust; only `v1.schema.json` is).

Future work — promote the regenerate script to a CI step that fails on
`git diff --exit-code` after a fresh regenerate. Tracked as a follow-up
issue out of scope for issue 15.

### Legacy-flags policy on `report health`

The pre-ADR-0015 `report health --stl ... --resin ... --printer ...`
flag surface is removed. `--in <PATH>` is the only producer-input flag.
clap's default rejection on unknown flags is sufficient — there are no
current users to migrate; the rejection just needs to hard-fail invalid
invocations.

## Versioning rules

`schema_version` is the discriminant. Versioning policy:

- **Bump on breaking changes.** Any of: removing a field, renaming a
  field, changing a field's type, reordering integer-discriminant enum
  variants, adding a required field.
- **Don't bump on additive changes.** Adding an optional field is
  additive — `serde(default)` on the new field keeps old envelopes
  loading.
- **Old loaders must reject unknown versions cleanly.** A v1 loader sees
  `schema_version: 2` and returns an `"unknown schema_version 2 (expected 1)"`
  error rather than parsing partial fields as if they were v1.
- **One vN.ts per version.** Bumping creates `v2.ts` alongside `v1.ts`;
  consumers branch on the discriminant and pick the right loader.

### Concrete versioning examples

- ✅ Adding `worst_cure_depth_um` to `LayerResult` with `serde(default)`:
  **additive**. Old envelopes parse; new envelopes have the field.
- ❌ Removing `wait_after_release_sec` from `Recipe`: **breaking**. Old
  envelopes carry the field; v2 loaders ignore it. Bump to v2.
- ❌ Changing `bottom_layer_count` from `u32` to `Option<u32>`:
  **breaking**. The wire shape changes from `6` to `null`/`6`. Bump.
- ❌ Renaming `cure_depth_um` to `cure_depth_micrometres`: **breaking**.
  Wire field name is the contract. Bump.
- ❌ Reordering `Severity` enum so `Critical = 0` instead of `Critical = 2`
  (when serialised as integer discriminants): **breaking**. The current
  serde encoding is a string tag (`"Critical"`) so this is moot today,
  but if a future change moves to integer encoding the reorder rule
  applies.
- ✅ Adding a `Suction` variant to `FailureType` with `#[serde(other)]`
  fallback: **additive** for old loaders that have the fallback.
- ❌ Adding a `Suction` variant without a fallback: **breaking**. Old
  loaders fail-stop on the unknown tag.

## Consequences

### Good

- Single source of truth for the on-disk shape (`v1.ts`).
- LLM tooling consumes a typed schema that mirrors the Rust serde shape.
- Breaking changes are signposted: the `schema_version` bump triggers
  consumer review.
- Atomic writes prevent partial-write corruption for downstream parsers.
- The producer-consumer split simplifies `report health` — no profile
  loading, no support config, no ambient/initial-LED args.

### Bad

- TS toolchain dependency on a Rust workspace (the `schemas/` subdirectory
  needs `npm install` to regenerate `v1.schema.json`). Mitigated: the
  regenerated `v1.schema.json` is committed; consumers don't run npm.
- Drift risk between Rust serde and zod. Mitigated: parity test in
  `sim_json_schema_parity.rs` fails CI immediately if `v1.schema.json`
  rejects fresh-from-Rust output.
- Existing on-disk sim.json files (e.g. `synth_sim_from_ctb` example output)
  must be regenerated under the new envelope shape. Acceptable given the
  pre-1.0 posture.

## References

- ADR-0004 (CLI profile loading; the 4-stage data-dir resolution chain
  this ADR's `resolve_profiles` helper preserves).
- ADR-0009 (Repositories vs IO placement; `schema_version` lives in the
  envelope wrapper, not on `PrintSimulation`).
- ADR-0010 (resinsim-viz presentation layer; this ADR preserves the
  one-way layering rule by moving `build_simulation_*` into
  `resinsim-core::app`).
- ADR-0011 (egui control panels; the GUI Save-Sim sidecar still works,
  via the optional-provenance branch).
- ADR-0014 (bevy_egui retained for viewer redesign; the v2 viewer
  consumes sim.json via `--load-sim` and benefits from the canonical shape).
