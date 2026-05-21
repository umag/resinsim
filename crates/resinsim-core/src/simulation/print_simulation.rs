use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "field-sim")]
use crate::entities::ResinProfile;
use crate::entities::{FailureEvent, LayerResult, PrinterProfile, Recipe, Severity};
#[cfg(feature = "field-sim")]
use crate::services::CureCalculator;
use crate::services::LayerTimingCalculator;
use crate::values::LayerHeightProvenance;
#[cfg(feature = "field-sim")]
use crate::values::{
    CureDepth, CureField, Energy, PhotoinitiatorField, StrainField, StressField, ThermalField,
    VatTemperature,
};

/// Errors returned by [`PrintSimulation`] mutators.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AggregateError {
    /// `add_layer` was called with a `LayerResult` whose `index` did not
    /// match the next sequential position. The aggregate's layer Vec is
    /// append-only and contiguous from 0; mis-ordered layers are caller
    /// bugs, not domain failures.
    #[error("layers must be sequential: expected {expected}, got {got}")]
    NonContiguousLayer { expected: u32, got: u32 },
    /// `set_voxel_fields` was called with a CureField and PhotoinitiatorField
    /// whose dimensions disagree. The two fields must share `(nx, ny, nz)`
    /// because every cure-mode iteration touches both at the same voxel
    /// index. Caller bug; ADR-0017 invariant.
    #[error("voxel field dimensions must match: cure={cure_dims:?}, photoinitiator={pi_dims:?}")]
    VoxelFieldDimensionMismatch {
        cure_dims: (u32, u32, u32),
        pi_dims: (u32, u32, u32),
    },
    /// `set_strain_stress_fields` was called with a StrainField and
    /// StressField whose dimensions disagree, or without a previously-
    /// installed `cure_field` whose dimensions they should mirror. ADR-
    /// 0018 invariant: strain ⇔ stress ⇔ cure all share `(nx, ny, nz)`.
    #[error(
        "strain/stress field dimensions must match cure field: strain={strain_dims:?}, \
         stress={stress_dims:?}, cure={cure_dims:?}"
    )]
    StrainStressFieldDimensionMismatch {
        strain_dims: (u32, u32, u32),
        stress_dims: (u32, u32, u32),
        cure_dims: Option<(u32, u32, u32)>,
    },
    /// `set_strain_stress_fields` was called before `set_voxel_fields`.
    /// ADR-0018 says strain is a downstream of cure: you cannot install
    /// strain/stress without first installing cure.
    #[error("set_strain_stress_fields requires a previously-installed cure_field")]
    StrainStressWithoutCureField,
}

/// Aggregate root: a complete simulation run for one geometry + resin + printer.
///
/// The aggregate OWNS the `Recipe` and `PrinterProfile` it was constructed
/// with. Projections (`summary()`, future force/temperature stats) read
/// those owned domain entities internally — callers don't re-thread them.
/// Layers and failures are only mutated through the root's methods.
///
/// # Deserialize note
///
/// `#[derive(Deserialize)]` reconstructs the aggregate directly from fields
/// — it does NOT call `Recipe::validate()` or `PrinterProfile::validate()`
/// on the child entities. Current code has zero external deserializers of
/// `PrintSimulation`; if a future consumer deserializes one, wrap the
/// output with an explicit validate() pass on the child entities before
/// treating the aggregate as trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintSimulation {
    /// Domain-validated `Recipe` (Clone-owned) used by `summary()` phase-time
    /// projection. Immutable for the aggregate's lifetime — no public
    /// mutator exposed.
    recipe: Recipe,
    /// Domain-validated `PrinterProfile` (Clone-owned) used by `summary()`
    /// phase-time projection. Immutable for the aggregate's lifetime.
    printer: PrinterProfile,
    layers: Vec<LayerResult>,
    failures: Vec<FailureEvent>,
    /// Voxel cure dose field (ADR-0017 / t2f1). Populated by
    /// `SimulationRunner` only when the `--voxel-cure-mm` flag is set;
    /// `None` for Tier-1 scalar runs. Aggregate invariant: when `Some`,
    /// the field's bbox must contain every layer's solid region.
    ///
    /// ADR-0019 / t2f3.5: persisted via the paired binary sidecar
    /// (`<stem>.fields.bin`), NOT inline in the sim.json envelope.
    /// `#[serde(skip)]` keeps this out of the JSON projection; the
    /// repository layer (`simulation_repo.rs`) reattaches voxel fields
    /// from the sidecar on `load_envelope`.
    #[cfg(feature = "field-sim")]
    #[serde(default, skip)]
    cure_field: Option<CureField>,
    /// Per-voxel photoinitiator concentration field (KB-160). Populated
    /// in lockstep with `cure_field`. Persisted via the sidecar per
    /// ADR-0019 (same rationale as `cure_field` above).
    #[cfg(feature = "field-sim")]
    #[serde(default, skip)]
    photoinitiator_field: Option<PhotoinitiatorField>,
    /// Per-voxel cure-driven shrinkage strain (ADR-0018 / t2f3).
    /// Populated only when voxel-cure mode is active (full Tier-2);
    /// dimension-locked to `cure_field` and `stress_field`.
    ///
    /// ADR-0019 / t2f3.5: persisted via the paired binary sidecar.
    /// Previously `#[serde(skip)]` because dense JSON would balloon to
    /// ~250 GB; now persisted losslessly in the sidecar's RSFIELD format.
    #[cfg(feature = "field-sim")]
    #[serde(default, skip)]
    strain_field: Option<StrainField>,
    /// Per-voxel residual stress in MPa (ADR-0018 / t2f3). Persisted
    /// via the sidecar per ADR-0019 (same rationale as `strain_field`).
    #[cfg(feature = "field-sim")]
    #[serde(default, skip)]
    stress_field: Option<StressField>,
    /// Per-voxel temperature field over the full vat envelope (ADR-0020 /
    /// t2f4). Populated by `SimulationRunner` only when `--voxel-cure-mm`
    /// is set (the auto-activation rule per ADR-0020 §Decision vii — Tier-2
    /// thermal diffusion and Tier-2 voxel cure are physically coupled).
    /// `None` for Tier-1 scalar runs.
    ///
    /// **Aggregate-shape note**: this field uses a DIFFERENT bbox + voxel
    /// size from cure/photoinitiator/strain/stress — those are part-bbox
    /// anchored, ThermalField is vat-envelope anchored
    /// (`PrinterProfile.build_envelope_mm`). The aggregate invariant
    /// allows divergent anchoring per `docs/patterns/thermal-field-z-dim-is-spatial.md`.
    ///
    /// ADR-0019 / t2f3.5: persisted via the paired binary sidecar
    /// (`<stem>.fields.bin`), kind_tag = 4 = Thermal (ADR-0020 §Decision x).
    #[cfg(feature = "field-sim")]
    #[serde(default, skip)]
    thermal_field: Option<ThermalField>,
    /// Reconciliation between the CTB's embedded layer-height (file-axis,
    /// runtime authority) and the resin recipe's `layer_height_um`
    /// (recipe-axis, authoring metadata). Per ADR-0005 Consequences
    /// "Policy: CTB as file-axis authority" + ticket
    /// `ctb-layer-height-authority`. Present on runs that entered via
    /// `run_from_layer_inputs*` (CTB / sliced-file paths); absent on STL /
    /// area-only paths where no CTB-derived value exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layer_height_provenance: Option<LayerHeightProvenance>,
}

