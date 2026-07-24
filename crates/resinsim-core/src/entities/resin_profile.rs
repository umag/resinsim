use serde::{Deserialize, Serialize};

use crate::entities::Recipe;
use crate::values::VatTemperature;

/// Default vat-temperature degradation threshold (°C). KB-150 — most standard
/// resins begin thermal breakdown around 50 °C.
fn default_degradation_temp_c() -> f32 {
    50.0
}

/// Default minimum safe vat temperature (°C). Below this viscosity spikes and
/// peel force grows non-linearly.
fn default_min_safe_temp_c() -> f32 {
    15.0
}

/// KB-153 literature-midpoint estimate for cure-kinetics Ea. Applied when a
/// ResinProfile has `cure_kinetics_ea_kj_mol = None`; the CLI / reports must
/// emit a LOUD warning in that case so users know the cure-drift physics is
/// running on an ESTIMATE, not a measured value. Per-resin calibration data
/// should update the TOML's `cure_kinetics_ea_kj_mol` field as measurements
/// arrive.
pub const DEFAULT_CURE_KINETICS_EA_KJ_MOL: f32 = 30.0;

/// KB-160 default initial photoinitiator concentration (dimensionless
/// fraction in `[0, 1]`). Convention: `PhotoinitiatorField` starts uniform
/// at 1.0 everywhere unless the resin TOML overrides via
/// `photoinitiator_concentration_initial`.
pub const DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL: f32 = 1.0;

/// KB-160 literature-midpoint estimate for the photoinitiator decay rate
/// constant in `1 / (mJ·cm⁻² × concentration-fraction)` units. Applied when
/// a ResinProfile has `photoinitiator_decay_constant_k_d = None`; the CLI /
/// reports MUST emit a LOUD warning in that case (±50 % uncertainty band).
/// Per-resin calibration data should update the TOML's
/// `photoinitiator_decay_constant_k_d` field as measurements arrive.
pub const DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D: f32 = 0.05;

/// KB-163 literature-midpoint estimate for photopolymer Young's modulus
/// (post-cure, fully-cured). Applied when a `ResinProfile` has
/// `youngs_modulus_mpa = None`; callers (CLI, reports, strain/stress
/// failures) MUST emit a LOUD warning with the ±50 % uncertainty band so
/// users know the mechanical analysis is running on an ESTIMATE. Per-resin
/// calibration (cured-vs-green dimensions + tensile measurement) updates
/// the TOML field as measurements arrive.
pub const DEFAULT_YOUNGS_MODULUS_MPA: f32 = 2000.0;

/// KB-163 literature-midpoint estimate for photopolymer Poisson's ratio
/// (post-cure thermoset). Applied when a `ResinProfile` has
/// `poissons_ratio = None`; callers MUST emit a LOUD warning (±0.05 band).
/// Constrained to (-1.0, 0.5) by validate() — 0.5 would make the linear-
/// elasticity stiffness matrix singular (incompressible limit).
pub const DEFAULT_POISSONS_RATIO: f32 = 0.35;

/// KB-164 literature-midpoint estimate for the Z/XY shrinkage anisotropy
/// ratio in DLP / SLA photopolymers. Per-layer cure mechanics constrain
/// XY shrinkage (cured layer below holds the new layer in plane) while
/// leaving Z free to deform; the cured part therefore shrinks more in Z
/// than in XY. Direct shrinkage measurements are sparse in the literature
/// but mechanical-modulus anisotropy from `PMC5344561` (E_z/E_xy = 1.27
/// for Castable Blend, 1.39 for Visijet FTX Green, untreated) supports
/// a ratio in the 1.3–1.5 range. v1 default: 1.5 with ±0.3 band; calibrate
/// via Athena II tensile + DIC follow-on.
///
/// Mapping: with `ε_iso = -L · C` the per-axis components are
/// `ε_zz = factor_z · ε_iso`, `ε_xx = ε_yy = factor_xy · ε_iso` where
/// `factor_z / factor_xy = z_ratio` and `2·factor_xy + factor_z = 3`
/// (volume-conserving so that `linear_shrinkage_pct` keeps its
/// vendor-data-sheet meaning).
pub const DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO: f32 = 1.5;

/// serde-default returning [`DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL`].
/// Used so legacy resin TOMLs without the field parse to the convention
/// default (1.0) rather than erroring at deserialise.
fn default_photoinitiator_concentration_initial() -> f32 {
    DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL
}

