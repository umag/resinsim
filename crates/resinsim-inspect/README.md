# resinsim-inspect

The `resinsim` command-line binary. Hosts three top-level subcommands:

- **`resinsim sim`** — produce a canonical `sim.json` envelope from an
  STL or CTB input. The producer side of the ADR-0015 pipeline.
- **`resinsim report`** — render a print-health report from a sim.json
  envelope. The consumer side of the ADR-0015 pipeline.
- **`resinsim inspect`** — single-domain inspection commands (cure,
  force, thermal, zaxis, athena, layers).

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
schema-version-1 envelope that any consumer can parse.

## sim.json envelope shape

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

The canonical schema source is `schemas/sim-json/v1.ts` (zod 4); the
JSON Schema bridge is `schemas/sim-json/v1.schema.json`. Cross-language
parity is enforced by
`crates/resinsim-core/tests/sim_json_schema_parity.rs`.

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
