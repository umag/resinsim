# resinsim-inspect

The `resinsim` command-line binary. Hosts three top-level subcommands:

- **`resinsim sim`** — produce a canonical `sim.json` envelope from an
  STL or CTB input. The producer side of the ADR-0015 pipeline.
- **`resinsim report`** — render a print-health report from a sim.json
  envelope. The consumer side of the ADR-0015 pipeline.
- **`resinsim inspect`** — single-domain inspection commands (cure,
  force, thermal, zaxis, athena, layers, calibrate, field).

## Tier-2 voxel cure mode (ADR-0017 / t2f1)

Available only in builds with the `field-sim` Cargo feature
(`cargo build --features field-sim`). The `resinsim sim` subcommand
accepts an additional `--voxel-cure-mm <FLOAT>` flag whose presence
enables the Tier-2 voxel-resolved cure path with KB-160 photoinitiator
depletion. The flag value (mm) is reserved for future resolution
decoupling; v1 uses the input mask's voxel size.

```sh
cargo build --features resinsim-inspect/field-sim
target/debug/resinsim sim --file model.ctb \
    --resin generic_standard --printer elegoo_mars5_ultra \
    --voxel-cure-mm 0.2 --out model.sim.json
```

The produced sim.json carries an optional `fields_sidecar` pointer
(not inline field blocks — ADR-0019 moved the four voxel fields
cure/photoinitiator/strain/stress, plus thermal from ADR-0020, into a
paired `<stem>.fields.bin` binary sidecar to keep sim.json itself
small; see "sim.json envelope shape" below). Default builds (no
`--features field-sim`) reject `--voxel-cure-mm` with a clap
unknown-flag error; STL inputs in voxel-mode emit a `note:` and fall
back to Tier-1 because STL has no per-layer masks. See ADR-0017 and
KB-160 for design + physics references.

## `inspect field` — Tier-2 voxel field slice inspector (ADR-0019 / ADR-0023)

Read-side inspector over the five Tier-2 voxel fields (cure,
photoinitiator, strain, stress, thermal). Loads a persisted
`<stem>.sim.json` + paired `<stem>.fields.bin` sidecar and renders one
2D slice as an aligned text table + ASCII histogram, or as JSON — it
**never re-runs a solver**. Stays visible in `--help` and present in
every build for discoverability; the handler itself requires the
`field-sim` Cargo feature and exits 2 with an actionable rebuild
message in default builds (a deliberate divergence from
`--voxel-cure-mm`'s bare-unknown-flag behaviour above — see ADR-0023
Decision 3).

```sh
cargo build --features resinsim-inspect/field-sim
target/debug/resinsim sim --file model.ctb \
    --resin generic_standard --printer elegoo_mars5_ultra \
    --voxel-cure-mm 0.2 --out model.sim.json
target/debug/resinsim inspect field --in model.sim.json \
    --field cure --slice z=10mm --json
```

`--in` (not `--file`): `inspect field` is an **envelope consumer**,
like `report health --in` — it reads an already-produced `sim.json` +
sidecar pair, so it takes `--in <PATH>` mirroring `report health`.
Contrast the **raw-file inspectors** (`inspect layers`, `inspect
athena`, `inspect calibrate`), which read a sliced file or archive
directly and so take `--file <PATH>` instead. This is a deliberate,
load-bearing naming rule (ADR-0023) — not an inconsistency.

`--slice <AXIS>=<VALUE>[mm]` addresses one axis by world millimetres;
the other two axes become the rendered 2D plane. Cure / photoinitiator
/ strain / stress resolve their Z axis through the print's cumulative
per-layer CTB heights (the layer index); `thermal` resolves Z as a
spatial offset into the vat envelope instead — two different physical
meanings behind the one `--slice` spelling (ADR-0023 Decision 2).

See ADR-0023 for the full read-side contract: the two-Z-semantics
split, the feature-off UX decision, the `--cured-only` dual-scope
statistics policy, and the descriptor-driven decode-budget
auto-extension (ceiling 24 GB).

## ADR-0015 pipeline

`sim.json` is the canonical interchange between simulation producer and
downstream consumers (resinsim-viz `--load-sim`, `resinsim report
health --in`, future LLM tooling). Producer/consumer are decoupled — the
producer hands off a typed envelope; the consumer reads only the
envelope and never re-runs the simulation.