/// Summary statistics for a completed simulation.
///
/// Time fields (`total_time_sec`, `bottom_time_sec`, `transition_time_sec`,
/// `normal_time_sec`) are filled from `LayerTimingCalculator::cumulative_times_sec`
/// against the aggregate's owned `Recipe + PrinterProfile`. Per-phase fields
/// sum to `total_time_sec` within f32 tolerance; all are zero when
/// `total_layers == 0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimSummary {
    pub total_layers: u32,
    pub critical_failures: usize,
    pub warnings: usize,
    pub max_peel_force_n: f32,
    pub max_force_layer: u32,
    pub min_safety_factor: f32,
    pub min_safety_layer: u32,
    pub max_temperature_c: f32,
    pub max_z_deflection_um: f32,
    pub total_time_sec: f32,
    pub bottom_time_sec: f32,
    pub transition_time_sec: f32,
    pub normal_time_sec: f32,
}

impl PrintSimulation {
    /// Construct an empty aggregate pinned to a `Recipe + PrinterProfile`
    /// pair. Both entities are Clone-owned; callers pass by value (typically
    /// via `.clone()` from refs they already hold).
    pub fn new(recipe: Recipe, printer: PrinterProfile) -> Self {
        Self {
            recipe,
            printer,
            layers: Vec::new(),
            failures: Vec::new(),
            #[cfg(feature = "field-sim")]
            cure_field: None,
            #[cfg(feature = "field-sim")]
            photoinitiator_field: None,
            #[cfg(feature = "field-sim")]
            strain_field: None,
            #[cfg(feature = "field-sim")]
            stress_field: None,
            #[cfg(feature = "field-sim")]
            thermal_field: None,
            layer_height_provenance: None,
        }
    }

    /// Add a layer result and its associated failures.
    ///
    /// Enforces invariant: layers must be added sequentially. Returns
    /// `Err(AggregateError::NonContiguousLayer { expected, got })` when
    /// `result.index` does not match `self.layers.len() as u32` — replaces
    /// the older `assert_eq!` panic so callers can recover or surface the
    /// error through their own `Result` chain.
    ///
    /// # Move semantics on `Err`
    ///
    /// `layer_failures` is moved into the call. On `Err` it is dropped, not
    /// returned — callers that want to retry must reconstruct the failures
    /// vector. Currently only `SimulationRunner::run_inner` (private) calls
    /// this; it propagates via `?` and never retries, so the drop is the
    /// same outcome as the historical panic path.
    pub fn add_layer(
        &mut self,
        result: LayerResult,
        mut layer_failures: Vec<FailureEvent>,
    ) -> Result<(), AggregateError> {
        let expected = self.layers.len() as u32;
        if result.index != expected {
            return Err(AggregateError::NonContiguousLayer {
                expected,
                got: result.index,
            });
        }
        self.layers.push(result);
        self.failures.append(&mut layer_failures);
        Ok(())
    }

    pub fn layers(&self) -> &[LayerResult] {
        &self.layers
    }

    pub fn failures(&self) -> &[FailureEvent] {
        &self.failures
    }

    /// Voxel cure dose field (ADR-0017 / t2f1). Returns `None` when the
    /// simulation was run in Tier-1 scalar mode (no `--voxel-cure-mm` flag).
    /// Only present in builds with the `field-sim` Cargo feature.
    #[cfg(feature = "field-sim")]
    pub fn cure_field(&self) -> Option<&CureField> {
        self.cure_field.as_ref()
    }

    /// Per-voxel photoinitiator concentration field (KB-160). Populated
    /// in lockstep with `cure_field`.
    #[cfg(feature = "field-sim")]
    pub fn photoinitiator_field(&self) -> Option<&PhotoinitiatorField> {
        self.photoinitiator_field.as_ref()
    }

    /// Mutable borrow of the voxel cure field — used by SimulationRunner
    /// while orchestrating the voxel path. Outside of the runner this is
    /// not the right API; consumers should use [`Self::cure_field`].
    #[cfg(feature = "field-sim")]
    pub fn cure_field_mut(&mut self) -> Option<&mut CureField> {
        self.cure_field.as_mut()
    }

    /// Mutable borrow of the photoinitiator field. Symmetric with
    /// [`Self::cure_field_mut`].
    #[cfg(feature = "field-sim")]
    pub fn photoinitiator_field_mut(&mut self) -> Option<&mut PhotoinitiatorField> {
        self.photoinitiator_field.as_mut()
    }

    /// Install voxel cure + photoinitiator fields onto the aggregate.
    /// Both must be set together (their dimensions must match) — passing
    /// only one would break the t2f1 invariant that the two fields share
    /// shape. Idempotent: overwrites any previously-set fields.
    #[cfg(feature = "field-sim")]
    pub fn set_voxel_fields(
        &mut self,
        cure: CureField,
        photoinitiator: PhotoinitiatorField,
    ) -> Result<(), AggregateError> {
        if cure.dimensions() != photoinitiator.dimensions() {
            return Err(AggregateError::VoxelFieldDimensionMismatch {
                cure_dims: cure.dimensions(),
                pi_dims: photoinitiator.dimensions(),
            });
        }
        self.cure_field = Some(cure);
        self.photoinitiator_field = Some(photoinitiator);
        Ok(())
    }

