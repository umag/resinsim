use std::collections::HashMap;

use crate::entities::{PrinterProfile, ResinProfile};
use crate::io::{geometry, sliced::LayerInput, stl};
use crate::services::build_plate::PlateAdhesionProfile;
use crate::services::failure_predictor::{
    FailurePredictor, LayerOverrides, SupportConfig, ThermalContext,
};
use crate::services::pairing_validator;
use crate::services::suction_detector::SuctionDetector;
#[cfg(feature = "field-sim")]
use crate::services::uniformity_calculator::UniformityProfile;
#[cfg(feature = "field-sim")]
use crate::services::{
    LightCrosstalkCalculator, ShrinkageCalculator, StressAccumulator, UniformityCalculator,
    VoxelCureCalculator,
};
use crate::simulation::PrintSimulation;
use crate::values::{
    AmbientTemperature, CrossSectionArea, InitialLedTemperature, LayerHeightProvenance,
    LayerHeightSeq, LayerMask, LayerPhase,
};
#[cfg(feature = "field-sim")]
use crate::values::{
    CureField, Energy, PenetrationDepth, PhotoinitiatorField, StrainField, StressField,
};

/// Application service: orchestrates a full simulation run.
/// Loads geometry, slices it, runs FailurePredictor per layer,
/// assembles the PrintSimulation aggregate.
pub struct SimulationRunner;

/// Internal per-run voxel state — built once by `run_inner_full` and
/// mutated through `apply_voxel_cure_for_layer` per layer. Captures the
/// invariants of t2f1's per-pixel exposure pass:
///
/// - `cure` and `pi` are dimension-locked at construction (set_voxel_fields
///   enforces this on installation; we maintain it inside the runner).
/// - `dp_um` / `ec_ref_mj_cm2` / `ref_temp_c` / `ea_cure_kj_mol` / `k_d` are
///   snapshotted from the ResinProfile at run start; they don't change
///   across layers within a single run.
/// - `led_power_mw_cm2` is the printer's nominal LED intensity at the LCD
///   plane; per-pixel intensity is `led_power_mw_cm2 × uniformity_factor(x,y)`
///   per KB-120 / `UniformityCalculator::intensity_factor`. Lateral light
///   crosstalk (XY Gaussian pixel bleed) and axial volumetric scatter
///   (Z Gaussian) are applied in `apply_voxel_cure_for_layer` per ADR-0018
///   when `printer.crosstalk_sigma_xy_um()` / `crosstalk_sigma_z_um()` are
///   `Some`. The four resulting runtime regimes (AA/BA/BB/CB/DD) are
///   enumerated in the `apply_voxel_cure_for_layer` doc.
#[cfg(feature = "field-sim")]
struct VoxelState {
    cure: CureField,
    pi: PhotoinitiatorField,
    /// t2f3 / ADR-0018 — per-voxel shrinkage strain. Dimension-locked to
    /// `cure` + `pi` at construction; written once per voxel during the
    /// `apply_voxel_shrinkage_for_layer` pass via `lock_strain_at`.
    strain: StrainField,
    /// t2f3 / ADR-0018 — per-voxel residual stress (MPa). Same
    /// dimensions as `strain`; written by `accumulate_layer_stress`.
    stress: StressField,
    dp_um: f32,
    ec_ref_mj_cm2: f32,
    ref_temp_c: f32,
    ea_cure_kj_mol: f32,
    k_d: f32,
    /// t2f3 — `ResinProfile::linear_shrinkage_pct() / 100`, dimensionless
    /// fraction. Snapshotted once at construction so per-layer iteration
    /// doesn't reread the profile.
    linear_shrinkage_frac: f32,
    /// t2f3 — `ResinProfile::effective_youngs_modulus_mpa()`. KB-163
    /// literature midpoint when uncalibrated.
    youngs_modulus_mpa: f32,
    /// t2f3 — `ResinProfile::effective_poissons_ratio()`.
    poissons_ratio: f32,
    /// t2f3 / KB-164 — `ResinProfile::effective_shrinkage_anisotropy_z_ratio()`.
    /// Default 1.5 (Z shrinks 50% more than XY) when uncalibrated.
    shrinkage_anisotropy_z_ratio: f32,
    led_power_mw_cm2: f32,
    /// LCD non-uniformity profile (KB-120). Derived from
    /// `printer.lcd_uniformity_variation` plus `printer.build_envelope_mm()`
    /// at run start. When the printer has no build envelope, a nominal
    /// Saturn-class fallback (192 × 120 mm) is used and a one-shot warn
    /// surfaces. With `variation == 0.0` the profile produces factor 1.0
    /// at every position — voxel-mode then matches pre-uniformity behaviour
    /// per-pixel.
    uniformity: UniformityProfile,
}

/// Output of [`SimulationRunner::prepare_layer_inputs`]: everything the
/// run-inner path needs derived from a CTB-style `LayerInput` slice, plus
/// the layer-height reconciliation between file-axis (CTB) and recipe-axis
/// (resin profile). See ADR-0005 Consequences "Policy: CTB as file-axis
/// authority" and ticket `ctb-layer-height-authority`.
///
/// **No I/O happens inside `prepare_layer_inputs`** — the helper is pure
/// data preparation. Callers inspect `layer_height_provenance.has_mismatch()`
/// and emit the user-facing warning themselves at the entry-point boundary
/// (next to where exit codes are decided). This keeps the helper testable
/// without a sink injection and avoids dyn-dispatch.
#[derive(Debug)]
struct PreparedInputs {
    areas: Vec<CrossSectionArea>,
    masks: Vec<LayerMask>,
    per_layer_overrides: Vec<(f32, f32)>,
    /// Per-layer CTB-authoritative layer heights as a typed value object.
    /// `.len() == areas.len()`. Each call to `predict_layer` and
    /// `apply_voxel_cure_for_layer` dispatches its layer's value via
    /// `layer_heights.get(i)`. Adaptive (variable-Z) CTBs are supported
    /// transparently — every layer carries its own slab thickness.
    /// Rationale: ADR-0005 Consequences "Policy: CTB as file-axis
    /// authority" + the `MismatchKind::Variable` branch on
    /// `LayerHeightProvenance`.
    layer_heights: LayerHeightSeq,
    /// Reconciliation outcome installed on the resulting `PrintSimulation`.
    layer_height_provenance: LayerHeightProvenance,
}

impl SimulationRunner {
    /// Run full simulation on an STL file.
    ///
    /// Ordering (ADR-0005 Consequences):
    ///   1. `resin.validate()` + `printer.validate()`
    ///   2. `pairing_validator::validate_pairing(printer, recipe)` — fail fast with ALL
    ///      violations BEFORE any geometry is sliced.
    ///   3. `slice_layers(..., recipe.layer_height_um, printer.voxel_size_mm)` — uses
    ///      the recipe + printer's configured voxel resolution.
    ///   4. Run FailurePredictor with mask-based SuctionDetector pre-pass.
    #[allow(clippy::too_many_arguments)]
    pub fn run_stl(
        stl_path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;
        let recipe = resin.recipe();

        let triangles = stl::load_stl(stl_path)?;
        let bbox = stl::bounding_box(&triangles);
        let geometries = geometry::slice_layers(
            &triangles,
            &bbox,
            recipe.layer_height_um(),
            printer.voxel_size_mm(),
        );
        let areas: Vec<CrossSectionArea> = geometries.iter().map(|g| g.area).collect();
        let masks: Vec<LayerMask> = geometries.into_iter().map(|g| g.mask).collect();
        // STL path: no CTB-derived layer_height; fall back to recipe.
        // Build a uniform per-layer Vec sized to the slice (the recipe
        // value is constant across every STL-sliced layer by construction
        // — slice_layers() uses recipe.layer_height_um as the Z-step).
        // Provenance is None (no file-axis value exists to reconcile).
        let layer_heights_um = vec![recipe.layer_height_um(); areas.len()];
        Self::run_inner(
            &areas,
            &masks,
            None,
            resin,
            printer,
            supports,
            plate,
            ambient,
            initial_led_temp,
            &layer_heights_um,
            None,
        )
    }

