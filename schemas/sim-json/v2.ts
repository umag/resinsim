/**
 * Canonical zod 4 schema for resinsim's `sim.json` interchange format
 * (schema_version = 2). See:
 * - docs/adr/0015-sim-json-canonical-interchange.md
 * - docs/adr/0019-voxel-field-on-disk-persistence.md (this version)
 *
 * v2 changes vs v1 (CLEAN BREAK — v1 envelopes are no longer supported):
 *   - schema_version literal bumped to 2.
 *   - Added optional `fields_sidecar` pointer at the envelope top-level
 *     that points at a paired binary sidecar `<stem>.fields.bin` carrying
 *     all four voxel fields (cure / photoinitiator / strain / stress) in
 *     the RSFIELD binary format. Tier-1 scalar simulations omit this
 *     field; Tier-2 voxel-cure runs (`--voxel-cure-mm` flag) emit it.
 *   - PrintSimulation no longer carries inline `cure_field` /
 *     `photoinitiator_field` JSON arrays. All voxel fields persist via
 *     the sidecar, not the envelope.
 *   - The existing v1.{ts,schema.json} files are preserved under
 *     `schemas/sim-json/archive/` for historical reference only.
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
export const NumericRangeV2 = z.object({
  min: z.number(),
  max: z.number(),
});

/** Build-envelope value-object on PrinterProfile. */
export const BuildEnvelopeMmV2 = z.object({
  width_mm: z.number(),
  depth_mm: z.number(),
  max_z_mm: z.number(),
});

/** Recipe value-object — the resin's concrete operating point. */
export const RecipeV2 = z.object({
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
export const PrinterProfileV2 = z.object({
  name: z.string(),
  led_power_mw_cm2: z.number(),
  pixel_pitch_um: z.number(),
  layer_height_range_um: NumericRangeV2,
  exposure_range_sec: NumericRangeV2,
  lift_speed_range_mm_min: NumericRangeV2,
  bottom_layer_count_max: z.number().int(),
  z_stiffness_n_per_mm: z.number(),
  delta_t_steady_c: z.number(),
  thermal_tau_sec: z.number(),
  lcd_uniformity_variation: z.number(),
  voxel_size_mm: z.number(),
  release_mechanism: z.enum(["linear", "tilt"]),
  led_delta_t_steady_c: z.number(),
  led_tau_sec: z.number(),
  led_to_vat_coupling: z.number(),
  build_envelope_mm: BuildEnvelopeMmV2,
});

/** Single completed layer's physical state. */
export const LayerResultV2 = z.object({
  index: z.number().int(),
  cure_depth_um: z.number(),
  peel_force_n: z.number(),
  suction_force_n: z.number(),
  total_force_n: z.number(),
  support_capacity_n: z.number(),
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
export const SeverityV2 = z.enum(["Info", "Warning", "Critical"]);

/** Failure-event type discriminant. Serialised as a string tag. */
export const FailureTypeV2 = z.enum([
  "SupportOverload",
  "ZDeflection",
  "VatTemperature",
  "InsufficientCureDepth",
  "Suction",
  "ThermalDegradation",
]);

/** A single failure event tagged onto a layer. */
export const FailureEventV2 = z.object({
  layer: z.number().int(),
  failure_type: FailureTypeV2,
  severity: SeverityV2,
  message: z.string(),
});

/** PrintSimulation aggregate — the canonical simulation payload. */
export const PrintSimulationV2 = z.object({
  recipe: RecipeV2,
  printer: PrinterProfileV2,
  layers: z.array(LayerResultV2),
  failures: z.array(FailureEventV2),
});

/**
 * Run-context metadata. Producers (resinsim sim) populate this so consumers
 * (resinsim report health --in, downstream LLM tooling) can render the
 * report header without needing the original CLI args.
 */
export const ProvenanceV2 = z.object({
  input_path: z.string(),
  resin_name: z.string(),
  printer_name: z.string(),
  n_supports: z.number().int(),
  tip_radius_mm: z.number(),
});

/**
 * Sidecar pointer (ADR-0019, t2f3.5). Carried on v2 envelopes when the
 * simulation has voxel-field data. `path` is relative to the
 * sim.json's parent directory; the Rust loader enforces path-traversal
 * + symlink-escape + is-regular-file rejection. `sha256` is hex-
 * encoded SHA-256 over the sidecar bytes (integrity check, not
 * cryptographic security). `fields_present` lists which of the four
 * voxel fields the sidecar carries; consumers can branch without
 * fully decoding the binary.
 */
export const SidecarPointerV2 = z.object({
  path: z.string(),
  byte_size: z.number().int(),
  sha256: z.string(),
  fields_present: z.array(z.string()),
});

/**
 * Top-level `sim.json` envelope. `schema_version` is a literal `2`
 * discriminator. `provenance` is optional (GUI Save-Sim omits it; CLI
 * `resinsim sim` always writes it). `fields_sidecar` is optional
 * (Tier-1 scalar simulations omit it; Tier-2 voxel-cure runs emit it).
 */
export const SimulationEnvelopeV2 = z.object({
  schema_version: z.literal(2),
  simulation: PrintSimulationV2,
  provenance: ProvenanceV2.optional(),
  fields_sidecar: SidecarPointerV2.optional(),
});

export type SimulationEnvelopeV2Type = z.infer<typeof SimulationEnvelopeV2>;
export type PrintSimulationV2Type = z.infer<typeof PrintSimulationV2>;
export type ProvenanceV2Type = z.infer<typeof ProvenanceV2>;
export type SidecarPointerV2Type = z.infer<typeof SidecarPointerV2>;