    /// Per-voxel shrinkage strain field (ADR-0018 / t2f3).
    #[cfg(feature = "field-sim")]
    pub fn strain_field(&self) -> Option<&StrainField> {
        self.strain_field.as_ref()
    }

    /// Per-voxel residual stress field (ADR-0018 / t2f3).
    #[cfg(feature = "field-sim")]
    pub fn stress_field(&self) -> Option<&StressField> {
        self.stress_field.as_ref()
    }

    /// Mutable borrow of the strain field. Symmetric with
    /// [`Self::cure_field_mut`] — only used by SimulationRunner.
    #[cfg(feature = "field-sim")]
    pub fn strain_field_mut(&mut self) -> Option<&mut StrainField> {
        self.strain_field.as_mut()
    }

    /// Mutable borrow of the stress field. Symmetric with
    /// [`Self::strain_field_mut`].
    #[cfg(feature = "field-sim")]
    pub fn stress_field_mut(&mut self) -> Option<&mut StressField> {
        self.stress_field.as_mut()
    }

    /// Vat-envelope 3D thermal field (ADR-0020 / t2f4). Populated when
    /// `--voxel-cure-mm` is set (auto-activation rule); `None` for
    /// Tier-1 scalar runs.
    #[cfg(feature = "field-sim")]
    pub fn thermal_field(&self) -> Option<&ThermalField> {
        self.thermal_field.as_ref()
    }

    /// Mutable borrow of the thermal field (solver entry point). Used by
    /// `SimulationRunner::apply_voxel_thermal_for_layer`.
    #[cfg(feature = "field-sim")]
    pub fn thermal_field_mut(&mut self) -> Option<&mut ThermalField> {
        self.thermal_field.as_mut()
    }

    /// Install the thermal field onto the aggregate. Called once at the
    /// start of a Tier-2 voxel-cure run (`apply_voxel_thermal_for_layer`
    /// allocates from `PrinterProfile.build_envelope_mm` + resin
    /// material props, initialises to uniform ambient, then steps it
    /// every layer).
    #[cfg(feature = "field-sim")]
    pub fn set_thermal_field(&mut self, field: ThermalField) {
        self.thermal_field = Some(field);
    }

    /// Install strain + stress fields onto the aggregate (ADR-0018 / t2f3).
    /// Parallel to `set_voxel_fields(cure, pi)` — preserves the existing
    /// 2-arg setter signature unchanged. Both must be set together; their
    /// dimensions must match each other AND the previously-installed
    /// `cure_field`. Returns
    /// `Err(AggregateError::StrainStressWithoutCureField)` if no
    /// cure_field is yet set (the t2f3 invariant: strain is downstream
    /// of cure).
    #[cfg(feature = "field-sim")]
    pub fn set_strain_stress_fields(
        &mut self,
        strain: StrainField,
        stress: StressField,
    ) -> Result<(), AggregateError> {
        let cure_dims = self
            .cure_field
            .as_ref()
            .map(|c| c.dimensions())
            .ok_or(AggregateError::StrainStressWithoutCureField)?;
        if strain.dimensions() != stress.dimensions() || strain.dimensions() != cure_dims {
            return Err(AggregateError::StrainStressFieldDimensionMismatch {
                strain_dims: strain.dimensions(),
                stress_dims: stress.dimensions(),
                cure_dims: Some(cure_dims),
            });
        }
        self.strain_field = Some(strain);
        self.stress_field = Some(stress);
        Ok(())
    }

    /// Per-layer cure-depth summary derived from the resin profile, with
    /// Ec(T) Arrhenius correction applied automatically using the layer's
    /// cached `vat_temperature_c`. AR-level composition: navigates to the
    /// requested layer, computes `Ec(T)` via
    /// [`CureCalculator::ec_at_temp`](crate::services::CureCalculator::ec_at_temp),
    /// then delegates to the lower-level
    /// [`LayerResult::cure_depth_um_summary`] primitive.
    ///
    /// Returns `None` when `layer_index` is out of bounds. In Tier-1 mode
    /// (no voxel cure field), short-circuits the Ec(T) compute and returns
    /// the cached `cure_depth_um` directly — this matches the underlying
    /// primitive's cache-fallback contract and avoids wasted Arrhenius
    /// arithmetic when the voxel path is inactive.
    ///
    /// See also:
    /// - KB-153 (Ec(T) Arrhenius source-of-truth)
    /// - ADR-0017 §3 (voxel cure architecture)
    /// - ADR-0007 (vat-temperature population contract in `SimulationRunner`)
    /// - `docs/patterns/single-source-arrhenius-helper.md`
    #[cfg(feature = "field-sim")]
    pub fn cure_depth_summary_for_resin(
        &self,
        layer_index: u32,
        resin: &ResinProfile,
    ) -> Option<CureDepth> {
        let layer = self.layers.get(layer_index as usize)?;
        // Tier-1 short-circuit: no voxel field ⇒ the primitive returns the
        // cached scalar anyway; skip the Ec(T) compute to save wasted work.
        if self.cure_field.is_none() {
            return CureDepth::new(layer.cure_depth_um).ok();
        }
        let ec_t = Self::ec_t_for_layer(layer, resin);
        Some(layer.cure_depth_um_summary(self, resin.penetration_depth_um(), ec_t.value()))
    }

    /// Worst-case (most-undercured) cure depth for a specific layer derived
    /// from the resin profile, with the same Ec(T) Arrhenius treatment as
    /// [`Self::cure_depth_summary_for_resin`]. Symmetric API.
    ///
    /// Returns `None` when `layer_index` is out of bounds. Tier-1 mode
    /// short-circuits to the cached `worst_cure_depth_um`.
    #[cfg(feature = "field-sim")]
    pub fn worst_cure_depth_summary_for_resin(
        &self,
        layer_index: u32,
        resin: &ResinProfile,
    ) -> Option<CureDepth> {
        let layer = self.layers.get(layer_index as usize)?;
        if self.cure_field.is_none() {
            return CureDepth::new(layer.worst_cure_depth_um).ok();
        }
        let ec_t = Self::ec_t_for_layer(layer, resin);
        Some(layer.worst_cure_depth_um_summary(self, resin.penetration_depth_um(), ec_t.value()))
    }

