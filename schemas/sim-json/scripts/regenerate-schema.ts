/**
 * Regenerate `v2.schema.json` from the canonical zod schema in `v2.ts`.
 *
 *   npm install && npm run regenerate-schema
 *
 * **Status: advisory** (issue 15 / ADR-0015). The script is provided for
 * authors who want to verify their `v2.ts` edits produce a schema
 * consistent with the committed `v2.schema.json`. It is NOT currently
 * invoked by CI — `v2.schema.json` is hand-aligned with `v2.ts` and the
 * Rust↔JSON Schema parity test (the load-bearing drift guard) lives in
 * `crates/resinsim-core/tests/sim_json_schema_parity.rs`.
 *
 * If you edit `v2.ts`, either (a) run this script and commit the updated
 * `v2.schema.json`, OR (b) hand-align `v2.schema.json` to match. Future
 * work integrates this script into CI; see ADR-0015 + the schemas/sim-json
 * README "Drift posture" section.
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { SimulationEnvelopeV2 } from "../v2.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));
const out = join(__dirname, "..", "v2.schema.json");

const jsonSchema = z.toJSONSchema(SimulationEnvelopeV2, { target: "draft-2020-12" });
writeFileSync(out, JSON.stringify(jsonSchema, null, 2) + "\n");
console.log(`wrote ${out}`);