```sh
# Step 1: produce the envelope
resinsim sim --file model.ctb \
    --resin generic_standard --printer generic_msla_4k \
    --out model.sim.json

# Step 2: render the report
resinsim report health --in model.sim.json
resinsim report health --in model.sim.json --json   # JSON output

# Step 3 (optional): visualise in the GUI
resinsim-viz --load-ctb model.ctb --load-sim model.sim.json
```

### Breaking change to `report health`

Pre-ADR-0015, `report health` accepted `--stl/--file/--resin/--printer`
plus the simulation-config args (`--tip-radius`, `--n-supports`,
`--ambient`, `--initial-led-temp`, `--data-dir`). All of those have
moved to `resinsim sim`; `report health` now accepts only `--in <PATH>`
and `--json`. There are no current users to migrate; clap's default
unknown-flag rejection is the legacy-flags response.

## Producer surfaces (`--out` vs `--save-sim`)

Two surfaces produce a `sim.json` envelope:

- **`resinsim sim --out <PATH>`** — the canonical CLI producer. Always
  writes a `Provenance`-bearing envelope (input path, resin name, printer
  name, support config). Consumers of the envelope can reconstruct the
  full report header from these fields without re-supplying CLI args.
- **`resinsim-viz --save-sim <PATH>`** — GUI side-effect of running an
  interactive simulation. Writes the same envelope shape but **without
  Provenance** (the GUI run is interactive — there is no producer-side
  CLI invocation to record). Consumers like `report health --in` degrade
  gracefully to `(unknown)` placeholder strings (text mode) or `null`
  fields (JSON mode) when they encounter a Save-Sim envelope.

The flag-name asymmetry is intentional — `--out` for the CLI's primary
output, `--save-sim` for the GUI's optional side-effect. Both produce a
schema-version-2 envelope that any consumer can parse.

## sim.json envelope shape

```jsonc
{
  "schema_version": 2,
  "simulation": { /* PrintSimulation aggregate */ },
  "provenance": {
    "input_path": "model.ctb",
    "resin_name": "Generic Standard",
    "printer_name": "Generic MSLA 4K",
    "n_supports": 20,
    "tip_radius_mm": 0.2
  },
  "fields_sidecar": {
    "path": "model.fields.bin",
    "byte_size": 12345,
    "sha256": "...",
    "fields_present": ["cure", "photoinitiator"]
  }
}
```

`fields_sidecar` (v2+) is present only for Tier-2 voxel-mode runs
(`resinsim sim --voxel-cure-mm`); it points at the paired
`<stem>.fields.bin` binary sidecar carrying the voxel fields
losslessly (ADR-0019). Absent for Tier-1 scalar runs.

The canonical schema source is `schemas/sim-json/v2.ts` (zod 4); the
JSON Schema bridge is `schemas/sim-json/v2.schema.json`. Cross-language
parity is enforced by
`crates/resinsim-core/tests/sim_json_schema_parity.rs`. **v1 envelopes
are no longer supported** (ADR-0019 / t2f3.5 clean break) — loading one
produces a typed `"unknown schema_version 1"` error with a
regeneration hint pointing back at `resinsim sim`.

See `docs/adr/0015-sim-json-canonical-interchange.md` for the full
versioning rules and concrete add-vs-rename-vs-retype examples.

## Profile resolution

`resinsim sim` and the various `resinsim inspect` subcommands resolve
profiles via the ADR-0004 4-stage data-dir chain:

1. `--data-dir <PATH>` flag
2. `$RESINSIM_DATA_DIR` env
3. `$CWD/data`
4. `<binary-parent>/data`

The first stage that yields an existing directory wins. Unknown
profile names hard-error with the available-profiles list.

## See also

- ADR-0004 — CLI profile loading and the 4-stage data-dir chain
- ADR-0009 — Repositories vs IO placement (envelope wrapper at IO boundary)
- ADR-0010 — viz/core layering rule
- ADR-0011 — egui control panels (Save-Sim sidecar)
- ADR-0015 — sim.json canonical interchange (this issue)
- ADR-0017 — voxel cure field + photoinitiator depletion (Tier-2 t2f1;
  §5 is the feature-off precedent `inspect field` deliberately diverges
  from)
- ADR-0019 — voxel field on-disk persistence (the binary sidecar
  `inspect field` reads)
- ADR-0023 — `inspect field`'s read-side contract: the two-Z-semantics
  `--slice` split, the feature-off UX decision, `--cured-only`'s
  dual-scope statistics, and the decode-budget auto-extension