    /// Layer-height reconciliation between the CTB (file-axis, runtime
    /// authority) and the resin recipe (recipe-axis, authoring metadata).
    /// `None` when the simulation came in via STL / area-only entry points
    /// (no CTB-derived value exists). `Some` for CTB-based runs;
    /// `.has_mismatch()` flags whether the user should be warned.
    pub fn layer_height_provenance(&self) -> Option<&LayerHeightProvenance> {
        self.layer_height_provenance.as_ref()
    }

    /// Install the layer-height reconciliation onto the aggregate. Called
    /// by `SimulationRunner` after `prepare_layer_inputs` has produced the
    /// provenance; STL / area-only paths skip this call.
    pub(crate) fn set_layer_height_provenance(&mut self, provenance: LayerHeightProvenance) {
        self.layer_height_provenance = Some(provenance);
    }

    pub fn critical_failures(&self) -> Vec<&FailureEvent> {
        self.failures
            .iter()
            .filter(|f| f.severity == Severity::Critical)
            .collect()
    }

    /// Re-check aggregate-level invariants on a deserialized aggregate.
    ///
    /// `#[derive(Deserialize)]` reconstructs `PrintSimulation` directly from
    /// fields and bypasses both `Recipe::validate()` / `PrinterProfile::validate()`
    /// on the child entities AND the `add_layer` constructor invariant
    /// ("layers must be sequential"). `SimulationRepository::load` calls this
    /// after `serde_json::from_str` so a tampered or schema-evolved file
    /// cannot silently violate aggregate invariants.
    ///
    /// Both paths now reject contiguity violations, with intentionally
    /// different return shapes: `add_layer` returns
    /// `Err(AggregateError::NonContiguousLayer)` on the live-mutation path,
    /// while `validate()` returns `Err(String)` on the deserialize-bypass
    /// path. The string return matches the freeform-text shape of the
    /// other deserialize-bypass guards in this method (recipe/printer
    /// child validate); converging the two paths is out of scope here.
    ///
    /// Returns `Err` on first failure with a message identifying the
    /// violation. See ADR-0009.
    pub fn validate(&self) -> Result<(), String> {
        self.recipe.validate().map_err(|e| format!("recipe: {e}"))?;
        self.printer
            .validate()
            .map_err(|e| format!("printer: {e}"))?;
        for (i, layer) in self.layers.iter().enumerate() {
            let expected = i as u32;
            if layer.index != expected {
                return Err(format!(
                    "layer index mismatch at position {i}: expected {expected}, got {}",
                    layer.index
                ));
            }
        }
        Ok(())
    }

    /// Cumulative wall-clock time (seconds) at the end of each layer.
    ///
    /// Returned `Vec<f32>` is indexed parallel to `self.layers()`: entry
    /// `i` is the cumulative print time once layer `i` has finished
    /// curing + lifting. Length always equals `self.layers().len()`,
    /// monotonic non-decreasing, all-zero on the empty aggregate.
    ///
    /// Narrow accessor for downstream consumers (e.g. resinsim-viz plot
    /// panels) that need a per-layer time axis without taking on a
    /// `Recipe + PrinterProfile` parameter pair. Delegates to
    /// `LayerTimingCalculator::cumulative_times_sec` against the
    /// aggregate's owned recipe + printer; encapsulation preserved
    /// (recipe / printer fields stay private).
    pub fn cumulative_times_sec(&self) -> Vec<f32> {
        LayerTimingCalculator::cumulative_times_sec(
            &self.recipe,
            &self.printer,
            self.layers.len() as u32,
        )
    }

    /// Compute summary statistics, including per-phase print duration.
    ///
    /// Reads `self.recipe + self.printer` (set at construction) to project
    /// phase times via `LayerTimingCalculator`. Short-print clamp semantics
    /// cover every layer-count regime (see module tests): when
    /// `total_layers` is zero, all time fields are zero; when it falls
    /// inside the bottom phase, transition + normal are zero; etc.
    pub fn summary(&self) -> SimSummary {
        let total_layers = self.layers.len() as u32;
        let (total_time_sec, bottom_time_sec, transition_time_sec, normal_time_sec) =
            self.phase_times(total_layers);

        if self.layers.is_empty() {
            return SimSummary {
                total_layers: 0,
                critical_failures: 0,
                warnings: 0,
                max_peel_force_n: 0.0,
                max_force_layer: 0,
                min_safety_factor: f32::INFINITY,
                min_safety_layer: 0,
                max_temperature_c: 0.0,
                max_z_deflection_um: 0.0,
                total_time_sec,
                bottom_time_sec,
                transition_time_sec,
                normal_time_sec,
            };
        }

        let mut max_force = f32::NEG_INFINITY;
        let mut max_force_layer = 0u32;
        let mut min_sf = f32::INFINITY;
        let mut min_sf_layer = 0u32;
        let mut max_temp = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;

        for lr in &self.layers {
            if lr.total_force_n > max_force {
                max_force = lr.total_force_n;
                max_force_layer = lr.index;
            }
            if lr.safety_factor < min_sf {
                min_sf = lr.safety_factor;
                min_sf_layer = lr.index;
            }
            if lr.vat_temperature_c > max_temp {
                max_temp = lr.vat_temperature_c;
            }
            if lr.z_deflection_um > max_z {
                max_z = lr.z_deflection_um;
            }
        }

        SimSummary {
            total_layers: self.layers.len() as u32,
            critical_failures: self
                .failures
                .iter()
                .filter(|f| f.severity == Severity::Critical)
                .count(),
            warnings: self
                .failures
                .iter()
                .filter(|f| f.severity == Severity::Warning)
                .count(),
            max_peel_force_n: max_force,
            max_force_layer,
            min_safety_factor: min_sf,
            min_safety_layer: min_sf_layer,
            max_temperature_c: max_temp,
            max_z_deflection_um: max_z,
            total_time_sec,
            bottom_time_sec,
            transition_time_sec,
            normal_time_sec,
        }
    }