/// Physical properties of a resin formulation (chemistry) + its recipe (ADR-0005, Axis 2).
/// Identity: `name`. Loaded from TOML profiles in `data/resins/`.
///
/// # Chemistry vs Recipe
///
/// **Chemistry** fields describe immutable physical properties of the resin formulation
/// (optics, mechanics, viscosity, thermal thresholds, peel-measurement metadata). They
/// change only when the formulator changes the resin.
///
/// **Recipe** (nested `Recipe` VO) describes the concrete operating point for a print
/// (exposure times, layer height, lift kinematics). It is chosen per-resin and may
/// change between tuning sessions.
///
/// `ref_lift_speed_mm_min` is chemistry, not recipe — it is measurement metadata for
/// `peel_adhesion_kpa` (KB-112 + KB-114). See ADR-0005 §3.
///
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` — external code cannot construct or mutate a
/// `ResinProfile`. Construction is restricted to the factory methods on this type and
/// to TOML deserialisation via `ResinProfileRepository`, both of which run
/// `validate()` before returning. After any field mutation by intra-crate code
/// (typically tests), `validate()` MUST be re-called before treating the profile as
/// trusted by downstream services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResinProfile {
    pub(crate) name: String,

    // Optical (Beer-Lambert)
    /// Penetration depth at 405nm. Unit: µm. KB-100, KB-101.
    pub(crate) penetration_depth_um: f32,
    /// Critical energy at 405nm. Unit: mJ/cm². KB-100, KB-101.
    pub(crate) critical_energy_mj_cm2: f32,

    // Mechanical
    /// Tensile strength (post-cure). Unit: MPa. KB-140.
    pub(crate) tensile_strength_mpa: f32,
    /// Peel adhesion to FEP. Unit: kPa. KB-110.
    pub(crate) peel_adhesion_kpa: f32,
    /// Reference speed at which `peel_adhesion_kpa` was measured. Unit: mm/min.
    /// Chemistry metadata — see KB-112, KB-114, ADR-0005 §3. Moved from
    /// `PrinterProfile` in the three-axis refactor: the peel-force model scales
    /// `peel_adhesion_kpa` by `f_resin(v_lift) / f_resin(v_ref)`, so `v_ref`
    /// travels with the adhesion measurement it was taken under.
    pub(crate) ref_lift_speed_mm_min: f32,

    // Shrinkage
    /// Linear shrinkage. Unit: %. KB-142.
    pub(crate) linear_shrinkage_pct: f32,

    // Thermal/Viscosity
    /// Viscosity at reference temperature. Unit: mPa·s. KB-141.
    pub(crate) viscosity_mpa_s: f32,
    /// Reference temperature for viscosity. Unit: °C.
    pub(crate) reference_temp_c: f32,
    /// Arrhenius activation energy. Unit: kJ/mol. KB-141.
    pub(crate) activation_energy_kj_mol: f32,

    /// Density. Unit: g/cm³.
    pub(crate) density_g_cm3: f32,

    /// Temperature above which this resin begins thermal degradation. Unit: °C.
    /// KB-150. Default 50 °C for typical standard resins.
    #[serde(default = "default_degradation_temp_c")]
    pub(crate) degradation_temp_c: f32,
    /// Temperature below which viscosity spike causes peel/suction problems. Unit: °C.
    /// Default 15 °C for typical standard resins.
    #[serde(default = "default_min_safe_temp_c")]
    pub(crate) min_safe_temp_c: f32,

    /// Cure-kinetics Arrhenius activation energy. Unit: kJ/mol. KB-153.
    /// **Optional** — when `None`, the Ec(T) correction uses
    /// [`DEFAULT_CURE_KINETICS_EA_KJ_MOL`] (30 kJ/mol, literature midpoint for
    /// radical photopolymerization) and callers (CLI, reports) SHOULD emit a
    /// loud warning that the cure-drift physics is running on an ESTIMATE, not
    /// a measured value. Per-resin calibration data should replace the default
    /// as measurements become available.
    #[serde(default)]
    pub(crate) cure_kinetics_ea_kj_mol: Option<f32>,

    /// First-layer base-adhesion σ-elevation (KB-116 oxygen-freshness). Unit:
    /// kPa. **Optional** — when `None`, `effective_base_adhesion_elevation_kpa`
    /// returns 0.0 (NO base term), so unset/legacy resins are behaviour-
    /// preserving. A non-zero value adds an elevated release-layer adhesion at
    /// layer 0 that relaxes over ~`recipe.bottom_layer_count()` layers
    /// (ADR-0022 Stage 1). Indicative until fitted against calibration data
    /// (single-print R²≈0); mirrors the `cure_kinetics_ea_kj_mol` Option-with-
    /// warn precedent.
    #[serde(default)]
    pub(crate) base_adhesion_elevation_kpa: Option<f32>,

    /// Aspect-ratio peel shape-factor strength (KB-185 Tier-1, ADR-0022
    /// Stage 3). Dimensionless in `[0, 1]`. **Optional** — when `None`,
    /// `effective_peel_shape_factor_strength` returns `0.0` (shape factor ≡ 1.0,
    /// NO correction), so unset/legacy resins are behaviour-preserving. A
    /// non-zero value modulates σ_peel by the layer's compactness
    /// (`PeelForceCalculator::peel_shape_factor`); `0.5` reproduces the Pan
    /// Fig.9 cylinder→star force ratio (≈0.795). Indicative until fitted against
    /// an equal-area shape sweep (ADR-0022 defers per-stratum fitting to the
    /// E-series); mirrors the `base_adhesion_elevation_kpa` opt-in precedent.
    #[serde(default)]
    pub(crate) peel_shape_factor_strength: Option<f32>,

    /// Initial photoinitiator concentration as a dimensionless fraction in
    /// `[0, 1]`. KB-160. Default 1.0 (convention) via `#[serde(default = ...)]`
    /// so legacy resin TOMLs without the field parse unchanged.
    /// Consumed by the t2f1 voxel cure path
    /// (`VoxelCureCalculator`) when the runtime `--voxel-cure-mm` flag is
    /// active; the Tier-1 scalar path does not read this field.
    #[serde(default = "default_photoinitiator_concentration_initial")]
    pub(crate) photoinitiator_concentration_initial: f32,

    /// Photoinitiator decay rate constant, in
    /// `1 / (mJ·cm⁻² × concentration-fraction)` units. KB-160.
    /// **Optional** — when `None`, the voxel cure path uses
    /// [`DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D`] (0.05 literature midpoint
    /// with ±50 % uncertainty) and the CLI / reports MUST emit a loud
    /// warning. Per-resin calibration replaces the default. Mirrors the
    /// `cure_kinetics_ea_kj_mol` Option-with-warn precedent.
    #[serde(default)]
    pub(crate) photoinitiator_decay_constant_k_d: Option<f32>,

    /// Young's modulus (linear-elastic stiffness). Unit: MPa. KB-163.
    /// **Optional** — when `None`, the t2f3 strain/stress path uses
    /// [`DEFAULT_YOUNGS_MODULUS_MPA`] (2000 MPa literature midpoint with
    /// ±50 % uncertainty) and `FailurePredictor::predict_strain_failures`
    /// MUST disclose the uncalibrated-moduli caveat in any emitted
    /// `FailureEvent.message`. Per-resin calibration via measured tensile
    /// data replaces the default. Consumed by `StressAccumulator` when the
    /// runtime `--voxel-cure-mm` flag is active; the Tier-1 scalar path
    /// does not read this field.
    #[serde(default)]
    pub(crate) youngs_modulus_mpa: Option<f32>,

    /// Poisson's ratio (post-cure thermoset). Dimensionless. KB-163.
    /// **Optional** — when `None`, the t2f3 strain/stress path uses
    /// [`DEFAULT_POISSONS_RATIO`] (0.35 literature midpoint with ±0.05
    /// uncertainty band) and MUST disclose the uncalibrated-moduli caveat
    /// in emitted FailureEvent messages. Range when present: strictly
    /// in (-1.0, 0.5) — ν = 0.5 corresponds to incompressible material
    /// and makes the closed-form linear-elasticity stiffness singular,
    /// so the validator rejects ν >= 0.5.
    #[serde(default)]
    pub(crate) poissons_ratio: Option<f32>,

    /// Z / XY shrinkage anisotropy ratio. KB-164. Dimensionless.
    /// **Optional** — when `None`, the t2f3 strain/stress path uses
    /// [`DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO`] (1.5, literature-midpoint
    /// engineering estimate). Value > 1.0 means Z shrinkage exceeds XY
    /// shrinkage (the physical norm for layer-by-layer cure — XY is
    /// constrained by adhesion to the cured layer below, Z is free to
    /// deform). The mapping preserves `linear_shrinkage_pct`'s vendor
    /// meaning: `2·factor_xy + factor_z = 3`, so an isotropic resin
    /// (ratio = 1.0) produces the legacy uniform ε field.
    /// Range when present: strictly > 0 (validate() enforces).
    #[serde(default)]
    pub(crate) shrinkage_anisotropy_z_ratio: Option<f32>,

    /// Thermal conductivity. Unit: W/(m·K). ADR-0020 / KB-152. Tier-2
    /// thermal diffusion (t2f4) input — used by the explicit FTCS solver
    /// to advance the heat equation across the resin domain. Literature
    /// midpoint for acrylate photopolymer is ~0.20 W/m·K.
    /// **Optional on the struct** so cross-feature TOML interchange holds
    /// (default-feature builds parse legacy TOMLs without this field);
    /// **REQUIRED at validate() time when the `field-sim` Cargo feature
    /// is on** — None then is a typed validate-time error per ADR-0020
    /// §Consequences.
    #[serde(default)]
    pub(crate) thermal_conductivity_w_mk: Option<f32>,

    /// Specific heat capacity. Unit: J/(kg·K). ADR-0020. Literature
    /// midpoint for acrylate photopolymer is ~1700 J/kg·K. Same
    /// Option-on-struct / required-under-field-sim semantics as
    /// `thermal_conductivity_w_mk`.
    #[serde(default)]
    pub(crate) specific_heat_j_kgk: Option<f32>,

    /// Convective heat-transfer coefficient at the resin-air free surface.
    /// Unit: W/(m²·K). ADR-0020. Drives the Newton-cooling boundary
    /// condition at the top face of the thermal field. Still-air natural
    /// convection ~10 W/m²·K is the literature midpoint. Same Option-on-
    /// struct / required-under-field-sim semantics.
    #[serde(default)]
    pub(crate) convective_top_h_w_m2k: Option<f32>,

    /// Concrete operating point for this resin (ADR-0005 Axis 2b).
    /// **Required** — no serde default. A legacy resin TOML missing `[recipe]` fails
    /// to deserialise, surfacing the migration loudly per ADR-0005 Consequences.
    pub(crate) recipe: Recipe,
}

impl ResinProfile {
    /// Resin profile identity (used for display + matching by name).
    pub fn name(&self) -> &str {
        &self.name
    }

    // --- Public read-only accessors (pub(crate) fields per validate-on-mutation contract) ---

