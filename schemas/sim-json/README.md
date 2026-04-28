# `sim.json` schema (v1)

Canonical interchange schema for resinsim's simulation output. Governed by
[ADR-0015](../../docs/adr/0015-sim-json-canonical-interchange.md).

## Files

| File | Role |
| --- | --- |
| `v1.ts` | Canonical zod 4 schema, used for TS-side type inference and downstream LLM-tooling consumption. Hand-edited. |
| `v1.schema.json` | JSON Schema (Draft 2020-12). Hand-aligned with `v1.ts`. **The Rust ↔ JSON Schema parity test (`crates/resinsim-core/tests/sim_json_schema_parity.rs`) is the load-bearing drift guard** — fresh Rust serde output must validate against this file. |
| `package.json` | Pins `zod 4.0.0` (exact) plus the regenerate-schema toolchain (advisory; not yet CI-integrated). |
| `scripts/regenerate-schema.ts` | `tsx` wrapper around `z.toJSONSchema(SimulationEnvelopeV1)`. **Currently advisory** — the script is for authors who want to verify their `v1.ts` edits produce a schema consistent with the committed `v1.schema.json`, but it is not invoked by CI. See "Drift posture" below. |

## Drift posture

The v1.schema.json file is hand-aligned with v1.ts. There are two
documented drift surfaces:

1. **Rust serde ↔ v1.schema.json**: Caught by
   `crates/resinsim-core/tests/sim_json_schema_parity.rs`. CI fails if a
   fresh `save_with_provenance(...)` output does not validate against the
   committed v1.schema.json. This is the load-bearing risk, fully
   automated.
2. **v1.ts ↔ v1.schema.json**: NOT automated currently. Authors editing
   `v1.ts` are responsible for either (a) running the regenerate script
   (`npm install && npm run regenerate-schema`) and committing the
   updated `v1.schema.json`, OR (b) hand-aligning `v1.schema.json` to
   match. Until a Node toolchain is part of CI, this drift is
   author-discipline-enforced; the parity test catches it transitively
   on the next Rust-side change.

Future work: integrate the regenerate script into CI so that a v1.ts
change that doesn't match the committed v1.schema.json fails CI directly.
Tracked as a follow-up issue (out of scope for issue 15).

## Bumping schema_version

Producer-side breaking changes (rename, remove, retype, integer-discriminant
reorder) require a new version. Procedure:

1. Copy `v1.ts` → `v2.ts`. Edit `v2.ts` to match the new shape.
2. Bump the literal in `SimulationEnvelopeV2` to `2`.
3. Bump `CURRENT_SCHEMA_VERSION` in `crates/resinsim-core/src/repositories/simulation_repo.rs` to `2`.
4. Regenerate `v2.schema.json` via `npm run regenerate-schema`.
5. Update the parity test in `crates/resinsim-core/tests/sim_json_schema_parity.rs`
   to validate against `v2.schema.json`.
6. Document the breaking change in `ADR-0015`'s Versioning Examples section.

`v1.ts` and `v1.schema.json` stay in the tree; consumers branching on
`schema_version` keep working with old envelopes.

## Additive (non-breaking) changes

Adding an optional field is additive — no version bump required. Edit `v1.ts`,
regenerate `v1.schema.json`, ship.

## Regenerating `v1.schema.json`

```sh
cd schemas/sim-json
npm install
npm run regenerate-schema
```

The regenerated file is committed alongside the `v1.ts` change.

## Pinning policy

Every dependency in `package.json` is an exact version (no caret, no tilde).
Drift in zod's JSON-Schema output (e.g. between zod 4.0.0 and 4.1.0) would
otherwise silently change `v1.schema.json`'s shape and could mask Rust
serde drift behind a moving baseline.

## Cross-language drift guard

The Rust integration test
`crates/resinsim-core/tests/sim_json_schema_parity.rs` produces a known
`SimulationEnvelope`, serializes via `save_to_path`, and validates the output
against `v1.schema.json` using the `jsonschema` crate. Any drift between
`v1.ts` and the actual Rust serde output fails CI immediately.
