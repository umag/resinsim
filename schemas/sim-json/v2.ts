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
 *   - 2026-08 (KB-153, sim-json-envelope-ea-default-flag): added optional
 *     `cure_kinetics_ea_is_default` at the envelope top level. Additive —
 *     schema_version stays 2. See the field's own doc comment for the
 *     three-valued (true / false / absent) wire contract.
 *   - 2026-08 (schemas-v2-missing-optional-fields): added optional
 *     `peel_shape_factor` plus five Tier-2 voxel-cure Option fields
 *     (`strain_magnitude_max`, `stress_von_mises_max_mpa`,
 *     `strain_gradient_max_frac`, `voxel_yield_fraction`,
 *     `crack_front_fraction`) to `LayerResultV2`, and `layer_height_provenance`
 *     — a two-branch `LayerHeightProvenanceV2` union (uniform `ctb_um` vs
 *     variable `ctb_layer_heights_um`, mirroring the Rust hand-written
 *     bimodal Serialize) with a `MismatchDetailV2` discriminated-union
 *     `mismatch` field — to `PrintSimulationV2`. All additive — schema_version
 *     stays 2.
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
export const NumericRangeV2 = z
  .object({
    min: z.number(),
    max: z.number(),
  })
  .meta({ id: "NumericRangeV2" });

/** Build-envelope value-object on PrinterProfile. */
export const BuildEnvelopeMmV2 = z
  .object({
    width_mm: z.number(),
    depth_mm: z.number(),
    max_z_mm: z.number(),
  })
  .meta({ id: "BuildEnvelopeMmV2" });

/** Recipe value-object — the resin's concrete operating point. */
export const RecipeV2 = z
  .object({
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
  })
  .meta({ id: "RecipeV2" });

/** PrinterProfile aggregate (hardware envelope only — recipe lives on Recipe). */
export const PrinterProfileV2 = z
  .object({
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
  })
  .meta({ id: "PrinterProfileV2" });

/** Single completed layer's physical state. */
export const LayerResultV2 = z
  .object({
    index: z.number().int(),
    cure_depth_um: z.number(),
    peel_force_n: z.number(),
    suction_force_n: z.number(),
    // ADR-0022 Stage 1 (KB-116). Optional with a 0 default so pre-Stage-1 v2
    // sim.json files (no base_force_n) still parse; mirrors the Rust
    // `#[serde(default)]` on LayerResult.base_force_n.
    base_force_n: z.number().default(0),
    // ADR-0022 Stage 3 (KB-185). Applied A/L peel shape factor, dimensionless
    // in (0, 1]. Absent when the resin's `peel_shape_factor_strength` is
    // unset (no correction — the factor is identically 1.0); absent is NOT
    // 1.0-by-default for consumers. Mirrors the Rust `#[serde(default,
    // skip_serializing_if = "Option::is_none")]` on LayerResult.peel_shape_factor.
    peel_shape_factor: z.number().optional(),
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
    /** Per-layer max Frobenius-norm strain (ADR-0018 / t2f3); Tier-2 voxel-cure only, absent on Tier-1 runs. */
    strain_magnitude_max: z.number().optional(),
    /** Per-layer max von Mises stress in MPa (ADR-0018 / t2f3); Tier-2 voxel-cure only. */
    stress_von_mises_max_mpa: z.number().optional(),
    /** Per-layer max strain gradient |∇ε| between adjacent voxels (ADR-0018 / t2f3); Tier-2 voxel-cure only. */
    strain_gradient_max_frac: z.number().optional(),
    /** Per-layer voxel yield fraction in [0, 1] (ADR-0018 / t2f3); Tier-2 voxel-cure only. */
    voxel_yield_fraction: z.number().optional(),
    /** Kendall interlayer crack-front fraction (peel-crack-propagation-tier1, KB-188/KB-116); `Some` only when > 0. */
    crack_front_fraction: z.number().optional(),
  })
  .meta({ id: "LayerResultV2" });

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
export const FailureEventV2 = z
  .object({
    layer: z.number().int(),
    failure_type: FailureTypeV2,
    severity: SeverityV2,
    message: z.string(),
  })
  .meta({ id: "FailureEventV2" });

/**
 * Structured detail describing why the CTB's layer-height and the resin
 * recipe's authored `layer_height_um` disagree. Discriminated on `kind`
 * (Rust: `MismatchKind`, `#[serde(tag = "kind", rename_all = "snake_case")]`
 * flattened via `#[serde(flatten)]` onto `MismatchDetail`).
 * `recipe_layers_for_same_z` is the layer count the recipe's authored value
 * would imply for the print's total Z-extent — present in both branches.
 */
export const MismatchDetailV2 = z
  .discriminatedUnion("kind", [
    z.object({
      kind: z.literal("uniform"),
      ctb_um: z.number(),
      recipe_layers_for_same_z: z.number().int(),
    }),
    z.object({
      kind: z.literal("variable"),
      recipe_layers_for_same_z: z.number().int(),
    }),
  ])
  .meta({ id: "MismatchDetailV2" });

