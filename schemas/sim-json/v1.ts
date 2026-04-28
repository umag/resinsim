/**
 * Canonical zod 4 schema for resinsim's `sim.json` interchange format
 * (schema_version = 1). See docs/adr/0015-sim-json-canonical-interchange.md.
 *
 * This file is the canonical TypeScript-side source for downstream LLM
 * tooling and other zod-aware consumers. Bumping schema_version creates a
 * sibling vN.ts; do not mutate v1.ts in place once consumers are committed
 * to v1.
 *
 * `v1.schema.json` (sibling, hand-aligned for now) is the cross-language
 * JSON Schema bridge that the Rust side validates against. The
 * `crates/resinsim-core/tests/sim_json_schema_parity.rs` parity test fails
 * CI if Rust serde output drifts from `v1.schema.json` — that is the
 * load-bearing drift guard for the Rust producer. v1.ts ↔ v1.schema.json
 * alignment is currently author-enforced (see schemas/sim-json/README.md).
 *
 * Versioning rules (per ADR-0015):
 *   - Adding an optional field is additive — do NOT bump schema_version.
 *   - Removing or renaming a field is breaking — bump.
 *   - Changing a field's type is breaking — bump.
 *   - Reordering enum integer discriminants is breaking — bump.
 *   - Adding an enum variant is breaking unless guarded by `#[serde(other)]`
 *     or `#[serde(default)]`.
 */
import { z } from "zod";

/** Inclusive numeric range value-object used by PrinterProfile envelope fields. */
export const NumericRangeV1 = z.object({
  min: z.number(),
  max: z.number(),
});

/** Build-envelope value-object on PrinterProfile. */
export const BuildEnvelopeMmV1 = z.object({
  width_mm: z.number(),
  depth_mm: z.number(),
  max_z_mm: z.number(),
});

/** Recipe value-object — the resin's concrete operating point. */
export const RecipeV1 = z.object({
  layer_height_um: z.number(),
  bottom_layer_count: z.number().int(),
  transition_layers: z.number().int(),
  normal_exposure_sec: z.number(),
  bottom_exposure_sec: z.number(),
  wait_before_cure_sec: z.number(),
  wait_before_release_sec: z.number(),
  wait_after_release_sec: z.number(),
  lift_speed_mm_min: z.number(),
  lift_cycle_sec: z.number(),
  lift_distance_mm: z.number(),
  retract_speed_mm_min: z.number().nullable().optional(),
});

/** PrinterProfile aggregate (hardware envelope only — recipe lives on Recipe). */
export const PrinterProfileV1 = z.object({
  name: z.string(),
  led_power_mw_cm2: z.number(),
  pixel_pitch_um: z.number(),
  layer_height_range_um: NumericRangeV1,
  exposure_range_sec: NumericRangeV1,
  lift_speed_range_mm_min: NumericRangeV1,
  bottom_layer_count_max: z.number().int(),
  z_stiffness_n_per_mm: z.number(),
  delta_t_steady_c: z.number(),
  thermal_tau_sec: z.number(),
  lcd_uniformity_variation: z.number(),
  voxel_size_mm: z.number(),
  // Rust-side ReleaseMechanism is a serde-tagged enum. Tighten the schema
  // to the known-tag set so a future Rust→wire enum drift fails the
  // parity test instead of silently passing through `z.string()`.
  release_mechanism: z.enum(["linear", "tilt"]),
  led_delta_t_steady_c: z.number(),
  led_tau_sec: z.number(),
  led_to_vat_coupling: z.number(),
  build_envelope_mm: BuildEnvelopeMmV1,
});

/** Single completed layer's physical state. */
export const LayerResultV1 = z.object({
  index: z.number().int(),
  cure_depth_um: z.number(),
  peel_force_n: z.number(),
  suction_force_n: z.number(),
  total_force_n: z.number(),
  support_capacity_n: z.number(),
  // `null` represents `f32::INFINITY` (zero-force layer). JSON has no
  // Infinity literal so the Rust serde adapter (`f32_with_infinity`)
  // round-trips non-finite values via null. Consumers that care about
  // the distinction should branch on `safety_factor === null`.
  safety_factor: z.number().nullable(),
  cross_section_area_mm2: z.number(),
  area_delta_mm2: z.number(),
  vat_temperature_c: z.number(),
  viscosity_mpa_s: z.number(),
  z_deflection_um: z.number(),
  effective_layer_height_um: z.number(),
  worst_cure_depth_um: z.number(),
});

/** Failure-event severity discriminant. Serialised as a string tag. */
export const SeverityV1 = z.enum(["Info", "Warning", "Critical"]);

/** Failure-event type discriminant. Serialised as a string tag. */
export const FailureTypeV1 = z.enum([
  "SupportOverload",
  "ZDeflection",
  "VatTemperature",
  "InsufficientCureDepth",
  "Suction",
  "ThermalDegradation",
]);

/** A single failure event tagged onto a layer. */
export const FailureEventV1 = z.object({
  layer: z.number().int(),
  failure_type: FailureTypeV1,
  severity: SeverityV1,
  message: z.string(),
});

/** PrintSimulation aggregate — the canonical simulation payload. */
export const PrintSimulationV1 = z.object({
  recipe: RecipeV1,
  printer: PrinterProfileV1,
  layers: z.array(LayerResultV1),
  failures: z.array(FailureEventV1),
});

/**
 * Run-context metadata. Producers (resinsim sim) populate this so consumers
 * (resinsim report health --in, downstream LLM tooling) can render the
 * report header without needing the original CLI args.
 */
export const ProvenanceV1 = z.object({
  input_path: z.string(),
  resin_name: z.string(),
  printer_name: z.string(),
  n_supports: z.number().int(),
  tip_radius_mm: z.number(),
});

/**
 * Top-level `sim.json` envelope. `schema_version` is a literal `1`
 * discriminator: future schemas live as `v2.ts`/`v3.ts` etc. and consumers
 * branch on this field. `provenance` is optional — GUI Save-Sim writes
 * envelopes without provenance; CLI `resinsim sim` always writes it.
 */
export const SimulationEnvelopeV1 = z.object({
  schema_version: z.literal(1),
  simulation: PrintSimulationV1,
  provenance: ProvenanceV1.optional(),
});

export type SimulationEnvelopeV1Type = z.infer<typeof SimulationEnvelopeV1>;
export type PrintSimulationV1Type = z.infer<typeof PrintSimulationV1>;
export type ProvenanceV1Type = z.infer<typeof ProvenanceV1>;
