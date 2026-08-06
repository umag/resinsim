/**
 * Regenerate `v2.schema.json` from the canonical zod schema in `v2.ts`.
 *
 *   npm install && npm run regenerate-schema
 *
 * **Status: generated-and-committed, not CI-enforced** (issue 15 /
 * ADR-0015 / schemas-sim-json-tooling-stale-at-v1). `v2.schema.json` is
 * this script's output — it is not hand-aligned. The `io`/`reused`/
 * `override` options below are load-bearing (they're what makes the
 * output match the published `additionalProperties: true` contract and
 * keep the `$defs` under human names; see the schemas/sim-json README
 * "Drift posture" section for why). This script is NOT currently invoked
 * by CI — there is no `.github/` in this repository — so the
 * Rust↔JSON Schema parity test
 * (`crates/resinsim-core/tests/sim_json_schema_parity.rs`) remains the
 * load-bearing, fully-automated drift guard for the Rust producer side.
 *
 * If you edit `v2.ts`, run this script and commit the updated
 * `v2.schema.json`. If a future change can't satisfy the invariants this
 * script's options exist to preserve, hand-align `v2.schema.json` instead
 * and document the residual in the README — do not hand-edit the
 * generated file otherwise.
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { SimulationEnvelopeV2 } from "../v2.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));
const out = join(__dirname, "..", "v2.schema.json");

// The published contract for the seven fields resinsim's Rust producer
// serialises but v2.ts deliberately does not declare — all seven on
// PrinterProfile (voxel_cure_resolution_mm, crosstalk_sigma_xy_um,
// crosstalk_sigma_z_um, convective_wall_h_w_m2k, vat_wall_thickness_mm,
// vat_wall_k_w_mk, vacuum_pressure_kpa — see crates/resinsim-core/tests/
// sim_json_schema_parity.rs and the sim_golden fixtures). 2026-08
// (schemas-v2-missing-optional-fields) declared the six previously-
// undeclared LayerResult fields plus PrintSimulation.layer_height_provenance;
// this comment's prior count ("eleven — six on LayerResult, seven on
// PrinterProfile") was already arithmetically wrong (6+7=13, not 11) even
// before that fix, so it is restated here rather than silently carried
// forward. zod's default `io: "output"` emits `additionalProperties: false`,
// which would reject every real Rust-produced envelope; `io: "input"`
// merely omits the keyword. The `override` below is what actually restores
// the literal `true` this contract requires, independent of which zod 4.x
// minor is installed.
const jsonSchema = z.toJSONSchema(SimulationEnvelopeV2, {
  target: "draft-2020-12",
  // "input" (not the default "output") does two things: (1) it stops
  // `.default(0)` on LayerResult.base_force_n from being promoted into
  // that object's `required` list, which would reject every
  // pre-ADR-0022-Stage-1 v2 envelope; (2) it makes zod omit
  // `additionalProperties` entirely instead of stamping `false`, which is
  // the precondition the `override` below relies on.
  io: "input",
  // "ref" (not the default "inline") extracts every schema tagged with
  // `.meta({ id })` in v2.ts into `$defs`, keyed by that human id, instead
  // of inlining it at every use site.
  reused: "ref",
  override: ({ jsonSchema: node }) => {
    if (node.type === "object") {
      node.additionalProperties = true;
    }
    // zod Object.assigns the whole meta record (including `id`) onto every
    // tagged node's JSON Schema output. The human `$defs` key already
    // carries that name; a stray `id` property is not a draft-2020-12
    // keyword we want to publish, so strip it.
    delete node.id;
  },
});

// zod only emits `$id` when generating from a registry via the `external`
// option (see toJSONSchema's registry overload); a single-schema call like
// this one never sets it. Stamp it to match the published contract.
jsonSchema.$id = "https://resinsim.local/schemas/sim-json/v2.schema.json";

// Re-emit through an explicit top-level key order so the diff stays
// limited to $defs member ordering (traversal order, not alphabetical)
// rather than scattering unrelated top-level keys.
const TOP_LEVEL_ORDER = ["$schema", "$id", "title", "description"] as const;
const ordered: Record<string, unknown> = {};
for (const key of TOP_LEVEL_ORDER) {
  if (key in jsonSchema) ordered[key] = jsonSchema[key];
}
for (const [key, value] of Object.entries(jsonSchema)) {
  if (!(TOP_LEVEL_ORDER as readonly string[]).includes(key) && key !== "$defs") {
    ordered[key] = value;
  }
}
if ("$defs" in jsonSchema) ordered.$defs = jsonSchema.$defs;

writeFileSync(out, JSON.stringify(ordered, null, 2) + "\n");
console.log(`wrote ${out}`);