    /// Run simulation from pre-computed per-layer areas (area-only entry point).
    ///
    /// Mask-synthesising adapter (Phase B, Step 7, suction-detector-raft-false-positive):
    /// each area is represented as a fully-solid 1×1 LayerMask at the printer's
    /// voxel resolution. Fully-solid masks produce zero cavity events — correct
    /// for test fixtures whose areas represent solid cross-sections (e.g.
    /// `cube_areas`, `sphere_areas`). Callers that want to exercise cavity
    /// detection use [`run_from_layer_inputs`] with a bespoke LayerMask stack.
    ///
    /// Revalidation here is defence-in-depth per ADR-0005 §5.
    #[allow(clippy::too_many_arguments)]
    pub fn run_from_areas(
        areas: &[CrossSectionArea],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;

        let masks: Vec<LayerMask> = (0..areas.len())
            .map(|_| {
                LayerMask::new_all_solid(1, 1, printer.voxel_size_mm())
                    .expect("1×1 all-solid mask at validated positive voxel_size_mm constructs")
            })
            .collect();
        // Area-only path: no CTB; build a uniform per-layer Vec from the
        // recipe value. No provenance to install (file-axis value does
        // not exist).
        let layer_heights_um = vec![resin.recipe().layer_height_um(); areas.len()];
        Self::run_inner(
            areas,
            &masks,
            None,
            resin,
            printer,
            supports,
            plate,
            ambient,
            initial_led_temp,
            &layer_heights_um,
            None,
        )
    }

    /// Run simulation from parsed LayerInputs (from CTB or other sliced files).
    ///
    /// Uses per-layer exposure and lift speed from the sliced file; baseline recipe
    /// values (bottom_exposure_sec, bottom_layer_count) come from `resin.recipe()`.
    /// The runtime layer-height is sourced **per layer** from the CTB
    /// (`LayerInput.layer_height_um`, file-axis authority per ADR-0005), so
    /// adaptive (variable layer height) CTBs are first-class — each layer's
    /// physics uses its own slab thickness. When the CTB disagrees with
    /// `recipe.layer_height_um` — either by being uniform but different, or
    /// by being variable — a warning is emitted to stderr and the
    /// reconciliation is surfaced on `PrintSimulation::layer_height_provenance()`.
    ///
    /// Each `LayerInput` should carry a populated `mask` for cavity detection;
    /// inputs without a mask get a synthesised fully-solid 1×1 mask at
    /// `printer.voxel_size_mm()` (no cavity events emitted for those layers).
    #[allow(clippy::too_many_arguments)]
    pub fn run_from_layer_inputs(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;

        let prepared = Self::prepare_layer_inputs(layers, resin, printer)?;
        Self::emit_layer_height_warning_if_mismatch(
            &prepared.layer_height_provenance,
            resin.name(),
        );
        // Destructure to move the Vec instead of cloning — addresses the
        // round-1 LOW finding about clone overhead on large prints.
        let PreparedInputs {
            areas,
            masks,
            per_layer_overrides,
            layer_heights,
            layer_height_provenance,
        } = prepared;
        Self::run_inner(
            &areas,
            &masks,
            Some(&per_layer_overrides),
            resin,
            printer,
            supports,
            plate,
            ambient,
            initial_led_temp,
            layer_heights.as_slice(),
            Some(layer_height_provenance),
        )
    }

    /// Pure data-preparation helper shared by [`Self::run_from_layer_inputs`]
    /// and [`Self::run_from_layer_inputs_with_voxel`]. **No I/O** — the
    /// returned `PreparedInputs.layer_height_provenance` is what the callers
    /// inspect to decide whether to emit the user-facing stderr warning.
    ///
    /// Extracts areas, mask-fallback-synthesises, builds per-layer overrides,
    /// collects the **per-layer** CTB layer heights (adaptive / variable-Z
    /// CTBs are supported — each layer's slab thickness is dispatched
    /// individually downstream), and reconciles against
    /// `recipe.layer_height_um`. Rejects non-finite / non-positive
    /// `layer_height_um` values (covers the NaN gap noted in
    /// `docs/patterns/anti/rust-nan-positive-validation-gap.md`) via the
    /// extractor.
    fn prepare_layer_inputs(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
    ) -> Result<PreparedInputs, String> {
        let areas: Vec<CrossSectionArea> = layers
            .iter()
            .map(|li| {
                CrossSectionArea::new(li.cross_section_area_mm2)
                    .map_err(|e| format!("layer {}: {e}", li.index))
            })
            .collect::<Result<_, _>>()?;

        // Collect masks from LayerInputs, synthesising a fully-solid fallback
        // for any layer that doesn't carry one. Fallback voxel resolution must
        // match the mask-carrying layers to satisfy CavityDetector's consistency
        // precondition; pick it from the first carrying layer, or from
        // printer.voxel_size_mm() if none.
        let printer_voxel = printer.voxel_size_mm();
        let carrying_voxel = layers
            .iter()
            .find_map(|li| li.mask.as_ref().map(|m| m.voxel_size_mm()))
            .unwrap_or(printer_voxel);
        let carrying_dims = layers
            .iter()
            .find_map(|li| {
                li.mask
                    .as_ref()
                    .map(|m| (m.width_cells(), m.height_cells()))
            })
            .unwrap_or((1, 1));
        let masks: Vec<LayerMask> = layers
            .iter()
            .map(|li| match &li.mask {
                Some(m) => m.clone(),
                None => LayerMask::new_all_solid(carrying_dims.0, carrying_dims.1, carrying_voxel)
                    .expect("consistent dims + positive voxel_size yields valid all-solid mask"),
            })
            .collect();

        let per_layer_overrides: Vec<(f32, f32)> = layers
            .iter()
            .map(|li| (li.exposure_sec, li.lift_speed_mm_min))
            .collect();

        // CTB is the file-axis authority for layer height (ADR-0005
        // Consequences). LayerHeightSeq carries the per-layer Vec as a
        // domain value object — it's the runtime authority both for
        // per-layer dispatch and for the LayerHeightProvenance
        // reconciliation. Adaptive (variable-Z) CTBs surface a
        // MismatchKind::Variable; the uniform-but-disagrees case
        // surfaces MismatchKind::Uniform.
        let ctb_seq = LayerHeightSeq::from_layer_inputs(layers)?;
        let recipe_layer_height_um = resin.recipe().layer_height_um();
        let provenance = LayerHeightProvenance::reconcile(ctb_seq.clone(), recipe_layer_height_um)
            .map_err(|e| format!("layer-height reconciliation: {e}"))?;

        Ok(PreparedInputs {
            areas,
            masks,
            per_layer_overrides,
            layer_heights: ctb_seq,
            layer_height_provenance: provenance,
        })
    }

