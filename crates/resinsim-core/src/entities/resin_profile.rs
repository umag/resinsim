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
}