    // Shared Ec(T) compose for the for_resin methods. Pulls the cached vat
    // temperature off the layer (populated by SimulationRunner per ADR-0007)
    // and composes the resin's reference Ec under Arrhenius correction.
    // .expect-justifications cite the upstream invariants per ADR-0003.
    #[cfg(feature = "field-sim")]
    fn ec_t_for_layer(layer: &LayerResult, resin: &ResinProfile) -> Energy {
        let ec_ref = Energy::new(resin.critical_energy_mj_cm2())
            .expect("ResinProfile::validate() guarantees critical_energy_mj_cm2 > 0");
        let vat = VatTemperature::new(layer.vat_temperature_c).expect(
            "LayerResult.vat_temperature_c populated from VatTemperature::value() in SimulationRunner per ADR-0007",
        );
        CureCalculator::ec_at_temp(
            ec_ref,
            resin.reference_temp_c(),
            vat,
            resin.effective_cure_kinetics_ea_kj_mol(),
        )
    }

    // (total, bottom, transition, normal) in seconds.
    // Explicit clamps cover every length regime: n=0 ⇒ all zero;
    // n within bottom phase ⇒ transition+normal zero; n in transition ⇒ normal zero.
    fn phase_times(&self, total_layers: u32) -> (f32, f32, f32, f32) {
        if total_layers == 0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let cumulative =
            LayerTimingCalculator::cumulative_times_sec(&self.recipe, &self.printer, total_layers);
        let bottom_count = self.recipe.bottom_layer_count();
        let transition_count = self.recipe.transition_layers();
        let bottom_n = bottom_count.min(total_layers) as usize;
        let trans_end = (bottom_count.saturating_add(transition_count)).min(total_layers) as usize;
        let n = total_layers as usize;

        let bottom_t = if bottom_n == 0 {
            0.0
        } else {
            cumulative[bottom_n - 1]
        };
        let transition_t = if trans_end <= bottom_n {
            0.0
        } else {
            cumulative[trans_end - 1] - bottom_t
        };
        let normal_t = if n <= trans_end {
            0.0
        } else {
            cumulative[n - 1] - cumulative[trans_end - 1]
        };
        let total_t = cumulative[n - 1];
        (total_t, bottom_t, transition_t, normal_t)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::entities::{LayerResult, PrinterProfile, Recipe, ResinProfile};

    /// Shared fixture: factory-default Recipe (generic_standard resin).
    /// `pub(crate)` so report_generator.rs test mod can reuse.
    pub(crate) fn default_recipe() -> Recipe {
        ResinProfile::generic_standard().recipe().clone()
    }

    /// Shared fixture: factory-default Linear PrinterProfile (generic_msla_4k).
    /// `pub(crate)` so report_generator.rs test mod can reuse.
    pub(crate) fn linear_printer() -> PrinterProfile {
        PrinterProfile::generic_msla_4k()
    }

    /// Shared fixture: synthetic LayerResult for tests that don't need
    /// physics realism. `pub(crate)` so other in-crate test modules
    /// (simulation_repo.rs) reuse it.
    pub(crate) fn make_layer(index: u32, force: f32, sf: f32, temp: f32) -> LayerResult {
        LayerResult {
            index,
            cure_depth_um: 100.0,
            peel_force_n: force,
            suction_force_n: 0.0,
            total_force_n: force,
            support_capacity_n: force * sf,
            safety_factor: sf,
            cross_section_area_mm2: 100.0,
            area_delta_mm2: 0.0,
            vat_temperature_c: temp,
            viscosity_mpa_s: 200.0,
            z_deflection_um: force / 0.46, // k=460
            effective_layer_height_um: 50.0 - force / 0.46,
            worst_cure_depth_um: 100.0,
            strain_magnitude_max: None,
            stress_von_mises_max_mpa: None,
            strain_gradient_max_frac: None,
            voxel_yield_fraction: None,
        }
    }

    #[test]
    fn sequential_layers_accepted() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        sim.add_layer(make_layer(1, 6.0, 2.5, 22.5), vec![])
            .expect("test fixture: explicit index 1 matches layer count 1 at this call site");
        assert_eq!(sim.layers().len(), 2);
    }