    /// Emit the layer-height-mismatch warning to stderr when present.
    /// Delegates the wording to [`LayerHeightProvenance::format_warning`]
    /// (behaviour lives with the data, not the runner).
    fn emit_layer_height_warning_if_mismatch(
        provenance: &LayerHeightProvenance,
        profile_name: &str,
    ) {
        if let Some(text) = provenance.format_warning(profile_name) {
            eprintln!("{text}");
        }
    }

    /// ADR-0017 / t2f1 voxel-cure-mode entry point. Identical to
    /// [`Self::run_from_layer_inputs`] but with an additional
    /// `voxel_cure_mm: Option<f32>` parameter:
    ///
    /// - `None` ⇒ Tier-1 scalar mode, identical to `run_from_layer_inputs`.
    /// - `Some(_)` ⇒ Tier-2 voxel mode: builds `CureField` +
    ///   `PhotoinitiatorField` from the layer masks, installs them on the
    ///   returned `PrintSimulation`, and overwrites each layer's
    ///   `cure_depth_um` / `worst_cure_depth_um` caches with the voxel
    ///   field's per-layer summary.
    ///
    /// Only available with the `field-sim` Cargo feature. Default builds
    /// don't see this method.
    #[cfg(feature = "field-sim")]
    #[allow(clippy::too_many_arguments)]
    pub fn run_from_layer_inputs_with_voxel(
        layers: &[LayerInput],
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
        voxel_cure_mm: Option<f32>,
    ) -> Result<PrintSimulation, String> {
        resin.validate().map_err(|e| format!("resin: {e}"))?;
        printer.validate().map_err(|e| format!("printer: {e}"))?;
        pairing_validator::validate_pairing(printer, resin.recipe())
            .map_err(|violations| format!("pairing: {}", violations.join("; ")))?;

        let prepared = Self::prepare_layer_inputs(layers, resin, printer)?;
        Self::emit_layer_height_warning_if_mismatch(
            &prepared.layer_height_provenance,
            resin.name(),
        );
        // Destructure to move the Vec instead of cloning.
        let PreparedInputs {
            areas,
            masks,
            per_layer_overrides,
            layer_heights,
            layer_height_provenance,
        } = prepared;
        Self::run_inner_full(
            &areas,
            &masks,
            Some(&per_layer_overrides),
            resin,
            printer,
            supports,
            plate,
            ambient,
            initial_led_temp,
            voxel_cure_mm,
            layer_heights.as_slice(),
            Some(layer_height_provenance),
        )
    }

