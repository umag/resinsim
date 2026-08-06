/**
 * TS-side parity guard for `schemas/sim-json/v2.ts`.
 *
 * Complementary to
 * `crates/resinsim-core/tests/sim_json_schema_parity.rs`, which validates
 * FRESH Rust-serde-written envelopes against `v2.schema.json` (JSON
 * Schema). This test validates the COMMITTED on-disk `sim_golden`
 * fixtures against the zod SOURCE (`v2.ts`) — a different producer (a
 * committed corpus, not a fresh Rust write) and a different consumer
 * (`v2.ts`, not `v2.schema.json`). See the schemas/sim-json README and
 * ADR-0015's "Drift posture" section.
 *
 * Nine assertions only the TS side can make (neither JSON Schema nor the
 * Rust parity suite can express these):
 *   1. Tri-state `cure_kinetics_ea_is_default` — JSON Schema's
 *      `{"type":"boolean"}` cannot express "absent must not be read as
 *      false".
 *   2. `base_force_n` optionality and its `.default(0)` — the Rust
 *      producer always writes the field, so the Rust suite structurally
 *      cannot cover the absent-field path.
 *   3. Unknown-key tolerance — the TS-side mirror of `v2.schema.json`'s
 *      `additionalProperties: true`.
 *   4. A type-tamper negative on a field the Rust suite does not tamper
 *      (`retract_speed_mm_min`), exercising the nullable+optional branch.
 *   5. (2026-08, schemas-v2-missing-optional-fields) An injected uniform
 *      `layer_height_provenance` (`ctb_um` + `layer_count` + `recipe_um`)
 *      parses against `v2.ts` SOURCE — catches a `v2.ts`-vs-`v2.schema.json`
 *      divergence the Rust suite (which only exercises the compiled
 *      `v2.schema.json`) cannot.
 *   6. An injected variable `layer_height_provenance` with a
 *      `mismatch: {kind: "variable", ...}` parses against `v2.ts` SOURCE —
 *      same rationale as (5), other branch.
 *   7. A wrong-typed `layer_height_provenance.ctb_um` fails against `v2.ts`
 *      SOURCE.
 *   8. A wrong-typed `peel_shape_factor` fails against `v2.ts` SOURCE.
 *   9. An object carrying NEITHER `ctb_um` nor `ctb_layer_heights_um` fails
 *      — the union has no matching branch (empirically also caught by
 *      `v2.schema.json`'s `anyOf` + per-branch `required`, so this is not
 *      uniquely a zod-vs-JSON-Schema expressiveness gap; it is still
 *      uniquely a `v2.ts`-SOURCE assertion because the Rust parity suite
 *      only ever exercises the compiled schema).
 *
 * Fixture coupling: this test reaches from `schemas/` into
 * `crates/resinsim-inspect/tests/fixtures/sim_golden/` because those
 * goldens are the only committed real envelopes in the repo. A future
 * fixture move must update FIXTURE_DIR below. This test only READS the
 * goldens — it must never write them, and must never set
 * `RESINSIM_REGENERATE_SIM_GOLDEN` (see
 * `crates/resinsim-inspect/tests/sim_golden.rs`) — doing so would mutate
 * fixtures another crate's byte-identity test depends on.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import { SimulationEnvelopeV2 } from "../v2.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));
// tests/ -> schemas/sim-json/ -> schemas/ -> <repo root> -> crates/...
const FIXTURE_DIR = join(
  __dirname,
  "..",
  "..",
  "..",
  "crates",
  "resinsim-inspect",
  "tests",
  "fixtures",
  "sim_golden",
);

const GOLDEN_FILENAMES = [
  "baseline.sim.json",
  "single_layer.sim.json",
  "zero_layers.sim.json",
] as const;

function loadGolden(name: string): unknown {
  const raw = readFileSync(join(FIXTURE_DIR, name), "utf8");
  return JSON.parse(raw);
}

// Enumerated explicitly, not globbed — a glob that silently matches zero
// files would exit 0 and make this guard decorative.
const goldens = new Map<string, unknown>(
  GOLDEN_FILENAMES.map((name) => [name, loadGolden(name)]),
);

test("exactly three sim_golden fixtures are loaded", () => {
  assert.equal(goldens.size, 3);
});

test("all three goldens parse against SimulationEnvelopeV2", () => {
  for (const name of GOLDEN_FILENAMES) {
    const result = SimulationEnvelopeV2.safeParse(goldens.get(name));
    assert.equal(
      result.success,
      true,
      `${name} must parse against SimulationEnvelopeV2: ${
        result.success ? "" : JSON.stringify(result.error.issues)
      }`,
    );
  }
});

test("cure_kinetics_ea_is_default is tri-state: true / false / absent (not false)", () => {
  const baseline = SimulationEnvelopeV2.parse(goldens.get("baseline.sim.json"));
  const singleLayer = SimulationEnvelopeV2.parse(goldens.get("single_layer.sim.json"));
  const zeroLayers = SimulationEnvelopeV2.parse(goldens.get("zero_layers.sim.json"));

  assert.equal(baseline.cure_kinetics_ea_is_default, true);
  assert.equal(singleLayer.cure_kinetics_ea_is_default, false);
  // Load-bearing: absence must round-trip as `undefined`, NOT `false`.
  // v2.ts's doc comment on the field: "consumers MUST NOT read absent as
  // false".
  assert.equal(zeroLayers.cure_kinetics_ea_is_default, undefined);
  assert.equal("cure_kinetics_ea_is_default" in zeroLayers, false);
});

test("base_force_n is optional and defaults to 0 when absent", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: { layers: Array<Record<string, unknown>> };
  };
  assert.ok(
    raw.simulation.layers.length > 0,
    "fixture assumption: baseline.sim.json has at least one layer",
  );
  delete raw.simulation.layers[0].base_force_n;

  const parsed = SimulationEnvelopeV2.parse(raw);
  assert.equal(parsed.simulation.layers[0].base_force_n, 0);
});

test("printer fields not declared in v2.ts are tolerated (additionalProperties parity)", () => {
  const raw = goldens.get("baseline.sim.json") as {
    simulation: { printer: Record<string, unknown> };
  };
  // These seven fields are real fields resinsim's Rust PrinterProfile
  // serialises that v2.ts deliberately does not declare (see
  // sim_json_schema_parity.rs and v2.schema.json's `additionalProperties:
  // true`). v2.ts must accept them anyway — a strict schema would reject
  // every real-world envelope from a printer profile written after v2.ts
  // was last updated.
  const undeclaredPrinterFields = [
    "voxel_cure_resolution_mm",
    "crosstalk_sigma_xy_um",
    "crosstalk_sigma_z_um",
    "convective_wall_h_w_m2k",
    "vat_wall_thickness_mm",
    "vat_wall_k_w_mk",
    "vacuum_pressure_kpa",
  ];
  for (const key of undeclaredPrinterFields) {
    assert.ok(
      key in raw.simulation.printer,
      `fixture assumption: baseline.sim.json's printer block carries ${key}`,
    );
  }

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    true,
    "v2.ts must not reject envelopes carrying printer fields it doesn't declare",
  );
});

test("a wrong-typed retract_speed_mm_min fails validation", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: { recipe: Record<string, unknown> };
  };
  raw.simulation.recipe.retract_speed_mm_min = "fast";

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    false,
    "a string retract_speed_mm_min must fail (schema is nullable number, not string)",
  );
});

// ---- schemas-v2-missing-optional-fields (2026-08): layer_height_provenance
// + peel_shape_factor, against v2.ts SOURCE. Literals for the provenance
// shapes are copied from
// crates/resinsim-core/tests/uat_steps/cli_report_health_layer_height_provenance.rs
// (UAT-1 / UAT-2), not invented. None of the three committed goldens carry
// either field, so every case here clone-and-injects — same technique as
// the base_force_n test above.

test("an injected uniform layer_height_provenance parses against v2.ts source", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: Record<string, unknown>;
  };
  raw.simulation.layer_height_provenance = {
    ctb_um: 40.0,
    layer_count: 4492,
    recipe_um: 40.0,
  };

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    true,
    `uniform layer_height_provenance must parse against v2.ts source: ${
      result.success ? "" : JSON.stringify(result.error.issues)
    }`,
  );
});

test("an injected variable layer_height_provenance with a variable mismatch parses against v2.ts source", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: Record<string, unknown>;
  };
  raw.simulation.layer_height_provenance = {
    ctb_layer_heights_um: [30.0, 40.0, 50.0, 40.0, 30.0],
    recipe_um: 30.0,
    mismatch: { kind: "variable", recipe_layers_for_same_z: 6 },
  };

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    true,
    `variable layer_height_provenance with a variable mismatch must parse against v2.ts source: ${
      result.success ? "" : JSON.stringify(result.error.issues)
    }`,
  );
});

test("a wrong-typed layer_height_provenance.ctb_um fails validation", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: Record<string, unknown>;
  };
  raw.simulation.layer_height_provenance = {
    ctb_um: "wide",
    layer_count: 4492,
    recipe_um: 40.0,
  };

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    false,
    "a string ctb_um must fail (schema declares ctb_um as a number)",
  );
});

test("a wrong-typed peel_shape_factor fails validation", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: { layers: Array<Record<string, unknown>> };
  };
  raw.simulation.layers[0].peel_shape_factor = "wide";

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    false,
    "a string peel_shape_factor must fail (schema declares peel_shape_factor as a number)",
  );
});

test("a layer_height_provenance carrying neither ctb_um nor ctb_layer_heights_um fails validation", () => {
  const raw = structuredClone(goldens.get("baseline.sim.json")) as {
    simulation: Record<string, unknown>;
  };
  raw.simulation.layer_height_provenance = {
    recipe_um: 40.0,
  };

  const result = SimulationEnvelopeV2.safeParse(raw);
  assert.equal(
    result.success,
    false,
    "an object with neither ctb_um nor ctb_layer_heights_um must fail (no union branch matches)",
  );
});