    pub fn penetration_depth_um(&self) -> f32 {
        self.penetration_depth_um
    }
    pub fn critical_energy_mj_cm2(&self) -> f32 {
        self.critical_energy_mj_cm2
    }
    pub fn tensile_strength_mpa(&self) -> f32 {
        self.tensile_strength_mpa
    }
    pub fn peel_adhesion_kpa(&self) -> f32 {
        self.peel_adhesion_kpa
    }
    pub fn ref_lift_speed_mm_min(&self) -> f32 {
        self.ref_lift_speed_mm_min
    }
    pub fn linear_shrinkage_pct(&self) -> f32 {
        self.linear_shrinkage_pct
    }
    pub fn viscosity_mpa_s(&self) -> f32 {
        self.viscosity_mpa_s
    }
    pub fn reference_temp_c(&self) -> f32 {
        self.reference_temp_c
    }
    pub fn activation_energy_kj_mol(&self) -> f32 {
        self.activation_energy_kj_mol
    }
    pub fn density_g_cm3(&self) -> f32 {
        self.density_g_cm3
    }
    /// ADR-0020 / t2f4 — resin thermal conductivity (W/m·K). Optional on
    /// the struct; required at validate() time under field-sim.
    pub fn thermal_conductivity_w_mk(&self) -> Option<f32> {
        self.thermal_conductivity_w_mk
    }
    /// ADR-0020 / t2f4 — resin specific heat capacity (J/kg·K). Optional
    /// on the struct; required at validate() time under field-sim.
    pub fn specific_heat_j_kgk(&self) -> Option<f32> {
        self.specific_heat_j_kgk
    }
    /// ADR-0020 / t2f4 — convective coefficient at the resin-air top free
    /// surface (W/m²·K). Optional on the struct; required at validate()
    /// time under field-sim.
    pub fn convective_top_h_w_m2k(&self) -> Option<f32> {
        self.convective_top_h_w_m2k
    }
    pub fn degradation_temp_c(&self) -> f32 {
        self.degradation_temp_c
    }
    pub fn min_safe_temp_c(&self) -> f32 {
        self.min_safe_temp_c
    }
    /// Cure-kinetics Ea, if the TOML carries a measured value. See
    /// [`DEFAULT_CURE_KINETICS_EA_KJ_MOL`] for the fallback.
    pub fn cure_kinetics_ea_kj_mol(&self) -> Option<f32> {
        self.cure_kinetics_ea_kj_mol
    }
    /// Effective Ea: the TOML value if present, otherwise the KB-153 default
    /// (30 kJ/mol). Callers that render user output SHOULD check
    /// [`cure_kinetics_ea_kj_mol`](Self::cure_kinetics_ea_kj_mol) and warn
    /// when it is None.
    pub fn effective_cure_kinetics_ea_kj_mol(&self) -> f32 {
        self.cure_kinetics_ea_kj_mol
            .unwrap_or(DEFAULT_CURE_KINETICS_EA_KJ_MOL)
    }
    /// Base-adhesion σ-elevation (kPa) if the TOML carries a value. KB-116.
    pub fn base_adhesion_elevation_kpa(&self) -> Option<f32> {
        self.base_adhesion_elevation_kpa
    }
    /// Effective base-adhesion σ-elevation: the TOML value, or 0.0 (no base
    /// term) when unset. Opt-in so legacy resins are behaviour-preserving.
    pub fn effective_base_adhesion_elevation_kpa(&self) -> f32 {
        self.base_adhesion_elevation_kpa.unwrap_or(0.0)
    }
    /// Peel shape-factor strength if the TOML carries a value. KB-185.
    pub fn peel_shape_factor_strength(&self) -> Option<f32> {
        self.peel_shape_factor_strength
    }
    /// Effective peel shape-factor strength: the TOML value, or 0.0 (shape
    /// factor ≡ 1.0, no correction) when unset. Opt-in so legacy resins are
    /// behaviour-preserving.
    pub fn effective_peel_shape_factor_strength(&self) -> f32 {
        self.peel_shape_factor_strength.unwrap_or(0.0)
    }
    /// Initial photoinitiator concentration (dimensionless fraction in [0,1]).
    /// KB-160. Consumed by the voxel cure path.
    pub fn photoinitiator_concentration_initial(&self) -> f32 {
        self.photoinitiator_concentration_initial
    }
    /// Photoinitiator decay constant if the TOML carries a measured value.
    /// See [`DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D`] for the fallback.
    pub fn photoinitiator_decay_constant_k_d(&self) -> Option<f32> {
        self.photoinitiator_decay_constant_k_d
    }
    /// Effective k_d: TOML value if present, otherwise the KB-160 default
    /// (0.05 with ±50 % uncertainty). Callers SHOULD check
    /// [`photoinitiator_decay_constant_k_d`](Self::photoinitiator_decay_constant_k_d)
    /// and warn when it is None.
    pub fn effective_photoinitiator_decay_constant_k_d(&self) -> f32 {
        self.photoinitiator_decay_constant_k_d
            .unwrap_or(DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D)
    }
    /// Young's modulus if the TOML carries a measured value (None ⇒
    /// uncalibrated). See [`DEFAULT_YOUNGS_MODULUS_MPA`] for the fallback.
    pub fn youngs_modulus_mpa(&self) -> Option<f32> {
        self.youngs_modulus_mpa
    }
    /// Effective Young's modulus: TOML value if present, otherwise the
    /// KB-163 default (2000 MPa with ±50 % uncertainty). Callers SHOULD
    /// check [`youngs_modulus_mpa`](Self::youngs_modulus_mpa) and warn /
    /// annotate emitted failures when it is None.
    pub fn effective_youngs_modulus_mpa(&self) -> f32 {
        self.youngs_modulus_mpa
            .unwrap_or(DEFAULT_YOUNGS_MODULUS_MPA)
    }
    /// Poisson's ratio if the TOML carries a measured value (None ⇒
    /// uncalibrated). See [`DEFAULT_POISSONS_RATIO`] for the fallback.
    pub fn poissons_ratio(&self) -> Option<f32> {
        self.poissons_ratio
    }
    /// Effective Poisson's ratio: TOML value if present, otherwise the
    /// KB-163 default (0.35 with ±0.05 band).
    pub fn effective_poissons_ratio(&self) -> f32 {
        self.poissons_ratio.unwrap_or(DEFAULT_POISSONS_RATIO)
    }
    /// Z / XY shrinkage anisotropy ratio if the TOML carries a measured
    /// value. See [`DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO`] for fallback.
    pub fn shrinkage_anisotropy_z_ratio(&self) -> Option<f32> {
        self.shrinkage_anisotropy_z_ratio
    }
    /// Effective Z/XY shrinkage anisotropy ratio: TOML value if present,
    /// otherwise KB-164 default (1.5 with ±0.3 band).
    pub fn effective_shrinkage_anisotropy_z_ratio(&self) -> f32 {
        self.shrinkage_anisotropy_z_ratio
            .unwrap_or(DEFAULT_SHRINKAGE_ANISOTROPY_Z_RATIO)
    }
    /// Whether the mechanical-moduli set used by the t2f3 strain/stress
    /// path is fully calibrated. Requires all THREE Option fields to be
    /// `Some`: `youngs_modulus_mpa` (KB-163), `poissons_ratio` (KB-163),
    /// and `shrinkage_anisotropy_z_ratio` (KB-164). When false, the
    /// predictor MUST disclose the uncalibrated-moduli caveat in any
    /// emitted `FailureEvent.message`. t2f3.1 widened the predicate from
    /// the original 2-of-2 (E + ν only) to 3-of-3 to close the
    /// disclosure-contract gap: a profile with E + ν explicit but
    /// z_ratio defaulted was previously reported as calibrated yet the
    /// z_ratio ±0.3 uncertainty band remains a material driver of σ_vm
    /// magnitude (post-anisotropy redesign).
    pub fn has_calibrated_moduli(&self) -> bool {
        self.youngs_modulus_mpa.is_some()
            && self.poissons_ratio.is_some()
            && self.shrinkage_anisotropy_z_ratio.is_some()
    }
    /// The concrete operating point (Recipe VO) for this resin.
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    /// Validate physical invariants. Must be called after deserialization from
    /// untrusted sources (e.g. TOML) to prevent NaN/inf propagation through
    /// downstream Beer-Lambert / Arrhenius calculations.
    ///
    /// **Contract:** intra-crate code that mutates any field of a previously
    /// validated `ResinProfile` MUST re-call `validate()` before passing the
    /// profile to a downstream service. See struct-level doc comment.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("resin name must not be empty".into());
        }
        let checks: &[(f32, &str)] = &[
            (self.penetration_depth_um, "penetration_depth_um"),
            (self.critical_energy_mj_cm2, "critical_energy_mj_cm2"),
            (self.tensile_strength_mpa, "tensile_strength_mpa"),
            (self.peel_adhesion_kpa, "peel_adhesion_kpa"),
            (self.ref_lift_speed_mm_min, "ref_lift_speed_mm_min"),
            (self.viscosity_mpa_s, "viscosity_mpa_s"),
            (self.activation_energy_kj_mol, "activation_energy_kj_mol"),
            (self.density_g_cm3, "density_g_cm3"),
        ];
        for (val, field) in checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        if self.linear_shrinkage_pct < 0.0 || !self.linear_shrinkage_pct.is_finite() {
            return Err(format!(
                "linear_shrinkage_pct must be finite and >= 0 (got {})",
                self.linear_shrinkage_pct
            ));
        }
        if !self.reference_temp_c.is_finite() {
            return Err(format!(
                "reference_temp_c must be finite (got {})",
                self.reference_temp_c
            ));
        }
        // Reference temperature feeds the Ec(T) Arrhenius formula (KB-153) as
        // 1/T_ref_K; a value at or below absolute zero produces non-physical
        // kinetics and was the HIGH-severity crash vector flagged in the
        // step-10 adversarial review.
        if self.reference_temp_c <= -273.15 {
            return Err(format!(
                "reference_temp_c must be above absolute zero (-273.15 °C), got {}",
                self.reference_temp_c
            ));
        }
        if !self.degradation_temp_c.is_finite() {
            return Err(format!(
                "degradation_temp_c must be finite (got {})",
                self.degradation_temp_c
            ));
        }
        if !self.min_safe_temp_c.is_finite() {
            return Err(format!(
                "min_safe_temp_c must be finite (got {})",
                self.min_safe_temp_c
            ));
        }
        if let Some(ea) = self.cure_kinetics_ea_kj_mol
            && (!ea.is_finite() || ea <= 0.0 || ea > 200.0)
        {
            return Err(format!(
                "cure_kinetics_ea_kj_mol, when present, must be finite and in \
                 (0.0, 200.0] kJ/mol (got {ea})"
            ));
        }
        // KB-185 / ADR-0022 Stage 3: shape-factor strength is a dimensionless
        // [0, 1] weight. Finite check FIRST so NaN doesn't silently satisfy the
        // range predicate (anti-pattern rust-nan-positive-validation-gap).
        if let Some(s) = self.peel_shape_factor_strength
            && (!s.is_finite() || !(0.0..=1.0).contains(&s))
        {
            return Err(format!(
                "peel_shape_factor_strength, when present, must be finite and in \
                 [0.0, 1.0] (got {s})"
            ));
        }
        // KB-160: photoinitiator concentration is a dimensionless [0, 1]
        // fraction. NaN-two-layer-defence (docs/patterns/nan-two-layer-defence.md):
        // explicit finite check before the range check so NaN doesn't silently
        // satisfy `0 <= NaN <= 1` (Rust's IEEE-754 NaN comparison gap, see
        // docs/patterns/anti/rust-nan-positive-validation-gap.md).
        if !self.photoinitiator_concentration_initial.is_finite()
            || !(0.0..=1.0).contains(&self.photoinitiator_concentration_initial)
        {
            return Err(format!(
                "photoinitiator_concentration_initial must be finite and in \
                 [0.0, 1.0] (got {})",
                self.photoinitiator_concentration_initial
            ));
        }
        if let Some(k_d) = self.photoinitiator_decay_constant_k_d
            && (!k_d.is_finite() || k_d <= 0.0)
        {
            return Err(format!(
                "photoinitiator_decay_constant_k_d, when present, must be \
                 finite and > 0.0 (got {k_d})"
            ));
        }
        // KB-163: Young's modulus is a stiffness, strictly positive when
        // present. Anti-pattern rust-nan-positive-validation-gap — finite
        // check FIRST so NaN doesn't pass the > 0.0 comparison.
        if let Some(e) = self.youngs_modulus_mpa
            && (!e.is_finite() || e <= 0.0)
        {
            return Err(format!(
                "youngs_modulus_mpa, when present, must be finite and > 0.0 \
                 (got {e})"
            ));
        }
        // KB-163: Poisson's ratio is dimensionless in the physical range
        // (-1.0, 0.5). ν = 0.5 is the incompressible limit and makes the
        // closed-form linear-elasticity stiffness matrix
        // (D = E·(1−ν)/((1+ν)(1−2ν))) singular — divide-by-zero in
        // StressTensor::from_strain_linear_elastic. Strict upper bound.
        if let Some(nu) = self.poissons_ratio
            && (!nu.is_finite() || nu <= -1.0 || nu >= 0.5)
        {
            return Err(format!(
                "poissons_ratio, when present, must be finite and strictly \
                 in (-1.0, 0.5) (got {nu})"
            ));
        }
        // KB-164: Z/XY shrinkage anisotropy ratio is dimensionless and
        // strictly positive. ratio = 1.0 is isotropic (legacy behaviour);
        // > 1.0 is the physical norm (Z shrinks more); < 1.0 is unusual
        // but allowed (e.g. ceramic-filled formulations with reinforcement
        // patterns).
        if let Some(r) = self.shrinkage_anisotropy_z_ratio
            && (!r.is_finite() || r <= 0.0)
        {
            return Err(format!(
                "shrinkage_anisotropy_z_ratio, when present, must be finite \
                 and > 0.0 (got {r})"
            ));
        }
        // ADR-0020 / t2f4: thermal material fields are Option<f32> on the
        // struct so cross-feature TOML interchange holds, but each present
        // value must pass the typed-boundary VO constructor's bounds (NaN +
        // sign). Under `field-sim`, None is ALSO rejected — see the
        // cfg-gated block below.
        if let Some(k) = self.thermal_conductivity_w_mk {
            crate::values::ThermalConductivity::new(k).map_err(|e| {
                format!("thermal_conductivity_w_mk: {e}")
            })?;
        }
        if let Some(cp) = self.specific_heat_j_kgk {
            crate::values::SpecificHeatCapacity::new(cp).map_err(|e| {
                format!("specific_heat_j_kgk: {e}")
            })?;
        }
        if let Some(h) = self.convective_top_h_w_m2k {
            crate::values::ConvectiveCoefficient::new(h).map_err(|e| {
                format!("convective_top_h_w_m2k: {e}")
            })?;
        }
        #[cfg(feature = "field-sim")]
        {
            if self.thermal_conductivity_w_mk.is_none() {
                return Err(
                    "thermal_conductivity_w_mk is required under the field-sim \
                     feature (ADR-0020 / KB-152). Literature midpoint for \
                     acrylate photopolymer is ~0.20 W/m·K — set it in the \
                     resin TOML."
                        .into(),
                );
            }
            if self.specific_heat_j_kgk.is_none() {
                return Err(
                    "specific_heat_j_kgk is required under the field-sim \
                     feature (ADR-0020). Literature midpoint for acrylate \
                     photopolymer is ~1700 J/kg·K — set it in the resin TOML."
                        .into(),
                );
            }
            if self.convective_top_h_w_m2k.is_none() {
                return Err(
                    "convective_top_h_w_m2k is required under the field-sim \
                     feature (ADR-0020). Still-air natural convection ~10 \
                     W/m²·K is the literature midpoint — set it in the resin \
                     TOML."
                        .into(),
                );
            }
        }
        // Both fields are validated finite above, so `>=` is safe on f32.
        if self.min_safe_temp_c >= self.degradation_temp_c {
            return Err(format!(
                "min_safe_temp_c ({}) must be strictly less than degradation_temp_c ({})",
                self.min_safe_temp_c, self.degradation_temp_c
            ));
        }
        self.recipe.validate().map_err(|e| format!("recipe: {e}"))?;
        Ok(())
    }

    /// Whether the given vat temperature exceeds this resin's degradation threshold.
    pub fn is_degradation_risk(&self, vat_temp: VatTemperature) -> bool {
        vat_temp.value() > self.degradation_temp_c
    }

    /// Whether the given vat temperature is below this resin's minimum safe operating point.
    pub fn is_too_cold(&self, vat_temp: VatTemperature) -> bool {
        vat_temp.value() < self.min_safe_temp_c
    }

    /// Elegoo Ceramic Grey V2.
    /// Sources: Elegoo published mechanical specs; optical/adhesion values estimated
    /// from ceramic-filled resin literature (calibrate with Athena II).
    /// Recipe: ceramic-filled resin needs thinner layers + longer cure per ADR-0005.
    pub fn elegoo_ceramic_grey_v2() -> Self {
        Self {
            name: "Elegoo Ceramic Grey V2".into(),
            penetration_depth_um: 145.0, // ceramic particles scatter, shallower cure
            critical_energy_mj_cm2: 5.5,
            tensile_strength_mpa: 38.0,  // Elegoo published spec
            peel_adhesion_kpa: 9.5,      // ceramic-filled: lower FEP adhesion than standard
            ref_lift_speed_mm_min: 60.0, // measurement speed for peel_adhesion_kpa (KB-112)
            linear_shrinkage_pct: 0.9,   // ceramic-constrained
            viscosity_mpa_s: 350.0,      // higher viscosity from ceramic filler
            reference_temp_c: 25.0,
            activation_energy_kj_mol: 52.0,
            density_g_cm3: 1.25, // ceramic filler increases density
            degradation_temp_c: default_degradation_temp_c(),
            min_safe_temp_c: default_min_safe_temp_c(),
            cure_kinetics_ea_kj_mol: None, // KB-153: no measured value — uses default 30 kJ/mol w/ loud warning
            base_adhesion_elevation_kpa: None, // KB-116: opt-in; None ⇒ 0.0 (no base term). Calibrated value lives in the TOML.
            peel_shape_factor_strength: None, // KB-185: opt-in; None ⇒ 0.0 (shape factor ≡ 1.0). Active value lives in the TOML.
            photoinitiator_concentration_initial: DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL,
            photoinitiator_decay_constant_k_d: None, // KB-160: no measured value — uses default 0.05 w/ ±50 % loud warning
            youngs_modulus_mpa: None, // KB-163: deliberately uncalibrated — see data/resins/elegoo_ceramic_grey_v2.toml comment
            poissons_ratio: None,     // KB-163: deliberately uncalibrated
            shrinkage_anisotropy_z_ratio: None, // KB-164: uncalibrated; falls back to 1.5
            // ADR-0020 / t2f4: thermal material properties. Ceramic-filled
            // resin has slightly higher conductivity from the ceramic phase
            // but specific heat tracks the polymer matrix.
            thermal_conductivity_w_mk: Some(0.25), // higher than neat acrylate due to ceramic filler
            specific_heat_j_kgk: Some(1600.0),     // ceramic-filled, slightly lower than neat acrylate
            convective_top_h_w_m2k: Some(10.0),    // still-air natural convection
            recipe: Recipe::elegoo_ceramic_grey(),
        }
    }

    /// Generic standard resin with conservative defaults from KB data.
    pub fn generic_standard() -> Self {
        Self {
            name: "Generic Standard".into(),
            penetration_depth_um: 170.0, // KB-100: Premium Black
            critical_energy_mj_cm2: 5.0, // KB-100: Premium Black
            tensile_strength_mpa: 35.0,  // KB-140: conservative
            peel_adhesion_kpa: 13.0,     // KB-110: standard FEP
            ref_lift_speed_mm_min: 60.0, // measurement speed for peel_adhesion_kpa
            linear_shrinkage_pct: 1.5,   // KB-142: standard range
            viscosity_mpa_s: 200.0,      // KB-141: typical
            reference_temp_c: 25.0,
            activation_energy_kj_mol: 52.0, // KB-150: derived from 82% drop
            density_g_cm3: 1.1,
            degradation_temp_c: default_degradation_temp_c(),
            min_safe_temp_c: default_min_safe_temp_c(),
            cure_kinetics_ea_kj_mol: None, // KB-153: no measured value — uses default 30 kJ/mol w/ loud warning
            base_adhesion_elevation_kpa: None, // KB-116: opt-in; None ⇒ 0.0 (no base term). Calibrated value lives in the TOML.
            peel_shape_factor_strength: None, // KB-185: opt-in; None ⇒ 0.0 (shape factor ≡ 1.0). Active value lives in the TOML.
            photoinitiator_concentration_initial: DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL,
            photoinitiator_decay_constant_k_d: None, // KB-160: no measured value — uses default 0.05 w/ ±50 % loud warning
            youngs_modulus_mpa: Some(2000.0), // KB-163: literature-midpoint photopolymer modulus (Premium-Black-class)
            poissons_ratio: Some(0.35),       // KB-163: standard thermoset
            shrinkage_anisotropy_z_ratio: Some(1.5), // KB-164: layer-by-layer Z constraint
            // ADR-0020 / t2f4: literature midpoint for neat acrylate photopolymer.
            thermal_conductivity_w_mk: Some(0.20),
            specific_heat_j_kgk: Some(1700.0),
            convective_top_h_w_m2k: Some(10.0),
            recipe: Recipe::generic_standard(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_standard_passes_validation() {
        ResinProfile::generic_standard()
            .validate()
            .expect("ResinProfile::generic_standard() factory must satisfy validate()");
    }

    #[test]
    fn elegoo_ceramic_grey_v2_passes_validation() {
        ResinProfile::elegoo_ceramic_grey_v2()
            .validate()
            .expect("ResinProfile::elegoo_ceramic_grey_v2() factory must satisfy validate()");
    }

    #[test]
    fn effective_base_adhesion_elevation_defaults_to_zero_when_unset() {
        let mut r = ResinProfile::generic_standard();
        r.base_adhesion_elevation_kpa = None;
        assert_eq!(r.effective_base_adhesion_elevation_kpa(), 0.0);
    }

    #[test]
    fn effective_base_adhesion_elevation_returns_set_value() {
        let mut r = ResinProfile::generic_standard();
        r.base_adhesion_elevation_kpa = Some(25.0);
        assert!((r.effective_base_adhesion_elevation_kpa() - 25.0).abs() < 1e-6);
    }

    // --- KB-185 peel shape-factor strength (ADR-0022 Stage 3) ---

    #[test]
    fn effective_peel_shape_factor_strength_defaults_to_zero_when_unset() {
        let mut r = ResinProfile::generic_standard();
        r.peel_shape_factor_strength = None;
        assert_eq!(r.effective_peel_shape_factor_strength(), 0.0);
    }

    #[test]
    fn effective_peel_shape_factor_strength_returns_set_value() {
        let mut r = ResinProfile::generic_standard();
        r.peel_shape_factor_strength = Some(0.5);
        assert!((r.effective_peel_shape_factor_strength() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn factories_default_peel_shape_factor_strength_to_none() {
        assert!(
            ResinProfile::generic_standard()
                .peel_shape_factor_strength()
                .is_none()
        );
        assert!(
            ResinProfile::elegoo_ceramic_grey_v2()
                .peel_shape_factor_strength()
                .is_none()
        );
    }

    #[test]
    fn peel_shape_factor_strength_below_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.peel_shape_factor_strength = Some(-0.1);
        assert!(p.validate().is_err());
    }

    #[test]
    fn peel_shape_factor_strength_above_one_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.peel_shape_factor_strength = Some(1.5);
        assert!(p.validate().is_err());
    }

    #[test]
    fn peel_shape_factor_strength_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.peel_shape_factor_strength = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn peel_shape_factor_strength_bounds_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.peel_shape_factor_strength = Some(0.0);
        assert!(p.validate().is_ok());
        p.peel_shape_factor_strength = Some(1.0);
        assert!(p.validate().is_ok());
        p.peel_shape_factor_strength = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn peel_shape_factor_strength_round_trips_through_toml() {
        // Minimal valid resin TOML (mirrors tests/uat_steps/legacy_resin_toml_defaults).
        let base = r#"name = "T"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#;
        // Legacy TOML (field absent) → None via #[serde(default)], still valid.
        let legacy: ResinProfile = toml::from_str(base).expect("legacy resin TOML parses");
        assert_eq!(legacy.peel_shape_factor_strength(), None);
        legacy.validate().expect("legacy resin validates");

        // Explicit value parses + validates — guards the shipped
        // generic_standard.toml = 0.5 against a serde/validation regression.
        let with_field = format!("peel_shape_factor_strength = 0.5\n{base}");
        let parsed: ResinProfile =
            toml::from_str(&with_field).expect("resin TOML with the field parses");
        assert_eq!(parsed.peel_shape_factor_strength(), Some(0.5));
        parsed.validate().expect("0.5 strength validates");
    }

    #[test]
    fn zero_critical_energy_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.critical_energy_mj_cm2 = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn negative_penetration_depth_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.penetration_depth_um = -5.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_field_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.viscosity_mpa_s = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn empty_name_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.name = "   ".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_degradation_temp_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.degradation_temp_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_min_safe_temp_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn min_safe_above_degradation_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 60.0;
        p.degradation_temp_c = 50.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn min_safe_equal_to_degradation_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 50.0;
        p.degradation_temp_c = 50.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn reference_temp_c_below_absolute_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.reference_temp_c = -400.0;
        let err = p
            .validate()
            .expect_err("reference_temp_c below absolute zero must fail validate()");
        assert!(
            err.contains("absolute zero"),
            "error must cite absolute zero: {err}"
        );
    }

    #[test]
    fn reference_temp_c_at_absolute_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.reference_temp_c = -273.15;
        assert!(p.validate().is_err());
    }

    #[test]
    fn nan_ref_lift_speed_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.ref_lift_speed_mm_min = f32::NAN;
        assert!(p.validate().is_err());
    }

    #[test]
    fn zero_ref_lift_speed_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.ref_lift_speed_mm_min = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn cure_kinetics_ea_defaults_to_none_on_factories() {
        assert!(ResinProfile::generic_standard()
            .cure_kinetics_ea_kj_mol
            .is_none());
        assert!(ResinProfile::elegoo_ceramic_grey_v2()
            .cure_kinetics_ea_kj_mol
            .is_none());
    }

    #[test]
    fn effective_cure_kinetics_ea_uses_default_when_none() {
        let p = ResinProfile::generic_standard();
        assert_eq!(
            p.effective_cure_kinetics_ea_kj_mol(),
            DEFAULT_CURE_KINETICS_EA_KJ_MOL
        );
    }

    #[test]
    fn effective_cure_kinetics_ea_uses_measured_when_some() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(42.0);
        assert_eq!(p.effective_cure_kinetics_ea_kj_mol(), 42.0);
    }

    #[test]
    fn cure_kinetics_ea_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(0.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn cure_kinetics_ea_negative_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(-5.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn cure_kinetics_ea_above_bound_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(250.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn cure_kinetics_ea_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn cure_kinetics_ea_none_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn cure_kinetics_ea_at_bound_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.cure_kinetics_ea_kj_mol = Some(200.0);
        assert!(p.validate().is_ok());
    }

    // --- KB-160 photoinitiator fields tests ---

    #[test]
    fn photoinitiator_concentration_initial_default_is_one() {
        let p = ResinProfile::generic_standard();
        assert_eq!(
            p.photoinitiator_concentration_initial(),
            DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL
        );
        assert_eq!(p.photoinitiator_concentration_initial(), 1.0);
    }

    #[test]
    fn photoinitiator_decay_constant_k_d_default_is_none() {
        let p = ResinProfile::generic_standard();
        assert_eq!(p.photoinitiator_decay_constant_k_d(), None);
        // The effective k_d falls back to the KB-160 literature midpoint.
        assert_eq!(
            p.effective_photoinitiator_decay_constant_k_d(),
            DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D
        );
        assert_eq!(p.effective_photoinitiator_decay_constant_k_d(), 0.05);
    }

    #[test]
    fn photoinitiator_concentration_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_concentration_initial = f32::NAN;
        let err = p
            .validate()
            .expect_err("NaN photoinitiator_concentration must fail validate");
        assert!(
            err.contains("photoinitiator_concentration_initial"),
            "error names the offending field: {err}"
        );
    }

    #[test]
    fn photoinitiator_concentration_negative_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_concentration_initial = -0.1;
        assert!(p.validate().is_err());
    }

    #[test]
    fn photoinitiator_concentration_above_one_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_concentration_initial = 1.5;
        assert!(p.validate().is_err());
    }

    #[test]
    fn photoinitiator_concentration_zero_accepted_as_boundary() {
        // Zero is a legitimate boundary (a resin with all photoinitiator
        // exhausted before t=0 — pathological but well-defined). The voxel
        // cure path will treat such a field as "burnt-out everywhere" per
        // KB-160's C_threshold floor.
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_concentration_initial = 0.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn photoinitiator_concentration_one_accepted_as_boundary() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_concentration_initial = 1.0;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn photoinitiator_decay_k_d_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_decay_constant_k_d = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn photoinitiator_decay_k_d_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_decay_constant_k_d = Some(0.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn photoinitiator_decay_k_d_negative_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_decay_constant_k_d = Some(-0.01);
        assert!(p.validate().is_err());
    }

    #[test]
    fn photoinitiator_decay_k_d_some_finite_positive_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.photoinitiator_decay_constant_k_d = Some(0.07);
        assert!(p.validate().is_ok());
        assert_eq!(p.photoinitiator_decay_constant_k_d(), Some(0.07));
        // effective_ returns the TOML value, not the default.
        assert_eq!(p.effective_photoinitiator_decay_constant_k_d(), 0.07);
    }

    #[test]
    fn photoinitiator_decay_k_d_none_uses_default() {
        let p = ResinProfile::generic_standard();
        // None ⇒ effective_ falls back to KB-160 default. Callers SHOULD warn.
        assert_eq!(p.photoinitiator_decay_constant_k_d(), None);
        assert_eq!(
            p.effective_photoinitiator_decay_constant_k_d(),
            DEFAULT_PHOTOINITIATOR_DECAY_CONSTANT_K_D
        );
    }

    // --- KB-163 Young's modulus + Poisson's ratio fields tests (t2f3) ---

    #[test]
    fn youngs_modulus_mpa_some_on_generic_standard() {
        // generic_standard ships with explicit KB-163 midpoints so the
        // common test fixture exercises the calibrated path, not the
        // default-with-warn path. has_calibrated_moduli() must agree.
        let p = ResinProfile::generic_standard();
        assert_eq!(p.youngs_modulus_mpa(), Some(2000.0));
        assert_eq!(p.poissons_ratio(), Some(0.35));
        assert!(p.has_calibrated_moduli());
    }

    #[test]
    fn youngs_modulus_mpa_defaults_to_none_on_elegoo() {
        // Elegoo Ceramic Grey V2 is deliberately uncalibrated until
        // Athena II measurements arrive.
        let p = ResinProfile::elegoo_ceramic_grey_v2();
        assert!(p.youngs_modulus_mpa().is_none());
        assert!(p.poissons_ratio().is_none());
        assert!(!p.has_calibrated_moduli());
    }

    #[test]
    fn effective_youngs_modulus_uses_default_when_none() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = None;
        assert_eq!(p.effective_youngs_modulus_mpa(), DEFAULT_YOUNGS_MODULUS_MPA);
        assert_eq!(p.effective_youngs_modulus_mpa(), 2000.0);
    }

    #[test]
    fn effective_youngs_modulus_uses_measured_when_some() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = Some(2750.0);
        assert_eq!(p.effective_youngs_modulus_mpa(), 2750.0);
    }

    #[test]
    fn effective_poissons_ratio_uses_default_when_none() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = None;
        assert_eq!(p.effective_poissons_ratio(), DEFAULT_POISSONS_RATIO);
        assert_eq!(p.effective_poissons_ratio(), 0.35);
    }

    #[test]
    fn youngs_modulus_zero_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = Some(0.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn youngs_modulus_negative_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = Some(-1.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn youngs_modulus_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn youngs_modulus_none_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.youngs_modulus_mpa = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn poissons_ratio_nan_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(f32::NAN);
        assert!(p.validate().is_err());
    }

    #[test]
    fn poissons_ratio_at_incompressible_limit_rejected() {
        // ν = 0.5 makes the closed-form 6×6 stiffness singular —
        // FailurePredictor / StressAccumulator would divide by zero.
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(0.5);
        let err = p
            .validate()
            .expect_err("ν = 0.5 must be rejected (incompressible limit)");
        assert!(
            err.contains("poissons_ratio"),
            "error names the field: {err}"
        );
    }

    #[test]
    fn poissons_ratio_above_half_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(0.6);
        assert!(p.validate().is_err());
    }

    #[test]
    fn poissons_ratio_at_minus_one_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(-1.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn poissons_ratio_below_minus_one_rejected() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(-1.5);
        assert!(p.validate().is_err());
    }

    #[test]
    fn poissons_ratio_zero_accepted_rod_like() {
        // ν = 0 is physical (rod-like, no lateral contraction).
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(0.0);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn poissons_ratio_near_incompressible_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = Some(0.49);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn poissons_ratio_none_accepted() {
        let mut p = ResinProfile::generic_standard();
        p.poissons_ratio = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn has_calibrated_moduli_requires_all_three() {
        // t2f3.1 A1: predicate is 3-of-3 — E, ν, AND z_ratio must
        // all be Some. Exhaustive single-axis miss coverage:
        let mut p = ResinProfile::generic_standard();
        assert!(p.has_calibrated_moduli());
        p.poissons_ratio = None;
        assert!(
            !p.has_calibrated_moduli(),
            "ν None must report uncalibrated"
        );
        p.poissons_ratio = Some(0.35);
        p.youngs_modulus_mpa = None;
        assert!(
            !p.has_calibrated_moduli(),
            "E None must report uncalibrated"
        );
        p.youngs_modulus_mpa = Some(2000.0);
        p.shrinkage_anisotropy_z_ratio = None;
        assert!(
            !p.has_calibrated_moduli(),
            "z_ratio None must report uncalibrated (t2f3.1 widening)"
        );
    }

    #[test]
    fn legacy_toml_without_mechanical_moduli_applies_defaults() {
        let toml_str = legacy_toml_without_thermal_thresholds();
        let p: ResinProfile = toml::from_str(&toml_str)
            .expect("legacy TOML without youngs_modulus_mpa / poissons_ratio must parse");
        assert_eq!(p.youngs_modulus_mpa(), None);
        assert_eq!(p.poissons_ratio(), None);
        assert!(!p.has_calibrated_moduli());
        assert_eq!(p.effective_youngs_modulus_mpa(), DEFAULT_YOUNGS_MODULUS_MPA);
        assert_eq!(p.effective_poissons_ratio(), DEFAULT_POISSONS_RATIO);
        p.validate()
            .expect("defaulted mechanical moduli must satisfy validate()");
    }

    #[test]
    fn toml_with_explicit_mechanical_moduli_round_trips() {
        // t2f3.1 A1: has_calibrated_moduli is 3-of-3 (E + ν + z_ratio).
        // Any fixture exercising the calibrated-disclosure path must
        // therefore set all three explicitly.
        let toml_str = format!(
            "{}\nyoungs_modulus_mpa = 2500.0\npoissons_ratio = 0.32\nshrinkage_anisotropy_z_ratio = 1.4\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile = toml::from_str(&toml_str)
            .expect("explicit youngs_modulus_mpa + poissons_ratio + z_ratio TOML must parse");
        assert_eq!(p.youngs_modulus_mpa(), Some(2500.0));
        assert_eq!(p.poissons_ratio(), Some(0.32));
        assert_eq!(p.shrinkage_anisotropy_z_ratio(), Some(1.4));
        assert!(p.has_calibrated_moduli());
        assert_eq!(p.effective_youngs_modulus_mpa(), 2500.0);
        assert_eq!(p.effective_poissons_ratio(), 0.32);
        assert_eq!(p.effective_shrinkage_anisotropy_z_ratio(), 1.4);
        p.validate()
            .expect("explicit mechanical moduli values must satisfy validate()");
    }

    #[test]
    fn toml_with_nan_youngs_modulus_rejected_by_validate() {
        let toml_str = format!(
            "{}\nyoungs_modulus_mpa = nan\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("NaN youngs_modulus_mpa must fail validate()");
        assert!(
            err.contains("youngs_modulus_mpa"),
            "error must name the field: {err}"
        );
    }

    #[test]
    fn toml_with_singular_poissons_ratio_rejected_by_validate() {
        // ν = 0.5 is the incompressible limit — stiffness matrix singular.
        let toml_str = format!(
            "{}\npoissons_ratio = 0.5\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("ν = 0.5 must fail validate() (incompressible limit)");
        assert!(
            err.contains("poissons_ratio"),
            "error must name the field: {err}"
        );
    }

    #[test]
    fn validate_delegates_to_recipe() {
        let mut p = ResinProfile::generic_standard();
        p.recipe.normal_exposure_sec = f32::NAN;
        let err = p.validate().expect_err("NaN in recipe must bubble up");
        assert!(
            err.contains("recipe"),
            "error prefixed with 'recipe': {err}"
        );
    }

    #[test]
    fn is_degradation_risk_uses_profile_threshold() {
        let mut p = ResinProfile::generic_standard();
        p.degradation_temp_c = 40.0;
        assert!(p.is_degradation_risk(
            VatTemperature::new(41.0).expect("test fixture: 41.0 °C is in VatTemperature domain")
        ));
        assert!(!p.is_degradation_risk(
            VatTemperature::new(39.0).expect("test fixture: 39.0 °C is in VatTemperature domain")
        ));
    }

    #[test]
    fn is_too_cold_uses_profile_threshold() {
        let mut p = ResinProfile::generic_standard();
        p.min_safe_temp_c = 18.0;
        assert!(p.is_too_cold(
            VatTemperature::new(17.0).expect("test fixture: 17.0 °C is in VatTemperature domain")
        ));
        assert!(!p.is_too_cold(
            VatTemperature::new(20.0).expect("test fixture: 20.0 °C is in VatTemperature domain")
        ));
    }

    // Contract demonstration — see ResinProfile struct doc comment.
    #[test]
    fn validate_after_mutation_contract() {
        let mut p = ResinProfile::generic_standard();
        p.validate().expect("baseline profile must be valid");
        p.name = "   ".into();
        assert!(
            p.validate().is_err(),
            "validate() must be re-called after intra-crate field mutation; \
             whitespace name should now be rejected"
        );
    }

    // --- Legacy-TOML serde(default) regression tests (T1-F6 block, updated for ADR-0005).
    //
    // KB-150 added `degradation_temp_c` + `min_safe_temp_c` via #[serde(default)] so
    // legacy TOMLs parse. ADR-0005 adds a REQUIRED `[recipe]` table (no serde(default))
    // — pre-refactor resin TOMLs without `[recipe]` must fail LOUDLY. The fixture below
    // embeds a valid `[recipe]` table so these tests continue exercising thermal-ordering
    // invariants; new tests below assert that the missing-`[recipe]` case fails to parse.

    /// Root-level (non-recipe) legacy TOML with no thermal thresholds present — allows
    /// tests to insert extra root-level fields (e.g. min_safe_temp_c) BEFORE the [recipe]
    /// table is appended. Without this split, an append-to-end pattern would place
    /// extra root fields inside the [recipe] table and they would not hit ResinProfile.
    fn legacy_toml_root_without_thermal_thresholds() -> String {
        // ADR-0020 / t2f4: thermal material fields are REQUIRED under
        // field-sim. They're optional on the struct so default-feature
        // tests still parse the same fixture; under field-sim they're
        // present so validate() passes. Tests targeting "missing optional
        // KB-150 thermal thresholds" still assert their absence — those
        // are `degradation_temp_c` / `min_safe_temp_c`, not the t2f4
        // material properties.
        r#"
name = "Legacy Resin"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
thermal_conductivity_w_mk = 0.20
specific_heat_j_kgk = 1700.0
convective_top_h_w_m2k = 10.0
"#
        .to_string()
    }

    /// Valid [recipe] table appended to `legacy_toml_root_*` fixtures.
    fn valid_recipe_table() -> &'static str {
        r#"
[recipe]
layer_height_um = 50.0
bottom_layer_count = 6
transition_layers = 3
normal_exposure_sec = 2.5
bottom_exposure_sec = 25.0
wait_before_cure_sec = 0.5
wait_before_release_sec = 1.0
wait_after_release_sec = 0.0
lift_speed_mm_min = 60.0
lift_cycle_sec = 7.5
lift_distance_mm = 5.0
"#
    }

    /// Baseline legacy TOML updated per ADR-0005 — keeps thermal-threshold fields absent
    /// (to exercise KB-150 serde defaults) but includes `[recipe]` so deserialize succeeds.
    fn legacy_toml_without_thermal_thresholds() -> String {
        format!(
            "{}{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        )
    }

    #[test]
    fn legacy_toml_full_missing_applies_both_defaults() {
        // Both thermal fields absent → serde fills 50.0 / 15.0.
        let toml_str = legacy_toml_without_thermal_thresholds();
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("legacy TOML must parse with serde defaults");
        assert_eq!(p.degradation_temp_c, default_degradation_temp_c());
        assert_eq!(p.min_safe_temp_c, default_min_safe_temp_c());
        p.validate()
            .expect("defaulted thermal thresholds must satisfy validate()");
    }

    #[test]
    fn legacy_toml_partial_missing_applies_single_default() {
        // Append min_safe_temp_c to the ROOT fixture (before the [recipe] table) —
        // ResinProfile.min_safe_temp_c is a root field. Appending after [recipe] would
        // misplace it inside the recipe table (checked by this test's assertions).
        let toml_str = format!(
            "{}\nmin_safe_temp_c = 12.0\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("partial-legacy TOML must parse with serde defaults");
        assert_eq!(p.min_safe_temp_c, 12.0);
        assert_eq!(p.degradation_temp_c, default_degradation_temp_c());
        p.validate()
            .expect("12 < default 50 — ordering invariant holds");
    }

    #[test]
    fn legacy_toml_invariant_crossing_rejected_by_validate() {
        let toml_str = format!(
            "{}\nmin_safe_temp_c = 55.0\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("parse must succeed; validate() is the gate");
        assert_eq!(p.min_safe_temp_c, 55.0);
        assert_eq!(p.degradation_temp_c, default_degradation_temp_c());
        let err = p
            .validate()
            .expect_err("55 > default-50 violates ordering invariant");
        assert!(
            err.contains("min_safe_temp_c") && err.contains("degradation_temp_c"),
            "error must identify both offending fields; got: {err}"
        );
    }

    // --- New ADR-0005 test: pre-refactor resin TOML (no [recipe] table) fails loudly. ---

    #[test]
    fn legacy_toml_missing_recipe_rejected() {
        // Pre-refactor resin TOML — no [recipe] table. Must fail to parse because
        // Recipe is required (no serde default).
        let toml_str = r#"
name = "Pre-refactor Legacy Resin"
penetration_depth_um = 170.0
critical_energy_mj_cm2 = 5.0
tensile_strength_mpa = 35.0
peel_adhesion_kpa = 13.0
ref_lift_speed_mm_min = 60.0
linear_shrinkage_pct = 1.5
viscosity_mpa_s = 200.0
reference_temp_c = 25.0
activation_energy_kj_mol = 52.0
density_g_cm3 = 1.1
"#;
        let result: Result<ResinProfile, _> = toml::from_str(toml_str);
        let err = result.expect_err("legacy TOML without [recipe] must fail to parse");
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("recipe"),
            "parse error must name the missing recipe field: {err_msg}"
        );
    }

    #[test]
    fn legacy_toml_with_nan_recipe_field_rejected() {
        let toml_str = legacy_toml_without_thermal_thresholds()
            .replace("normal_exposure_sec = 2.5", "normal_exposure_sec = nan");
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("NaN in recipe field must fail validate()");
        assert!(
            err.contains("recipe") && err.contains("normal_exposure_sec"),
            "error must name recipe + field: {err}"
        );
    }

    // --- KB-160 legacy TOML compat ---

    #[test]
    fn legacy_toml_without_photoinitiator_fields_applies_defaults() {
        // Legacy TOML (no photoinitiator_* fields) parses with
        // photoinitiator_concentration_initial = 1.0 (serde default) and
        // photoinitiator_decay_constant_k_d = None (serde default), satisfies
        // validate(). Matches the cure_kinetics_ea_kj_mol Option-default
        // precedent.
        let toml_str = legacy_toml_without_thermal_thresholds();
        let p: ResinProfile = toml::from_str(&toml_str)
            .expect("legacy TOML without photoinitiator_* fields must parse");
        assert_eq!(
            p.photoinitiator_concentration_initial(),
            DEFAULT_PHOTOINITIATOR_CONCENTRATION_INITIAL
        );
        assert_eq!(p.photoinitiator_decay_constant_k_d(), None);
        p.validate()
            .expect("defaulted photoinitiator fields must satisfy validate()");
    }

    #[test]
    fn toml_with_explicit_photoinitiator_fields_round_trips() {
        let toml_str = format!(
            "{}\nphotoinitiator_concentration_initial = 0.85\nphotoinitiator_decay_constant_k_d = 0.07\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("explicit photoinitiator_* TOML must parse");
        assert_eq!(p.photoinitiator_concentration_initial(), 0.85);
        assert_eq!(p.photoinitiator_decay_constant_k_d(), Some(0.07));
        assert_eq!(p.effective_photoinitiator_decay_constant_k_d(), 0.07);
        p.validate()
            .expect("explicit photoinitiator_* values must satisfy validate()");
    }

    #[test]
    fn toml_with_nan_photoinitiator_concentration_rejected_by_validate() {
        let toml_str = format!(
            "{}\nphotoinitiator_concentration_initial = nan\n{}",
            legacy_toml_root_without_thermal_thresholds(),
            valid_recipe_table()
        );
        let p: ResinProfile =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = p
            .validate()
            .expect_err("NaN photoinitiator_concentration must fail validate()");
        assert!(
            err.contains("photoinitiator_concentration_initial"),
            "error must name the field: {err}"
        );
    }

    // --- t2f3.1 A1: has_calibrated_moduli is a 3-of-3 predicate ---

    #[test]
    fn has_calibrated_moduli_false_when_z_ratio_unset() {
        // A profile with E + ν explicit but z_ratio defaulted is
        // partially calibrated — the disclosure caveat MUST fire.
        let p = ResinProfile {
            shrinkage_anisotropy_z_ratio: None,
            ..ResinProfile::generic_standard()
        };
        assert!(p.youngs_modulus_mpa().is_some());
        assert!(p.poissons_ratio().is_some());
        assert!(p.shrinkage_anisotropy_z_ratio().is_none());
        assert!(
            !p.has_calibrated_moduli(),
            "partial calibration (z_ratio unset) must NOT report calibrated"
        );
    }

    #[test]
    fn has_calibrated_moduli_true_when_all_three_set() {
        // Positive control: generic_standard ships with E=2000, ν=0.35,
        // z_ratio=1.5 — all three Some.
        let p = ResinProfile::generic_standard();
        assert!(p.youngs_modulus_mpa().is_some());
        assert!(p.poissons_ratio().is_some());
        assert!(p.shrinkage_anisotropy_z_ratio().is_some());
        assert!(
            p.has_calibrated_moduli(),
            "all-three-Some must report calibrated"
        );
    }

    #[test]
    fn has_calibrated_moduli_false_when_e_unset() {
        // Coverage of the E-only-missing direction.
        let p = ResinProfile {
            youngs_modulus_mpa: None,
            ..ResinProfile::generic_standard()
        };
        assert!(!p.has_calibrated_moduli());
    }

    #[test]
    fn has_calibrated_moduli_false_when_nu_unset() {
        // Coverage of the ν-only-missing direction.
        let p = ResinProfile {
            poissons_ratio: None,
            ..ResinProfile::generic_standard()
        };
        assert!(!p.has_calibrated_moduli());
    }
}