    #[test]
    fn non_sequential_layer_returns_err() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]).expect(
            "test fixture: index 0 satisfies add_layer's contiguity precondition on an empty sim",
        );
        let err = sim
            .add_layer(make_layer(5, 6.0, 2.5, 22.5), vec![])
            .expect_err("non-contiguous index 5 (expected 1) must return Err");
        assert_eq!(
            err,
            AggregateError::NonContiguousLayer {
                expected: 1,
                got: 5,
            }
        );
    }

    #[test]
    fn summary_finds_extremes() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        sim.add_layer(
            make_layer(1, 20.0, 0.8, 25.0),
            vec![FailureEvent {
                layer: 1,
                failure_type: crate::entities::FailureType::SupportOverload,
                severity: Severity::Critical,
                message: "test".into(),
            }],
        )
        .expect("test fixture: explicit index 1 matches layer count 1 at this call site");
        sim.add_layer(make_layer(2, 10.0, 2.0, 24.0), vec![])
            .expect("test fixture: explicit index 2 matches layer count 2 at this call site");

        let s = sim.summary();
        assert_eq!(s.total_layers, 3);
        assert_eq!(s.max_force_layer, 1);
        assert!((s.max_peel_force_n - 20.0).abs() < 1e-6);
        assert_eq!(s.min_safety_layer, 1);
        assert!((s.min_safety_factor - 0.8).abs() < 1e-6);
        assert!((s.max_temperature_c - 25.0).abs() < 1e-6);
        assert_eq!(s.critical_failures, 1);
    }

    // SimSummary per-phase time field tests — ported from the orphan
    // (jj:rnvrvtprzxmt `feat(print-time-report)`). In v4 the aggregate owns
    // recipe+printer, so summary() is arg-less — tests construct
    // PrintSimulation::new(recipe, printer) up-front.

    #[test]
    fn summary_includes_total_time() {
        // 100-layer Linear + generic_standard run: expected total equals
        // LayerTimingCalculator::cumulative_times_sec(...).last().
        let recipe = default_recipe();
        let printer = linear_printer();
        let mut sim = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..100 {
            sim.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential index i in 0..100 satisfies add_layer's contiguity precondition");
        }
        let s = sim.summary();
        let expected = *LayerTimingCalculator::cumulative_times_sec(&recipe, &printer, 100)
            .last()
            .expect("100 layers produces a non-empty cumulative vector");
        assert!(
            (s.total_time_sec - expected).abs() < 1e-4,
            "total_time_sec should match calculator.last(): got {}, expected {expected}",
            s.total_time_sec,
        );
        assert!(
            s.total_time_sec > 0.0,
            "non-empty run must have positive total time"
        );
    }

    #[test]
    fn summary_per_phase_adds_to_total() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        for i in 0..100 {
            sim.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential index i in 0..100 satisfies add_layer's contiguity precondition");
        }
        let s = sim.summary();
        let sum = s.bottom_time_sec + s.transition_time_sec + s.normal_time_sec;
        let tol = (s.total_time_sec.abs() * 1e-3).max(1e-6);
        assert!(
            (sum - s.total_time_sec).abs() < tol,
            "phase sum {sum} should equal total {} within {tol}",
            s.total_time_sec,
        );
    }

    #[test]
    fn summary_single_bottom_layer() {
        // n=1 with default recipe (bottom_count=6) — firmly in the bottom phase.
        // Exercises cumulative[0] indexing + the bottom_n=min(6,1)=1 clamp branch.
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 1.0, 3.0, 22.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        let s = sim.summary();
        assert!(s.bottom_time_sec > 0.0);
        assert_eq!(s.transition_time_sec, 0.0);
        assert_eq!(s.normal_time_sec, 0.0);
        assert!((s.bottom_time_sec - s.total_time_sec).abs() < 1e-4);
    }

    #[test]
    fn summary_short_print_clamps() {
        let recipe = default_recipe();
        let printer = linear_printer();

        // total_layers == 0: all time fields zero, no panic.
        let empty = PrintSimulation::new(recipe.clone(), printer.clone()).summary();
        assert_eq!(empty.total_layers, 0);
        assert_eq!(empty.total_time_sec, 0.0);
        assert_eq!(empty.bottom_time_sec, 0.0);
        assert_eq!(empty.transition_time_sec, 0.0);
        assert_eq!(empty.normal_time_sec, 0.0);

        // total_layers == bottom_count - 1: only bottom_time_sec non-zero.
        let bottom_only_n = recipe.bottom_layer_count() - 1;
        let mut sim_b = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..bottom_only_n {
            sim_b.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential index i in 0..bottom_only_n satisfies add_layer's contiguity precondition");
        }
        let b = sim_b.summary();
        assert!(b.bottom_time_sec > 0.0);
        assert_eq!(b.transition_time_sec, 0.0);
        assert_eq!(b.normal_time_sec, 0.0);
        assert!((b.bottom_time_sec - b.total_time_sec).abs() < 1e-4);

        // total_layers == bottom_count + transition_layers - 1: normal stays zero.
        let mid_n = recipe.bottom_layer_count() + recipe.transition_layers() - 1;
        let mut sim_t = PrintSimulation::new(recipe.clone(), printer.clone());
        for i in 0..mid_n {
            sim_t.add_layer(make_layer(i, 1.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential index i in 0..mid_n satisfies add_layer's contiguity precondition");
        }
        let t = sim_t.summary();
        assert!(t.bottom_time_sec > 0.0);
        assert!(t.transition_time_sec > 0.0);
        assert_eq!(t.normal_time_sec, 0.0);
    }

    // --- validate() — aggregate-level deserialize-bypass guard (ADR-0009) ---

    fn build_three_layer_sim() -> PrintSimulation {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        sim.add_layer(make_layer(1, 6.0, 2.5, 22.5), vec![])
            .expect("test fixture: explicit index 1 matches layer count 1 at this call site");
        sim.add_layer(make_layer(2, 7.0, 2.0, 23.0), vec![])
            .expect("test fixture: explicit index 2 matches layer count 2 at this call site");
        sim
    }

    #[test]
    fn validate_ok_for_well_formed_aggregate() {
        let sim = build_three_layer_sim();
        sim.validate().expect("well-formed aggregate must validate");
    }

    #[test]
    fn validate_returns_err_when_recipe_invalid() {
        let sim = build_three_layer_sim();
        let mut value = serde_json::to_value(&sim).expect("PrintSimulation must serialize to JSON");
        value["recipe"]["layer_height_um"] = serde_json::json!(-1.0);
        let tampered: PrintSimulation =
            serde_json::from_value(value).expect("tampered JSON must still deserialize");
        let err = tampered
            .validate()
            .expect_err("invalid recipe must fail validate()");
        assert!(
            err.contains("recipe") && err.contains("layer_height_um"),
            "error must identify the recipe field; got: {err}"
        );
    }

    #[test]
    fn validate_returns_err_when_printer_invalid() {
        let sim = build_three_layer_sim();
        let mut value = serde_json::to_value(&sim).expect("PrintSimulation must serialize to JSON");
        value["printer"]["name"] = serde_json::json!("");
        let tampered: PrintSimulation =
            serde_json::from_value(value).expect("tampered JSON must still deserialize");
        let err = tampered
            .validate()
            .expect_err("invalid printer must fail validate()");
        assert!(
            err.contains("printer") && err.contains("name"),
            "error must identify the printer field; got: {err}"
        );
    }

    #[test]
    fn validate_returns_err_when_layer_indices_non_sequential() {
        let sim = build_three_layer_sim();
        let mut value = serde_json::to_value(&sim).expect("PrintSimulation must serialize to JSON");
        // Skip index 1 — layers now read [0, 5, 2], with position 1 violating.
        value["layers"][1]["index"] = serde_json::json!(5);
        let tampered: PrintSimulation =
            serde_json::from_value(value).expect("tampered JSON must still deserialize");
        let err = tampered
            .validate()
            .expect_err("non-sequential layer indices must fail validate()");
        assert!(
            err.contains("layer index mismatch at position 1"),
            "error must point at the first non-sequential position; got: {err}"
        );
    }

    #[test]
    fn cumulative_times_sec_length_matches_layers() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        for i in 0..7 {
            sim.add_layer(make_layer(i, 5.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential indices satisfy the contiguity precondition");
        }
        assert_eq!(sim.cumulative_times_sec().len(), 7);
    }

    #[test]
    fn cumulative_times_sec_empty_when_no_layers() {
        let sim = PrintSimulation::new(default_recipe(), linear_printer());
        assert!(sim.cumulative_times_sec().is_empty());
    }

    #[test]
    fn cumulative_times_sec_is_monotonic_non_decreasing() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        for i in 0..50 {
            sim.add_layer(make_layer(i, 5.0, 3.0, 22.0), vec![])
                .expect("test fixture: sequential indices satisfy the contiguity precondition");
        }
        let times = sim.cumulative_times_sec();
        for i in 1..times.len() {
            assert!(
                times[i] >= times[i - 1],
                "cumulative time must be non-decreasing at index {i}: {} vs {}",
                times[i - 1],
                times[i]
            );
        }
    }

    // --- ADR-0017 / t2f1 voxel-field aggregate-membership tests ---

    #[cfg(feature = "field-sim")]
    #[test]
    fn voxel_fields_absent_by_default() {
        let sim = PrintSimulation::new(default_recipe(), linear_printer());
        assert!(sim.cure_field().is_none());
        assert!(sim.photoinitiator_field().is_none());
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn set_voxel_fields_installs_both() {
        use crate::values::{CureField, PhotoinitiatorField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        let cure = CureField::new(4, 4, 4, 0.2, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi = PhotoinitiatorField::new(4, 4, 4, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        sim.set_voxel_fields(cure, pi).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        assert!(sim.cure_field().is_some());
        assert!(sim.photoinitiator_field().is_some());
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn set_voxel_fields_rejects_dimension_mismatch() {
        use crate::values::{CureField, PhotoinitiatorField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        let cure = CureField::new(4, 4, 4, 0.2, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi = PhotoinitiatorField::new(4, 4, 5, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let err = sim.set_voxel_fields(cure, pi).expect_err("test fixture: dimension mismatch deliberately injected, so Err is the expected outcome");
        // Destructured payload — see docs/patterns/anti/bare-matches-as-test-assertion.md
        assert!(
            matches!(
                err,
                AggregateError::VoxelFieldDimensionMismatch {
                    cure_dims: (4, 4, 4),
                    pi_dims: (4, 4, 5),
                }
            ),
            "expected VoxelFieldDimensionMismatch with destructured payload, got {err:?}"
        );
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn set_voxel_fields_overwrites_previous() {
        use crate::values::{CureField, PhotoinitiatorField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        let cure_a = CureField::new(4, 4, 4, 0.2, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi_a = PhotoinitiatorField::new(4, 4, 4, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        sim.set_voxel_fields(cure_a, pi_a).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let (nx_a, _, _) = sim.cure_field().expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)").dimensions();
        assert_eq!(nx_a, 4);

        let cure_b = CureField::new(8, 8, 8, 0.1, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi_b = PhotoinitiatorField::new(8, 8, 8, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        sim.set_voxel_fields(cure_b, pi_b).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let (nx_b, _, _) = sim.cure_field().expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)").dimensions();
        assert_eq!(nx_b, 8);
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn voxel_fields_mut_borrow_works() {
        use crate::values::{CureField, PhotoinitiatorField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        let cure = CureField::new(2, 2, 2, 0.2, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi = PhotoinitiatorField::new(2, 2, 2, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        sim.set_voxel_fields(cure, pi).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        // Mutate through the &mut accessor — SimulationRunner's usage shape.
        sim.cure_field_mut().expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)").add_dose(0, 0, 0, 5.0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        assert_eq!(sim.cure_field().expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)").dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)"), 5.0);
        sim.photoinitiator_field_mut()
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)")
            .deplete(0, 0, 0, 0.05, 5.0)
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let c = sim.photoinitiator_field().expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)").concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        assert!(c < 1.0 && c > 0.0);
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn voxel_fields_skip_serializing_when_none() {
        // Aggregate without voxel fields ⇒ JSON output does NOT contain
        // `cure_field` or `photoinitiator_field` keys, preserving the
        // shape for Tier-1 sim.json consumers.
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![]).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let json = serde_json::to_string(&sim).expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        assert!(
            !json.contains("cure_field"),
            "Tier-1 sim.json must omit cure_field when None; got: {json}"
        );
        assert!(
            !json.contains("photoinitiator_field"),
            "Tier-1 sim.json must omit photoinitiator_field when None"
        );
    }

    // --- *_summary_for_resin convenience methods (t2f1.5 F1) ---

    #[cfg(feature = "field-sim")]
    #[test]
    fn cure_depth_summary_for_resin_out_of_bounds_returns_none() {
        // AR query for a layer index past the end must return None — no
        // panic, no .expect on a missing slot. Witness for the
        // Option<CureDepth> return contract.
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        sim.add_layer(make_layer(0, 5.0, 3.0, 22.0), vec![])
            .expect("test fixture: index 0 satisfies add_layer contiguity on an empty sim");
        let resin = ResinProfile::generic_standard();
        assert!(sim.cure_depth_summary_for_resin(1, &resin).is_none());
        assert!(sim.cure_depth_summary_for_resin(99, &resin).is_none());
        assert!(sim.worst_cure_depth_summary_for_resin(1, &resin).is_none());
    }

    /// Tier-1 short-circuit witness. We deliberately set
    /// `vat_temperature_c = f32::NAN` on the fixture LayerResult — if the
    /// AR method DIDN'T short-circuit before computing Ec(T), the
    /// `VatTemperature::new(NAN).expect(...)` inside `ec_t_for_layer` would
    /// panic. The fact that this test returns Some(cached) proves the
    /// `if self.cure_field.is_none()` short-circuit ran first, matching
    /// the lower-level primitive's cache-fallback cost profile.
    #[cfg(feature = "field-sim")]
    #[test]
    fn cure_depth_summary_for_resin_tier1_skips_ec_t_compute() {
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        let mut layer = make_layer(0, 5.0, 3.0, 22.0);
        layer.cure_depth_um = 87.5; // distinctive non-default cached value
        layer.vat_temperature_c = f32::NAN; // poison — would panic Ec(T) compute
        layer.worst_cure_depth_um = 73.25;
        sim.add_layer(layer, vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        let resin = ResinProfile::generic_standard();

        let cd = sim
            .cure_depth_summary_for_resin(0, &resin)
            .expect("Tier-1 mode must return Some(cached) for in-bounds layer");
        assert_eq!(
            cd.value(),
            87.5,
            "Tier-1 path must return the cached cure_depth_um verbatim"
        );

        let worst = sim
            .worst_cure_depth_summary_for_resin(0, &resin)
            .expect("Tier-1 mode must return Some(cached) for in-bounds layer");
        assert_eq!(
            worst.value(),
            73.25,
            "Tier-1 path must return the cached worst_cure_depth_um verbatim"
        );
    }

    /// Voxel-mode contract: the AR convenience method composes
    /// `CureCalculator::ec_at_temp` with the cached `vat_temperature_c` and
    /// delegates to the existing `LayerResult::cure_depth_um_summary`
    /// primitive. This test proves byte-identical agreement with the
    /// manual Ec(T) compose chain — the smoking gun that the v1 plan
    /// (placement on LayerResult) was trying to resolve.
    #[cfg(feature = "field-sim")]
    #[test]
    fn cure_depth_summary_for_resin_matches_manual_ec_t_chain() {
        use crate::services::CureCalculator;
        use crate::values::{CureField, Energy, PhotoinitiatorField, VatTemperature};

        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        // Single-layer fixture at a representative non-reference temperature
        // so Ec(T) actually drifts from Ec_ref (35 °C plateau vs 25 °C ref).
        let mut layer = make_layer(0, 5.0, 3.0, 35.0);
        layer.cure_depth_um = 110.0; // post-runner cache, computed using Ec(T)
        sim.add_layer(layer, vec![])
            .expect("test fixture: explicit index 0 matches layer count 0 at this call site");
        // Tiny 1×1×1 cure_field with a known dose so the primitive's
        // dispatch branch (cf.layer_summary) actually runs.
        let mut cure = CureField::new(1, 1, 1, 0.05, [0.0, 0.0, 0.0])
            .expect("1×1×1 at 0.05 mm voxel + (0,0,0) bbox_min satisfies CureField::new domain");
        cure.add_dose(0, 0, 0, 25.0)
            .expect("(0,0,0) is in-bounds for the 1×1×1 field we just constructed");
        let pi = PhotoinitiatorField::new(1, 1, 1, 1.0)
            .expect("1×1×1 at C₀ = 1.0 satisfies PhotoinitiatorField::new domain");
        sim.set_voxel_fields(cure, pi)
            .expect("matching 1×1×1 dims satisfy set_voxel_fields precondition");

        let resin = ResinProfile::generic_standard();

        // Manual chain — same calls the runner makes in failure_predictor.
        let ec_ref = Energy::new(resin.critical_energy_mj_cm2()).expect(
            "ResinProfile::generic_standard() factory guarantees critical_energy_mj_cm2 > 0",
        );
        let vat = VatTemperature::new(35.0)
            .expect("35.0 °C is in the VatTemperature domain (finite, above absolute zero)");
        let ec_t = CureCalculator::ec_at_temp(
            ec_ref,
            resin.reference_temp_c(),
            vat,
            resin.effective_cure_kinetics_ea_kj_mol(),
        );
        let expected =
            sim.layers()[0].cure_depth_um_summary(&sim, resin.penetration_depth_um(), ec_t.value());

        let observed = sim
            .cure_depth_summary_for_resin(0, &resin)
            .expect("voxel-mode call on in-bounds layer 0 must yield Some");
        assert_eq!(
            observed.value().to_bits(),
            expected.value().to_bits(),
            "AR convenience method must be byte-identical to the manual Ec(T) compose chain"
        );

        let expected_worst = sim.layers()[0].worst_cure_depth_um_summary(
            &sim,
            resin.penetration_depth_um(),
            ec_t.value(),
        );
        let observed_worst = sim
            .worst_cure_depth_summary_for_resin(0, &resin)
            .expect("voxel-mode call on in-bounds layer 0 must yield Some");
        assert_eq!(
            observed_worst.value().to_bits(),
            expected_worst.value().to_bits(),
            "worst_* AR convenience method must be byte-identical to its manual chain too"
        );
    }

    // --- t2f3.1 B2: set_strain_stress_fields error-path coverage ---
    //
    // The happy path is exercised in voxel_strain_stress_integration.rs;
    // these two tests lock the two typed error variants directly so
    // future refactors of set_strain_stress_fields can't accidentally
    // make the error paths unreachable. Both use destructured matches!
    // (not bare `matches!(...)`) per
    // docs/patterns/anti/bare-matches-as-test-assertion.md to avoid
    // silent-green assertion regressions.

    #[cfg(feature = "field-sim")]
    #[test]
    fn set_strain_stress_fields_errors_without_cure_field() {
        use crate::values::{StrainField, StressField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        // No set_voxel_fields call → cure_field is None.
        let strain = StrainField::new(2, 2, 2, 0.2, [0.0, 0.0, 0.0])
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let stress = StressField::new(2, 2, 2, 0.2, [0.0, 0.0, 0.0])
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let result = sim.set_strain_stress_fields(strain, stress);
        assert!(
            matches!(result, Err(AggregateError::StrainStressWithoutCureField)),
            "expected StrainStressWithoutCureField, got {result:?}"
        );
    }

    #[cfg(feature = "field-sim")]
    #[test]
    fn set_strain_stress_fields_rejects_dimension_mismatch() {
        use crate::values::{CureField, PhotoinitiatorField, StrainField, StressField};
        let mut sim = PrintSimulation::new(default_recipe(), linear_printer());
        // Install voxel fields at (4,4,4) first so cure_field is Some.
        let cure = CureField::new(4, 4, 4, 0.2, [0.0, 0.0, 0.0])
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let pi = PhotoinitiatorField::new(4, 4, 4, 1.0)
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        sim.set_voxel_fields(cure, pi)
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        // Strain at (4,4,4) but stress at (4,4,5) — stress disagrees.
        let strain = StrainField::new(4, 4, 4, 0.2, [0.0, 0.0, 0.0])
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let stress = StressField::new(4, 4, 5, 0.2, [0.0, 0.0, 0.0])
            .expect("test fixture: literal inputs satisfy the called function's preconditions (dimension match, finite f32, validated profile)");
        let result = sim.set_strain_stress_fields(strain, stress);
        assert!(
            matches!(
                result,
                Err(AggregateError::StrainStressFieldDimensionMismatch {
                    strain_dims: (4, 4, 4),
                    stress_dims: (4, 4, 5),
                    cure_dims: Some((4, 4, 4)),
                })
            ),
            "expected StrainStressFieldDimensionMismatch with destructured payload, got {result:?}"
        );
    }
}