    /// Internal: run the simulation given resolved areas + masks. Every public
    /// entry point converges here.
    ///
    /// `layer_heights_um` is the runtime authority for layer height (CTB
    /// per-layer Vec when the call came in via `run_from_layer_inputs*`,
    /// or a recipe-derived uniform Vec for STL / area-only paths). Length
    /// must equal `areas.len()`. `layer_height_provenance` is `Some` only
    /// for LayerInput-based runs and gets installed on the resulting
    /// `PrintSimulation`.
    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        areas: &[CrossSectionArea],
        masks: &[LayerMask],
        per_layer_overrides: Option<&[(f32, f32)]>,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
        layer_heights_um: &[f32],
        layer_height_provenance: Option<LayerHeightProvenance>,
    ) -> Result<PrintSimulation, String> {
        // Tier-1 scalar path: voxel mode disabled.
        Self::run_inner_full(
            areas,
            masks,
            per_layer_overrides,
            resin,
            printer,
            supports,
            plate,
            ambient,
            initial_led_temp,
            None,
            layer_heights_um,
            layer_height_provenance,
        )
    }

    /// Internal: full run with optional voxel cure mode (ADR-0017 / t2f1).
    /// `voxel_cure_mm: Some(_)` enables the Tier-2 path — a `CureField` and
    /// `PhotoinitiatorField` are built sized to the layer masks + layer count
    /// and installed on the returned aggregate. Each layer's predict_layer
    /// result has its `cure_depth_um` and `worst_cure_depth_um` cache fields
    /// overwritten with `LayerSummary.mean` / `LayerSummary.min` from the
    /// voxel field after the per-pixel exposure pass.
    ///
    /// V1 simplification: the cure field uses the LayerMask's `voxel_size_mm`
    /// for X/Y/Z. The CLI `--voxel-cure-mm` value is preserved on the call
    /// chain and serves as a request — when it disagrees with the mask
    /// resolution, the simulation runs anyway (mask wins for v1; t2f5 GPU
    /// work will introduce resolution decoupling). The Z-voxel index equals
    /// the layer index — one voxel slab per layer.
    #[allow(clippy::too_many_arguments)]
    fn run_inner_full(
        areas: &[CrossSectionArea],
        masks: &[LayerMask],
        per_layer_overrides: Option<&[(f32, f32)]>,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
        #[cfg_attr(not(feature = "field-sim"), allow(unused_variables))] voxel_cure_mm: Option<f32>,
        layer_heights_um: &[f32],
        layer_height_provenance: Option<LayerHeightProvenance>,
    ) -> Result<PrintSimulation, String> {
        // Caller-contract: `layer_heights_um` is indexed by layer index;
        // its length must equal `areas.len()` so per-layer dispatch can
        // pick the right slab thickness. Enforced by the two construction
        // sites: STL/area paths build a uniform Vec sized to areas.len();
        // CTB paths build a LayerHeightSeq via from_layer_inputs (same
        // length as the layer slice that produced areas). Violation =
        // programmer bug, not a runtime error, so debug_assert_eq! is
        // the right shape — panics loudly under debug builds and is a
        // no-op in release builds.
        debug_assert_eq!(
            layer_heights_um.len(),
            areas.len(),
            "internal contract: layer_heights_um indexed by layer must match areas len"
        );
        let recipe = resin.recipe();
        let suction_map = Self::build_suction_map(masks)?;
        let phases = LayerPhase::classify_sequence(areas, recipe);

        // Print-wide thermal context — constructed once, passed by reference
        // to every predict_layer call. ADR-0007 follow-on (step-10 review-code
        // LOW).
        let thermal = ThermalContext {
            ambient,
            initial_led_temp,
        };

        let mut sim = PrintSimulation::new(recipe.clone(), printer.clone());
        let mut prev_area = CrossSectionArea::new(0.0).expect("zero is valid");

        // ADR-0017 / t2f1: build the voxel state up-front when voxel mode is on.
        // `voxel_state` is `Some` only when (a) the feature is compiled in AND
        // (b) the caller passed `voxel_cure_mm: Some(_)` AND (c) masks/layers
        // are non-empty (zero-layer prints have nothing to voxelize).
        #[cfg(feature = "field-sim")]
        let mut voxel_state: Option<VoxelState> = if let Some(_requested_mm) = voxel_cure_mm {
            if let Some(first) = masks.first() {
                let nx = first.width_cells();
                let ny = first.height_cells();
                let voxel_size_mm = first.voxel_size_mm();
                let nz = areas.len() as u32;
                if nx > 0 && ny > 0 && nz > 0 {
                    let cure = CureField::new(nx, ny, nz, voxel_size_mm, [0.0, 0.0, 0.0])
                        .map_err(|e| format!("voxel cure field: {e}"))?;
                    let pi = PhotoinitiatorField::new(
                        nx,
                        ny,
                        nz,
                        resin.photoinitiator_concentration_initial(),
                    )
                    .map_err(|e| format!("photoinitiator field: {e}"))?;
                    // ADR-0018 / t2f3 — strain + stress fields are
                    // dimension-locked to (cure, pi) and allocated
                    // together. Their constructors honour the
                    // MAX_FIELD_ALLOCATION_BYTES budget — out-of-budget
                    // configurations fail BEFORE the per-layer loop
                    // begins (no silent kernel OOM).
                    let strain =
                        StrainField::new(nx, ny, nz, voxel_size_mm, [0.0, 0.0, 0.0])
                            .map_err(|e| format!("strain field: {e}"))?;
                    let stress =
                        StressField::new(nx, ny, nz, voxel_size_mm, [0.0, 0.0, 0.0])
                            .map_err(|e| format!("stress field: {e}"))?;
                    // KB-120 LCD non-uniformity profile. Sized to the cure
                    // field's lateral extent (nx × ny at voxel_size_mm) so
                    // the UniformityCalculator's radial cosine model centres
                    // on the mask's geometric centre. When the printer's
                    // build envelope is present and matches the mask span
                    // (typical CTB → mask path), the two are consistent.
                    // Variation 0.0 (or printer profile with no LCD
                    // uniformity data) collapses the per-pixel factor to
                    // 1.0 everywhere — identical-pixel behaviour preserved.
                    let plate_width_mm = nx as f32 * voxel_size_mm;
                    let plate_depth_mm = ny as f32 * voxel_size_mm;
                    let uniformity = UniformityProfile {
                        variation: printer.lcd_uniformity_variation(),
                        plate_width_mm,
                        plate_depth_mm,
                    };
                    Some(VoxelState {
                        cure,
                        pi,
                        strain,
                        stress,
                        dp_um: resin.penetration_depth_um(),
                        ec_ref_mj_cm2: resin.critical_energy_mj_cm2(),
                        ref_temp_c: resin.reference_temp_c(),
                        ea_cure_kj_mol: resin.effective_cure_kinetics_ea_kj_mol(),
                        k_d: resin.effective_photoinitiator_decay_constant_k_d(),
                        linear_shrinkage_frac: resin.linear_shrinkage_pct() / 100.0,
                        youngs_modulus_mpa: resin.effective_youngs_modulus_mpa(),
                        poissons_ratio: resin.effective_poissons_ratio(),
                        shrinkage_anisotropy_z_ratio: resin
                            .effective_shrinkage_anisotropy_z_ratio(),
                        led_power_mw_cm2: printer.led_power_mw_cm2(),
                        uniformity,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        for (i, &area) in areas.iter().enumerate() {
            let (exposure_override, lift_speed_override) = per_layer_overrides
                .and_then(|pl| pl.get(i).copied())
                .map(|(e, l)| (Some(e), Some(l)))
                .unwrap_or((None, None));
            let overrides = LayerOverrides {
                exposure_sec: exposure_override,
                lift_speed_mm_min: lift_speed_override,
                suction_force_n: suction_map.get(&(i as u32)).copied(),
                is_raft: matches!(phases.get(i), Some(LayerPhase::Raft)),
            };
            // Per-layer CTB-authoritative slab thickness — supports
            // adaptive (variable-Z) slicing transparently.
            let layer_height_um_i = layer_heights_um[i];
            #[allow(unused_mut)]
            let (mut result, mut failures) = FailurePredictor::predict_layer(
                i as u32,
                area,
                prev_area,
                &overrides,
                resin,
                printer,
                recipe,
                layer_height_um_i,
                supports,
                plate,
                &thermal,
            );

            // ADR-0017 / t2f1: voxel cure pass for this layer.
            // ADR-0018 / t2f3: strain + stress passes follow.
            #[cfg(feature = "field-sim")]
            if let Some(state) = voxel_state.as_mut() {
                Self::apply_voxel_cure_for_layer(
                    state,
                    i as u32,
                    &masks[i],
                    &thermal,
                    recipe,
                    layer_height_um_i,
                    printer,
                    &overrides,
                    &mut result,
                )?;

                // t2f3 — per-voxel shrinkage strain from the cure field
                // we just populated. Locks each voxel exactly once for
                // the layer (cured-layer-locks-strain invariant).
                Self::apply_voxel_shrinkage_for_layer(
                    state,
                    i as u32,
                    layer_height_um_i,
                    &thermal,
                    recipe,
                    printer,
                )?;

                // t2f3 — per-voxel linear-elastic stress from the strain
                // we just locked.
                Self::accumulate_layer_stress(state, i as u32)?;

                // t2f3 — populate the LayerResult per-layer aggregate
                // caches BEFORE the failure detector runs (the detector
                // reads from the live fields but consumers downstream
                // of sim.json need the cached scalars since the heavy
                // strain/stress fields are #[serde(skip)] per the
                // t2f3.5 follow-up). The Options are populated even
                // when zero — the presence of Some(_) is the signal
                // that the run was field-sim-enabled.
                result.strain_magnitude_max =
                    state.strain.magnitude_layer_max(i as u32).ok();
                result.stress_von_mises_max_mpa =
                    state.stress.von_mises_layer_max(i as u32).ok();
                result.strain_gradient_max_frac =
                    state.strain.gradient_layer_max(i as u32).ok();
                result.voxel_yield_fraction = state
                    .stress
                    .yield_fraction(i as u32, resin.tensile_strength_mpa())
                    .ok();

                // t2f3 — strain/stress-driven failure detection. Appends
                // any emitted WarpingRisk + CohesiveFailure events to
                // the per-layer failures vector BEFORE `add_layer`, so
                // they are persisted on the aggregate alongside the
                // Tier-1 failures from `predict_layer`.
                let strain_failures = FailurePredictor::predict_strain_failures(
                    i as u32,
                    &state.strain,
                    &state.stress,
                    resin,
                );
                failures.extend(strain_failures);
            }

            sim.add_layer(result, failures)
                .map_err(|e| format!("simulation: {e}"))?;
            prev_area = area;
        }

        #[cfg(feature = "field-sim")]
        if let Some(state) = voxel_state {
            sim.set_voxel_fields(state.cure, state.pi)
                .map_err(|e| format!("install voxel fields: {e}"))?;
            // ADR-0018 / t2f3 — parallel setter for the t2f3 fields.
            // Preserves the existing `set_voxel_fields(cure, pi)`
            // signature unchanged; the aggregate's `set_strain_stress_fields`
            // enforces the invariant that strain/stress dimensions match
            // the already-installed cure_field.
            sim.set_strain_stress_fields(state.strain, state.stress)
                .map_err(|e| format!("install strain/stress fields: {e}"))?;
        }

        // Install layer-height reconciliation on the aggregate. Present only
        // for runs entered via run_from_layer_inputs* (CTB / sliced-file
        // paths); STL / area-only paths pass `None` here.
        if let Some(provenance) = layer_height_provenance {
            sim.set_layer_height_provenance(provenance);
        }

        Ok(sim)
    }

    /// Apply the per-layer voxel cure pass: iterate set pixels of the layer
    /// mask, run `VoxelCureCalculator::apply_column_exposure` for each, then
    /// overwrite the LayerResult's Tier-1 cache fields with `LayerSummary`
    /// from the voxel field's Z-slab at this layer. v1 minimum-viable
    /// implementation per ADR-0017, extended per ADR-0018 / t2f2.
    ///
    /// **Four runtime regimes** based on
    /// `(printer.crosstalk_sigma_xy_um().is_some(), printer.crosstalk_sigma_z_um().is_some())`:
    /// - **(AA)** both None ⇒ t2f1 path unchanged: `mask.iter_solid()`
    ///   loop calling `apply_column_exposure` once per pixel at `iz_top = layer`.
    /// - **(BA/BB)** σ_xy Some, σ_z None ⇒ build per-layer intensity grid,
    ///   XY-convolve via `LightCrosstalkCalculator::apply_separable_2d`,
    ///   iterate FULL grid (off-mask pixels may have non-zero convolved
    ///   intensity), call `apply_column_exposure` once per (ix, iy) at
    ///   `iz_top = layer`.
    /// - **(CB)** σ_xy None, σ_z Some ⇒ no XY conv; per `iter_solid()`
    ///   pixel, call `compute_column_exposure` to obtain the dose column,
    ///   apply 1D Z convolution to the dose column via
    ///   `apply_separable_1d_z`, then deposit via `add_dose` + `deplete`.
    /// - **(DD)** both Some ⇒ XY-convolve intensity first, iterate full
    ///   grid; for each pixel, `compute_column_exposure` + Z-conv + deposit.
    ///
    /// Co-scattering of cure dose + PI depletion: the Z conv operates on
    /// the cure DOSE column (a linear quantity); the deposit-time
    /// `pi_field.deplete(k_d, convolved_dose[iz])` uses KB-160's
    /// multiplicative-exponential depletion law on the convolved dose at
    /// each voxel, so depletion correctly composes with scatter — see
    /// ADR-0018 §2 Approximation regime.
    #[cfg(feature = "field-sim")]
    #[allow(clippy::too_many_arguments)]
    fn apply_voxel_cure_for_layer(
        state: &mut VoxelState,
        layer: u32,
        mask: &LayerMask,
        thermal: &ThermalContext,
        recipe: &crate::entities::Recipe,
        layer_height_um: f32,
        printer: &PrinterProfile,
        overrides: &LayerOverrides,
        result: &mut crate::entities::LayerResult,
    ) -> Result<(), String> {
        let exposure_sec =
            overrides
                .exposure_sec
                .unwrap_or(if layer < recipe.bottom_layer_count() {
                    recipe.bottom_exposure_sec()
                } else {
                    recipe.normal_exposure_sec()
                });

        let dp = PenetrationDepth::new(state.dp_um).map_err(|e| format!("voxel dp: {e}"))?;

        // ADR-0018 / t2f2: detect the runtime regime from the printer's
        // crosstalk configuration. None/None ⇒ t2f1 fast path (regime AA).
        let sigma_xy_um = printer.crosstalk_sigma_xy_um();
        let sigma_z_um = printer.crosstalk_sigma_z_um();

        let voxel_size_mm = mask.voxel_size_mm();
        // Z-step depth is the CTB-authoritative layer_height_um (the actual
        // print layer thickness, file-axis per ADR-0005 + ticket
        // `ctb-layer-height-authority`), NOT the LATERAL mask voxel size
        // — Z-voxel index represents one layer slab. ADR-0017 §6
        // "Coordinates" + voxel_cure_calculator doc comment.
        if sigma_xy_um.is_none() && sigma_z_um.is_none() {
            // Regime AA: t2f1 path UNCHANGED for bit-exact equivalence.
            for (ix, iy) in mask.iter_solid() {
                let x_mm = (ix as f32 + 0.5) * voxel_size_mm;
                let y_mm = (iy as f32 + 0.5) * voxel_size_mm;
                let factor = UniformityCalculator::intensity_factor(x_mm, y_mm, &state.uniformity);
                let pixel_intensity = state.led_power_mw_cm2 * factor;
                VoxelCureCalculator::apply_column_exposure(
                    &mut state.cure,
                    &mut state.pi,
                    ix,
                    iy,
                    layer,
                    pixel_intensity,
                    exposure_sec,
                    dp,
                    state.k_d,
                    layer_height_um,
                )
                .map_err(|e| format!("voxel cure layer {layer}: {e}"))?;
            }
        } else {
            Self::apply_voxel_cure_for_layer_crosstalk(
                state,
                layer,
                mask,
                exposure_sec,
                dp,
                sigma_xy_um,
                sigma_z_um,
                layer_height_um,
            )?;
        }

        // Note: regimes BA/BB/CB/DD branched into
        // `apply_voxel_cure_for_layer_crosstalk` above; control returns
        // here and continues with the layer-summary recomputation below.

        // Recompute layer summary using KB-153 Ec(T) at the actual vat
        // temperature for this layer (single-source-arrhenius-helper
        // pattern — delegate to CureCalculator via VoxelCureCalculator).
        let vat_temp = crate::services::ThermalCalculator::vat_temperature_at_layer_v2(
            recipe,
            printer,
            thermal.ambient.value(),
            thermal.initial_led_temp.map(|t| t.value()),
            layer,
        );
        let ec_ref = Energy::new(state.ec_ref_mj_cm2).map_err(|e| format!("ec_ref: {e}"))?;
        let ec_t = VoxelCureCalculator::ec_at_temp(
            ec_ref,
            state.ref_temp_c,
            vat_temp,
            state.ea_cure_kj_mol,
        );

        // Replace the Tier-1 scalar cache with the voxel-derived summary.
        // LayerResult's `cure_depth_um` and `worst_cure_depth_um` stay as
        // `pub f32` caches; the dispatch methods on LayerResult fall back
        // to these when no aggregate cure_field is consulted, but here we
        // promote the voxel result into the cache so that direct field
        // readers (legacy callers) see the voxel-corrected number.
        if let Ok(summary) = state.cure.layer_summary(layer, state.dp_um, ec_t.value()) {
            if summary.mean.is_finite() && summary.mean >= 0.0 {
                result.cure_depth_um = summary.mean;
            }
            if summary.min.is_finite() && summary.min >= 0.0 {
                result.worst_cure_depth_um = summary.min;
            }
        }

        Ok(())
    }

    /// ADR-0018 / t2f3 — per-layer shrinkage strain pass.
    ///
    /// Walks every voxel of the layer's Z-slab, reads the cumulative
    /// absorbed dose from `state.cure` (populated by the upstream
    /// `apply_voxel_cure_for_layer` pass), converts dose → cure-extent
    /// via Beer-Lambert (KB-103) with Arrhenius Ec(T) (KB-153), and
    /// locks the resulting `StrainTensor` into `state.strain` exactly
    /// once. The cured-layer-locks-strain invariant is enforced by
    /// `StrainField::lock_strain_at`.
    ///
    /// Voxels with sub-threshold cure (dose ≤ Ec(T)) get
    /// `StrainTensor::zero()` — uncured liquid undergoes no
    /// shrinkage strain (KB-161).
    #[cfg(feature = "field-sim")]
    #[allow(clippy::too_many_arguments)]
    fn apply_voxel_shrinkage_for_layer(
        state: &mut VoxelState,
        layer: u32,
        layer_height_um: f32,
        thermal: &ThermalContext,
        recipe: &crate::entities::Recipe,
        printer: &PrinterProfile,
    ) -> Result<(), String> {
        // Snapshot temperature-dependent Ec(T) once per layer; the same
        // value is then used at every voxel. ec_at_temp signature mirrors
        // the existing voxel cure pass for consistency.
        let vat_temp = crate::services::ThermalCalculator::vat_temperature_at_layer_v2(
            recipe,
            printer,
            thermal.ambient.value(),
            thermal.initial_led_temp.map(|t| t.value()),
            layer,
        );
        let ec_t = crate::services::CureCalculator::ec_at_temp(
            Energy::new(state.ec_ref_mj_cm2).expect("ec_ref validated > 0 at profile load"),
            state.ref_temp_c,
            vat_temp,
            state.ea_cure_kj_mol,
        );

        let (nx, ny, _) = state.cure.dimensions();
        for ix in 0..nx {
            for iy in 0..ny {
                let dose = state
                    .cure
                    .dose_at(ix, iy, layer)
                    .map_err(|e| format!("cure dose_at({ix},{iy},{layer}): {e}"))?;
                let cure_extent = ShrinkageCalculator::cure_extent_at_voxel(
                    dose,
                    ec_t.value(),
                    state.dp_um,
                    layer_height_um,
                );
                let tensor = ShrinkageCalculator::free_shrinkage_strain_at_voxel(
                    cure_extent,
                    state.linear_shrinkage_frac,
                    state.shrinkage_anisotropy_z_ratio,
                );
                // Skip the lock_strain_at call entirely when the voxel
                // is uncured — leaves the default zero tensor and
                // avoids a no-op write that would still pass through
                // `AlreadyLocked` checks on revisit (defence in depth
                // for any future caller that revisits a layer slab).
                if tensor == crate::values::StrainTensor::zero() {
                    continue;
                }
                state
                    .strain
                    .lock_strain_at(ix, iy, layer, tensor)
                    .map_err(|e| format!("strain lock_strain_at({ix},{iy},{layer}): {e}"))?;
            }
        }
        Ok(())
    }

    /// ADR-0018 / t2f3 — per-layer linear-elastic stress accumulation.
    ///
    /// Walks every voxel of the layer's Z-slab, reads the strain we
    /// just locked, applies the closed-form 6×6 isotropic stiffness
    /// (KB-162) to produce a stress tensor, and writes it into
    /// `state.stress`. Voxels with zero strain produce zero stress and
    /// are skipped (the field is zero-initialised).
    #[cfg(feature = "field-sim")]
    fn accumulate_layer_stress(
        state: &mut VoxelState,
        layer: u32,
    ) -> Result<(), String> {
        let (nx, ny, _) = state.strain.dimensions();
        for ix in 0..nx {
            for iy in 0..ny {
                let eps = state
                    .strain
                    .strain_at(ix, iy, layer)
                    .map_err(|e| format!("strain_at({ix},{iy},{layer}): {e}"))?;
                if eps == crate::values::StrainTensor::zero() {
                    continue;
                }
                let sigma = StressAccumulator::strain_to_stress(
                    &eps,
                    state.youngs_modulus_mpa,
                    state.poissons_ratio,
                )
                .map_err(|e| format!("strain_to_stress({ix},{iy},{layer}): {e}"))?;
                state
                    .stress
                    .accumulate_at(ix, iy, layer, sigma)
                    .map_err(|e| format!("stress accumulate_at({ix},{iy},{layer}): {e}"))?;
            }
        }
        Ok(())
    }

    /// ADR-0018 / t2f2 crosstalk helper. Handles regimes BA/BB/CB/DD.
    /// Called from `apply_voxel_cure_for_layer` when at least one σ is Some.
    ///
    /// Algorithm:
    /// 1. Build a 2D pixel intensity grid for this layer (mask × uniformity ×
    ///    led_power).
    /// 2. If σ_xy is Some: XY 2D Gaussian convolution on the intensity grid
    ///    (LCD source crosstalk + lateral resin scatter component).
    /// 3. Iterate the (possibly post-XY-conv) grid:
    ///    - σ_xy active ⇒ FULL grid (off-mask pixels may now have non-zero
    ///      intensity);
    ///    - σ_xy None ⇒ `mask.iter_solid()` only (no XY spread).
    /// 4. For each pixel: snapshot PI column, call `compute_column_exposure`
    ///    to obtain the Beer-Lambert dose column (Vec<f32>).
    /// 5. If σ_z is Some: 1D Z Gaussian convolution on the dose column.
    /// 6. Deposit: for each in-bounds iz, `cure.add_dose` + `pi.deplete`
    ///    using the (possibly Z-convolved) dose at iz. KB-160 multiplicative
    ///    depletion correctly co-scatters with the convolved linear dose.
    #[cfg(feature = "field-sim")]
    #[allow(clippy::too_many_arguments)]
    fn apply_voxel_cure_for_layer_crosstalk(
        state: &mut VoxelState,
        layer: u32,
        mask: &LayerMask,
        exposure_sec: f32,
        dp: PenetrationDepth,
        sigma_xy_um: Option<f32>,
        sigma_z_um: Option<f32>,
        layer_height_um: f32,
    ) -> Result<(), String> {
        use ndarray::Array2;

        let (nx, ny, nz) = state.cure.dimensions();
        let voxel_size_mm = mask.voxel_size_mm();

        // (1) Build the 2D intensity grid from the solid mask + KB-120
        // uniformity factor. Off-mask pixels stay at 0.
        let mut intensity = Array2::<f32>::zeros((nx as usize, ny as usize));
        for (ix, iy) in mask.iter_solid() {
            let x_mm = (ix as f32 + 0.5) * voxel_size_mm;
            let y_mm = (iy as f32 + 0.5) * voxel_size_mm;
            let factor = UniformityCalculator::intensity_factor(x_mm, y_mm, &state.uniformity);
            intensity[(ix as usize, iy as usize)] = state.led_power_mw_cm2 * factor;
        }

        // (2) XY pre-convolution (if σ_xy active).
        let xy_active = if let Some(sigma_xy) = sigma_xy_um {
            let sigma_xy_voxels = sigma_xy / (voxel_size_mm * 1000.0);
            let xy_kernel = LightCrosstalkCalculator::build_separable_kernel(sigma_xy_voxels)
                .map_err(|e| format!("xy kernel layer {layer}: {e:?}"))?;
            let mut xy_scratch = Array2::<f32>::zeros((nx as usize, ny as usize));
            LightCrosstalkCalculator::apply_separable_2d(
                &mut intensity,
                &xy_kernel,
                &mut xy_scratch,
            )
            .map_err(|e| format!("xy conv layer {layer}: {e:?}"))?;
            true
        } else {
            false
        };

        // (3) Build the Z kernel + reusable per-column scratch buffers if σ_z active.
        let z_kernel = if let Some(sigma_z) = sigma_z_um {
            let sigma_z_layers = sigma_z / layer_height_um;
            Some(
                LightCrosstalkCalculator::build_separable_kernel(sigma_z_layers)
                    .map_err(|e| format!("z kernel layer {layer}: {e:?}"))?,
            )
        } else {
            None
        };
        let mut z_scratch_column: Vec<f32> = vec![0.0; nz as usize];

        // (4) Iterate pixels. When σ_xy is active we walk the full grid
        // (post-conv intensity may be non-zero off the original mask);
        // otherwise only iter_solid pixels see exposure.
        let iter_pixels: Box<dyn Iterator<Item = (u32, u32)>> = if xy_active {
            Box::new((0..ny).flat_map(move |iy| (0..nx).map(move |ix| (ix, iy))))
        } else {
            Box::new(mask.iter_solid())
        };
        for (ix, iy) in iter_pixels {
            let pixel_intensity = intensity[(ix as usize, iy as usize)];
            if pixel_intensity == 0.0 {
                continue;
            }

            // (5) PI column snapshot + compute_column_exposure → dose column.
            let pi_snapshot = state
                .pi
                .column_at(ix, iy)
                .map_err(|e| format!("pi snapshot ({ix},{iy}) layer {layer}: {e}"))?;
            let mut dose_col = VoxelCureCalculator::compute_column_exposure(
                &pi_snapshot,
                layer,
                nz,
                pixel_intensity,
                exposure_sec,
                dp,
                state.k_d,
                layer_height_um,
            )
            .map_err(|e| format!("compute col ({ix},{iy}) layer {layer}: {e}"))?;

            // (6) Z convolution on the dose column (if σ_z active).
            if let Some(ref zk) = z_kernel {
                LightCrosstalkCalculator::apply_separable_1d_z(
                    &mut dose_col,
                    zk,
                    &mut z_scratch_column,
                )
                .map_err(|e| format!("z conv ({ix},{iy}) layer {layer}: {e:?}"))?;
            }

            // (7) Deposit: add_dose + deplete at each iz with non-zero
            // convolved dose. KB-160 multiplicative depletion is applied
            // per-voxel using local C(iz), composing correctly with the
            // convolved dose (linear) — see ADR-0018 §2 Approximation regime.
            for iz in 0..nz {
                let dose = dose_col[iz as usize];
                if dose == 0.0 {
                    continue;
                }
                state
                    .cure
                    .add_dose(ix, iy, iz, dose)
                    .map_err(|e| format!("add_dose ({ix},{iy},{iz}) layer {layer}: {e}"))?;
                state
                    .pi
                    .deplete(ix, iy, iz, state.k_d, dose)
                    .map_err(|e| format!("deplete ({ix},{iy},{iz}) layer {layer}: {e}"))?;
            }
        }
        Ok(())
    }

    /// Run SuctionDetector mask-based pre-pass and build a layer→force map.
    ///
    /// Propagates `CavityError` as a human-readable string — callers of the
    /// public `run_*` entry points already return `Result<_, String>`.
    fn build_suction_map(masks: &[LayerMask]) -> Result<HashMap<u32, f32>, String> {
        let risks = SuctionDetector::detect_from_masks(masks)
            .map_err(|e| format!("suction detection: {e}"))?;
        Ok(risks
            .into_iter()
            .map(|r| (r.layer, r.suction_force_n))
            .collect())
    }

    /// Auto-detect format from file extension and run simulation.
    #[allow(clippy::too_many_arguments)]
    pub fn run_auto(
        path: &std::path::Path,
        resin: &ResinProfile,
        printer: &PrinterProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        ambient: AmbientTemperature,
        initial_led_temp: Option<InitialLedTemperature>,
    ) -> Result<PrintSimulation, String> {
        let format = crate::io::sliced::detect_format(path)
            .ok_or_else(|| format!("unknown file format: {}", path.display()))?;

        match format {
            "STL" => Self::run_stl(
                path,
                resin,
                printer,
                supports,
                plate,
                ambient,
                initial_led_temp,
            ),
            "CTB" => {
                let (_info, layers) = crate::io::ctb::parse_ctb(path)?;
                Self::run_from_layer_inputs(
                    &layers,
                    resin,
                    printer,
                    supports,
                    plate,
                    ambient,
                    initial_led_temp,
                )
            }
            other => Err(format!("format {other} not yet supported for simulation")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_plate() -> PlateAdhesionProfile {
        PlateAdhesionProfile::default_textured()
    }

    fn test_ambient() -> AmbientTemperature {
        AmbientTemperature::new(22.0)
            .expect("test fixture: 22.0 °C is in AmbientTemperature domain")
    }

    fn cube_areas(n_layers: usize, area: f64) -> Vec<CrossSectionArea> {
        vec![CrossSectionArea::new(area).expect("test area is non-negative"); n_layers]
    }

    // ---- Layer-height warning wording tests moved (DDD: behaviour
    //      with the data). See
    //      crates/resinsim-core/src/values/layer_height_provenance.rs
    //      `format_warning_*` tests, which cover the uniform branch,
    //      the variable-Z branch, and the collision-aware variable-Z
    //      sub-branch (round-1 UX MED fix).

    fn sphere_areas(n_layers: usize, radius_mm: f64) -> Vec<CrossSectionArea> {
        let layer_height = 2.0 * radius_mm / n_layers as f64;
        (0..n_layers)
            .map(|i| {
                let h = (i as f64 + 0.5) * layer_height;
                let d = (h - radius_mm).abs();
                let a = std::f64::consts::PI * (radius_mm * radius_mm - d * d);
                CrossSectionArea::new(a.max(0.0)).expect("max(0.0) guarantees non-negative")
            })
            .collect()
    }

    #[test]
    fn cube_constant_force_across_layers() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        let first_force = layers[10].total_force_n;
        let last_force = layers[99].total_force_n;
        assert!(
            (first_force - last_force).abs() < 0.1,
            "force should be ~constant: first={first_force}, last={last_force}"
        );
    }

    #[test]
    fn sphere_force_peaks_at_equator() {
        let areas = sphere_areas(200, 10.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 30 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let summary = sim.summary();
        assert!(
            summary.max_force_layer > 80 && summary.max_force_layer < 120,
            "max force should be near equator, got layer {}",
            summary.max_force_layer
        );
    }

    #[test]
    fn cube_no_critical_failures_with_adequate_supports() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert_eq!(
            sim.summary().critical_failures,
            0,
            "small cube should have no failures"
        );
    }

    #[test]
    fn large_area_with_plate_adhesion_may_survive() {
        // 5000 mm² cross section, peel = 65 N
        // 5 supports = 21.99 N, but interlayer bond = 50 × 5000 × 0.001 = 250 N
        // Total = 271.99 N >> 65 N → passes with interlayer bond
        let areas = cube_areas(50, 5000.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 5 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let overload_count = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SupportOverload)
            .count();
        assert_eq!(
            overload_count, 0,
            "interlayer bond should prevent support overload"
        );
    }

    #[test]
    fn no_supports_no_plate_fails() {
        // Remove both plate adhesion and supports → guaranteed failure
        let areas = cube_areas(50, 500.0);
        let no_plate = PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0,
            bottom_layer_count: 0,
            interlayer_bond_kpa: 0.0,
        };
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.0, n_supports: 0 }, &no_plate, test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        assert!(
            sim.summary().critical_failures > 0,
            "no supports + no plate should fail"
        );
    }

    /// Construct a closed-cup LayerInput stack with a bespoke LayerMask:
    /// `base_layers` solid layers (build-plate floor) → `wall_layers` ring-wall
    /// layers (trapped void interior) → `cap_layers` solid layers (FEP-side
    /// closure). Uses 7×7 voxel grid at 1mm voxel → 5×5 interior = 25 mm²
    /// sealed area (above the 1 N downstream threshold).
    ///
    /// Rewritten in Phase B Step 7 (suction-detector-raft-false-positive):
    /// previously this test used area-only sequences, which no longer exercise
    /// cavity detection under the mask-based path.
    fn closed_cup_layer_inputs(
        base_layers: usize,
        wall_layers: usize,
        cap_layers: usize,
        exposure_sec: f32,
        layer_height_um: f32,
        lift_speed_mm_min: f32,
    ) -> Vec<LayerInput> {
        let w = 7u32;
        let h = 7u32;
        let voxel = 1.0_f32;

        let solid_mask = LayerMask::new_all_solid(w, h, voxel).expect("7×7 @ 1mm mask constructs");
        let ring_mask = {
            let mut m = LayerMask::new_all_solid(w, h, voxel).expect("7×7 @ 1mm mask constructs");
            for x in 1..w - 1 {
                for y in 1..h - 1 {
                    m.clear(x, y).expect("interior cell in bounds");
                }
            }
            m
        };

        let solid_area = (w as f64) * (h as f64) * (voxel as f64).powi(2); // 49 mm²
        let ring_area = solid_area - 25.0; // 24 mm² (wall ring)

        let mut layers = Vec::new();
        let mut idx: u32 = 0;
        let mut z_mm = 0.0_f32;
        let layer_height_mm = layer_height_um / 1000.0;
        for _ in 0..base_layers {
            layers.push(
                LayerInput::new(
                    idx,
                    solid_area,
                    exposure_sec,
                    lift_speed_mm_min,
                    layer_height_um,
                    z_mm,
                )
                .expect("valid LayerInput")
                .with_mask(solid_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        for _ in 0..wall_layers {
            layers.push(
                LayerInput::new(
                    idx,
                    ring_area,
                    exposure_sec,
                    lift_speed_mm_min,
                    layer_height_um,
                    z_mm,
                )
                .expect("valid LayerInput")
                .with_mask(ring_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        for _ in 0..cap_layers {
            layers.push(
                LayerInput::new(
                    idx,
                    solid_area,
                    exposure_sec,
                    lift_speed_mm_min,
                    layer_height_um,
                    z_mm,
                )
                .expect("valid LayerInput")
                .with_mask(solid_mask.clone()),
            );
            idx += 1;
            z_mm += layer_height_mm;
        }
        layers
    }

    #[test]
    fn closed_cup_triggers_suction_warning() {
        // 5 solid base + 10 ring walls + 1 cap (layer 15 is the closure).
        // Interior = 5×5 = 25 mm² at 1mm voxel. Force = 50 kPa × 25 × 1e-3 = 1.25 N
        // — above FailurePredictor's 1 N emission gate.
        let layers = closed_cup_layer_inputs(5, 10, 1, 2.5, 50.0, 60.0);
        let sim = SimulationRunner::run_from_layer_inputs(
            &layers,
            &ResinProfile::generic_standard(),
            &PrinterProfile::generic_msla_4k(),
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            test_ambient(),
            None,
        )
        .expect("test fixture: validated profiles satisfy run_from_layer_inputs preconditions");
        let suction_events: Vec<_> = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .collect();
        assert!(
            !suction_events.is_empty(),
            "closed cup should trigger suction warning, got: {:?}",
            sim.failures()
        );
        // Event at the closure layer (15 = 5 base + 10 walls).
        assert_eq!(suction_events[0].layer, 15);
    }

    #[test]
    fn solid_cube_no_suction() {
        let areas = cube_areas(100, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let suction_count = sim
            .failures()
            .iter()
            .filter(|f| f.failure_type == crate::entities::FailureType::SuctionCup)
            .count();
        assert_eq!(suction_count, 0, "solid cube should have no suction");
    }

    #[test]
    fn suction_adds_to_total_force() {
        // Same fixture as closed_cup_triggers_suction_warning; check that the
        // closure layer's total_force exceeds its peel_force.
        let layers = closed_cup_layer_inputs(5, 10, 1, 2.5, 50.0, 60.0);
        let sim = SimulationRunner::run_from_layer_inputs(
            &layers,
            &ResinProfile::generic_standard(),
            &PrinterProfile::generic_msla_4k(),
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            test_ambient(),
            None,
        )
        .expect("test fixture: validated profiles satisfy run_from_layer_inputs preconditions");
        let closure_layer = &sim.layers()[15];
        assert!(
            closure_layer.suction_force_n > 0.0,
            "closure layer should have suction force, got {}",
            closure_layer.suction_force_n
        );
        assert!(
            closure_layer.total_force_n > closure_layer.peel_force_n,
            "total should exceed peel when suction present"
        );
    }

    #[test]
    fn temperature_rises_over_long_print() {
        let areas = cube_areas(500, 100.0);
        let sim = SimulationRunner::run_from_areas(
            &areas, &ResinProfile::generic_standard(), &PrinterProfile::generic_msla_4k(),
            &SupportConfig { tip_radius_mm: 0.2, n_supports: 20 }, &default_plate(), test_ambient(), None,
        ).expect("test fixture: validated factory profiles satisfy SimulationRunner::run_from_areas preconditions");
        let layers = sim.layers();
        assert!(layers[490].vat_temperature_c > layers[10].vat_temperature_c + 3.0);
        assert!(layers[490].viscosity_mpa_s < layers[10].viscosity_mpa_s);
    }

    // --- Step 12: profile fixture invariant tests ---

    #[test]
    fn generic_msla_4k_passes_validate() {
        PrinterProfile::generic_msla_4k()
            .validate()
            .expect("PrinterProfile::generic_msla_4k() factory must satisfy validate()");
    }

    #[test]
    fn generic_standard_resin_passes_validate() {
        ResinProfile::generic_standard()
            .validate()
            .expect("ResinProfile::generic_standard() factory must satisfy validate()");
    }

    #[test]
    fn invalid_printer_profile_returns_err() {
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.lcd_uniformity_variation = 2.0; // outside [0, 1]
        let areas = cube_areas(5, 100.0);
        let result = SimulationRunner::run_from_areas(
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            test_ambient(),
            None,
        );
        assert!(
            result.is_err(),
            "invalid profile should be rejected at entry point"
        );
    }

    // --- ADR-0005: pairing runs before slicing. Locks the ordering invariant that
    // a recipe outside the printer envelope fails fast at simulation entry, not
    // after geometry has been sliced into layer areas. ---

    #[test]
    fn pairing_violation_returns_err_before_slice_areas() {
        // Narrow printer envelope that excludes the resin's recipe layer height.
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.layer_height_range_um = crate::values::FloatRange::new(100.0, 150.0)
            .expect("test fixture: 100..150 µm range is valid");
        let areas = cube_areas(5, 100.0);
        // generic_standard recipe has layer_height_um = 50.0 → outside the narrowed range.
        let err = SimulationRunner::run_from_areas(
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            test_ambient(),
            None,
        )
        .expect_err("pairing violation must fail simulation entry");
        assert!(
            err.starts_with("pairing:"),
            "err must identify pairing stage: {err}"
        );
        assert!(
            err.contains("layer_height_um"),
            "err must name the offending recipe field: {err}"
        );
    }

    #[test]
    fn pairing_reports_all_violations_at_once() {
        let mut printer = PrinterProfile::generic_msla_4k();
        printer.layer_height_range_um = crate::values::FloatRange::new(100.0, 150.0)
            .expect("test fixture: 100..150 µm range is valid");
        printer.exposure_range_sec = crate::values::FloatRange::new(10.0, 60.0)
            .expect("test fixture: 10..60 sec range is valid");
        let areas = cube_areas(5, 100.0);
        let err = SimulationRunner::run_from_areas(
            &areas,
            &ResinProfile::generic_standard(),
            &printer,
            &SupportConfig {
                tip_radius_mm: 0.2,
                n_supports: 10,
            },
            &default_plate(),
            test_ambient(),
            None,
        )
        .expect_err("multiple pairing violations must fail");
        // Both layer_height AND exposure violate; violations are joined with "; ".
        assert!(err.contains("layer_height_um"));
        assert!(err.contains("normal_exposure_sec"));
    }
}