/**
 * Reconciliation between the CTB file-axis layer-height authority and the
 * resin recipe's authored `layer_height_um` (ADR-0005 "Policy: CTB as
 * file-axis authority"). Rust:
 * `values::layer_height_provenance::LayerHeightProvenance`
 * (crates/resinsim-core/src/values/layer_height_provenance.rs), a
 * hand-written bimodal Serialize/Deserialize for schema efficiency — uniform
 * CTBs serialise a flat `ctb_um` scalar + `layer_count`; variable /
 * adaptive-sliced CTBs serialise the full `ctb_layer_heights_um` Vec instead.
 * `recipe_um` and the optional `mismatch` are common to both branches;
 * `mismatch` is absent on agreement. `layer_count` is optional in the
 * uniform branch — the legacy `{ctb_um, recipe_um}` shape (no
 * `layer_count`) is accepted by the Rust reader's fall-through
 * (reconstructed as a single-layer series,
 * `LayerHeightProvenanceWire`/layer_height_provenance.rs:406-410).
 * `mismatch` is NOT branch-locked to its matching `kind` — the Rust
 * `Deserialize` enforces no such cross-constraint even though `reconcile`
 * only ever pairs uniform-with-uniform and variable-with-variable.
 */
export const LayerHeightProvenanceV2 = z
  .union([
    z.object({
      ctb_um: z.number(),
      layer_count: z.number().int().optional(),
      recipe_um: z.number(),
      mismatch: MismatchDetailV2.optional(),
    }),
    z.object({
      ctb_layer_heights_um: z.array(z.number()),
      recipe_um: z.number(),
      mismatch: MismatchDetailV2.optional(),
    }),
  ])
  .meta({ id: "LayerHeightProvenanceV2" });

/** PrintSimulation aggregate — the canonical simulation payload. */
export const PrintSimulationV2 = z
  .object({
    recipe: RecipeV2,
    printer: PrinterProfileV2,
    layers: z.array(LayerResultV2),
    failures: z.array(FailureEventV2),
    /**
     * Optional CTB-vs-recipe layer-height reconciliation (ADR-0005). Present
     * on runs that entered via `run_from_layer_inputs*` (CTB / sliced-file
     * paths); absent on STL / area-only paths where no CTB-derived value
     * exists. Mirrors Rust's `#[serde(default, skip_serializing_if =
     * "Option::is_none")]` on `PrintSimulation.layer_height_provenance`. See
     * `LayerHeightProvenanceV2`'s doc comment for the two-branch wire shape.
     */
    layer_height_provenance: LayerHeightProvenanceV2.optional(),
  })
  .meta({ id: "PrintSimulationV2" });

/**
 * Run-context metadata. Producers (resinsim sim) populate this so consumers
 * (resinsim report health --in, downstream LLM tooling) can render the
 * report header without needing the original CLI args.
 */
export const ProvenanceV2 = z
  .object({
    input_path: z.string(),
    resin_name: z.string(),
    printer_name: z.string(),
    n_supports: z.number().int(),
    tip_radius_mm: z.number(),
    /** ADR-0025 / t2f5: compute device that ran the thermal solver.
     * "cpu" for the default Rayon path; the wgpu adapter name for
     * the GPU path. Additive — schema_version stays 2. */
    compute_device: z.string().optional(),
  })
  .meta({ id: "ProvenanceV2" });

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
export const SidecarPointerV2 = z
  .object({
    path: z.string(),
    byte_size: z.number().int(),
    sha256: z.string(),
    fields_present: z.array(z.string()),
  })
  .meta({ id: "SidecarPointerV2" });

/**
 * Top-level `sim.json` envelope. `schema_version` is a literal `2`
 * discriminator. `provenance` is optional (GUI Save-Sim omits it; CLI
 * `resinsim sim` always writes it). `fields_sidecar` is optional
 * (Tier-1 scalar simulations omit it; Tier-2 voxel-cure runs emit it).
 */
export const SimulationEnvelopeV2 = z
  .object({
    schema_version: z.literal(2),
    simulation: PrintSimulationV2,
    provenance: ProvenanceV2.optional(),
    fields_sidecar: SidecarPointerV2.optional(),
    /**
     * KB-153. true — the producing run's resin TOML omitted
     * cure_kinetics_ea_kj_mol, so every cure depth was computed from the
     * 30 kJ/mol literature-midpoint ESTIMATE. false — measured value.
     * ABSENT — the producer did not record it; consumers MUST NOT read
     * absent as false. Additive per ADR-0015: schema_version stays 2.
     */
    cure_kinetics_ea_is_default: z.boolean().optional(),
  })
  .meta({
    title: "SimulationEnvelopeV2",
    description:
      "Canonical sim.json envelope (schema_version=2). Source: schemas/sim-json/v2.ts (zod 4). v1 envelopes are no longer supported (clean break per ADR-0019 / t2f3.5); historical reference under schemas/sim-json/archive/. Adds optional fields_sidecar pointer to paired binary sidecar carrying voxel fields. 2026-08 (KB-153): adds optional top-level cure_kinetics_ea_is_default (additive, schema_version stays 2). 2026-08 (schemas-v2-missing-optional-fields): declares LayerResultV2.peel_shape_factor plus five Tier-2 Option fields, and PrintSimulationV2.layer_height_provenance (a two-branch LayerHeightProvenanceV2 union with a MismatchDetailV2 discriminated-union mismatch field) — additive, schema_version stays 2.",
  });

export type SimulationEnvelopeV2Type = z.infer<typeof SimulationEnvelopeV2>;
export type PrintSimulationV2Type = z.infer<typeof PrintSimulationV2>;
export type ProvenanceV2Type = z.infer<typeof ProvenanceV2>;
export type SidecarPointerV2Type = z.infer<typeof SidecarPointerV2>;
