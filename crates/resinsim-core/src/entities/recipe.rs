use serde::{Deserialize, Serialize};

/// Concrete operating point for a print — the values a slicer writes into a CTB.
///
/// Recipe is a **value object** nested inside `ResinProfile`: no identity, equality by
/// value, replaced as a unit. Fields are operationally chosen per resin (Voxeldance
/// Tango / ChituBox / Lychee call this "resin settings"). The owning `PrinterProfile`'s
/// hardware envelope constrains the valid range for each field; `PairingValidator`
/// enforces that constraint at simulation entry. See ADR-0005.
///
/// # Construction
///
/// External code constructs a Recipe via factory methods (`Recipe::generic_standard`,
/// `Recipe::elegoo_ceramic_grey`) or via TOML deserialisation inside a `ResinProfile`.
/// The in-crate `Recipe::new` constructor is `pub(crate)` so tests and factory methods
/// funnel through a single validated entry point.
///
/// # Validate-on-mutation contract
///
/// Fields are `pub(crate)` per `docs/patterns/entity-validate-on-mutation.md`. After
/// any intra-crate mutation, `validate()` MUST be re-called before treating the recipe
/// as trusted. `ResinProfile::validate()` delegates to `recipe.validate()`; downstream
/// services (`FailurePredictor`, `PairingValidator`) assume this has already run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub(crate) layer_height_um: f32,
    pub(crate) bottom_layer_count: u32,
    pub(crate) transition_layers: u32,
    pub(crate) normal_exposure_sec: f32,
    pub(crate) bottom_exposure_sec: f32,
    pub(crate) wait_before_cure_sec: f32,
    pub(crate) wait_before_release_sec: f32,
    pub(crate) wait_after_release_sec: f32,
    pub(crate) lift_speed_mm_min: f32,
    pub(crate) lift_cycle_sec: f32,
    pub(crate) lift_distance_mm: f32,

    /// Retract speed (plate or vat return motion). Linear-mechanism printers
    /// commonly have retract speed 2–3× faster than lift speed; `LayerTimingCalculator`
    /// uses both to compute per-layer time. For Tilt-mechanism printers this is
    /// CTB metadata only (the calculator ignores it; see ADR-0007).
    ///
    /// Serde default `None` ⇒ fall back to `lift_speed_mm_min`. Legacy TOMLs
    /// written before this field existed behave as if lift and retract speeds
    /// are equal.
    #[serde(default)]
    pub(crate) retract_speed_mm_min: Option<f32>,
}

impl Recipe {
    /// In-crate constructor. All floating-point fields must be finite and positive.
    /// Wait times must be finite and >= 0 (zero wait is legitimate). `retract_speed_mm_min`
    /// is `Option<f32>`; `None` falls back to `lift_speed_mm_min` at read time.
    ///
    /// 12 arguments matches the 12 Recipe fields — a builder would hide the
    /// validated-construction contract that funnels through this single entry point.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        layer_height_um: f32,
        bottom_layer_count: u32,
        transition_layers: u32,
        normal_exposure_sec: f32,
        bottom_exposure_sec: f32,
        wait_before_cure_sec: f32,
        wait_before_release_sec: f32,
        wait_after_release_sec: f32,
        lift_speed_mm_min: f32,
        lift_cycle_sec: f32,
        lift_distance_mm: f32,
        retract_speed_mm_min: Option<f32>,
    ) -> Result<Self, String> {
        let r = Self {
            layer_height_um,
            bottom_layer_count,
            transition_layers,
            normal_exposure_sec,
            bottom_exposure_sec,
            wait_before_cure_sec,
            wait_before_release_sec,
            wait_after_release_sec,
            lift_speed_mm_min,
            lift_cycle_sec,
            lift_distance_mm,
            retract_speed_mm_min,
        };
        r.validate()?;
        Ok(r)
    }

    pub fn validate(&self) -> Result<(), String> {
        let positive_checks: &[(f32, &str)] = &[
            (self.layer_height_um, "layer_height_um"),
            (self.normal_exposure_sec, "normal_exposure_sec"),
            (self.bottom_exposure_sec, "bottom_exposure_sec"),
            (self.lift_speed_mm_min, "lift_speed_mm_min"),
            (self.lift_cycle_sec, "lift_cycle_sec"),
            (self.lift_distance_mm, "lift_distance_mm"),
        ];
        for (val, field) in positive_checks {
            if !val.is_finite() || *val <= 0.0 {
                return Err(format!("{field} must be finite and > 0 (got {val})"));
            }
        }
        let non_negative_checks: &[(f32, &str)] = &[
            (self.wait_before_cure_sec, "wait_before_cure_sec"),
            (self.wait_before_release_sec, "wait_before_release_sec"),
            (self.wait_after_release_sec, "wait_after_release_sec"),
        ];
        for (val, field) in non_negative_checks {
            if !val.is_finite() || *val < 0.0 {
                return Err(format!("{field} must be finite and >= 0 (got {val})"));
            }
        }
        // retract_speed_mm_min: when present, must be finite and > 0. None is legal
        // (falls back to lift_speed_mm_min at read time).
        if let Some(v) = self.retract_speed_mm_min
            && (!v.is_finite() || v <= 0.0)
        {
            return Err(format!(
                "retract_speed_mm_min must be finite and > 0 when set (got {v})"
            ));
        }
        Ok(())
    }

    // --- Public read-only accessors (pub(crate) fields per validate-on-mutation contract) ---

    pub fn layer_height_um(&self) -> f32 {
        self.layer_height_um
    }
    pub fn bottom_layer_count(&self) -> u32 {
        self.bottom_layer_count
    }
    pub fn transition_layers(&self) -> u32 {
        self.transition_layers
    }
    pub fn normal_exposure_sec(&self) -> f32 {
        self.normal_exposure_sec
    }
    pub fn bottom_exposure_sec(&self) -> f32 {
        self.bottom_exposure_sec
    }
    pub fn wait_before_cure_sec(&self) -> f32 {
        self.wait_before_cure_sec
    }
    pub fn wait_before_release_sec(&self) -> f32 {
        self.wait_before_release_sec
    }
    pub fn wait_after_release_sec(&self) -> f32 {
        self.wait_after_release_sec
    }
    pub fn lift_speed_mm_min(&self) -> f32 {
        self.lift_speed_mm_min
    }
    pub fn lift_cycle_sec(&self) -> f32 {
        self.lift_cycle_sec
    }
    pub fn lift_distance_mm(&self) -> f32 {
        self.lift_distance_mm
    }

    /// Retract speed with fallback. Returns the explicit retract_speed_mm_min if
    /// present, otherwise `lift_speed_mm_min`. Legacy recipes without the field
    /// behave as if retract_speed equals lift_speed.
    pub fn retract_speed_mm_min(&self) -> f32 {
        self.retract_speed_mm_min.unwrap_or(self.lift_speed_mm_min)
    }

    /// Whether `retract_speed_mm_min` was set explicitly (vs falling back to
    /// `lift_speed_mm_min`). Useful for `LayerTimingCalculator` diagnostics.
    pub fn has_explicit_retract_speed(&self) -> bool {
        self.retract_speed_mm_min.is_some()
    }

    /// Baseline recipe for a generic standard resin. Values match the recipe previously
    /// baked into `PrinterProfile::generic_msla_4k` (KB-171 RERF exposure finder + typical
    /// MSLA 4K defaults). Migration rationale: same resin as before, same recipe — just
    /// the ownership axis changed.
    pub fn generic_standard() -> Self {
        Self::new(
            50.0, // layer_height_um
            6,    // bottom_layer_count
            3,    // transition_layers (typical RERF transition window)
            2.5,  // normal_exposure_sec
            25.0, // bottom_exposure_sec
            0.5,  // wait_before_cure_sec
            1.0,  // wait_before_release_sec
            0.0,  // wait_after_release_sec
            60.0, // lift_speed_mm_min
            7.5,  // lift_cycle_sec
            5.0,  // lift_distance_mm
            None, // retract_speed_mm_min — falls back to lift_speed_mm_min
        )
        .expect(
            "Recipe::generic_standard factory constants must satisfy Recipe::validate() \
             — all positive finite, all waits >= 0",
        )
    }

    /// Baseline recipe for Elegoo Ceramic Grey V2 on Mars-class printers.
    /// Ceramic-filled resins scatter UV more aggressively (lower Dp, KB-100), so layer
    /// height is reduced and exposure lengthened. Higher viscosity (KB-141) slows peel,
    /// so lift speed is conservative and lift cycle longer.
    pub fn elegoo_ceramic_grey() -> Self {
        Self::new(
            40.0, // layer_height_um — ceramic particles reduce penetration depth
            8,    // bottom_layer_count — heavier resin benefits from more raft
            4,    // transition_layers
            3.2,  // normal_exposure_sec — longer cure for ceramic-filled
            35.0, // bottom_exposure_sec
            0.8,  // wait_before_cure_sec — viscous resin needs settle
            1.5,  // wait_before_release_sec
            0.0,  // wait_after_release_sec
            50.0, // lift_speed_mm_min — slower for viscous resin
            8.5,  // lift_cycle_sec
            5.0,  // lift_distance_mm
            None, // retract_speed_mm_min — falls back to lift_speed_mm_min
        )
        .expect(
            "Recipe::elegoo_ceramic_grey factory constants must satisfy Recipe::validate() \
             — all positive finite, all waits >= 0",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_standard_passes_validation() {
        Recipe::generic_standard()
            .validate()
            .expect("Recipe::generic_standard() factory must satisfy validate()");
    }

    #[test]
    fn elegoo_ceramic_grey_passes_validation() {
        Recipe::elegoo_ceramic_grey()
            .validate()
            .expect("Recipe::elegoo_ceramic_grey() factory must satisfy validate()");
    }

    #[test]
    fn generic_standard_has_expected_defaults() {
        let r = Recipe::generic_standard();
        assert_eq!(r.layer_height_um(), 50.0);
        assert_eq!(r.normal_exposure_sec(), 2.5);
        assert_eq!(r.bottom_exposure_sec(), 25.0);
        assert_eq!(r.bottom_layer_count(), 6);
    }

    #[test]
    fn ceramic_grey_differs_from_generic_on_exposure_and_layer_height() {
        // The whole refactor exists because these values must differ per resin.
        let generic = Recipe::generic_standard();
        let ceramic = Recipe::elegoo_ceramic_grey();
        assert_ne!(
            generic.normal_exposure_sec(),
            ceramic.normal_exposure_sec(),
            "ceramic-filled resin must have a distinct exposure — the bug this refactor fixes"
        );
        assert_ne!(
            generic.layer_height_um(),
            ceramic.layer_height_um(),
            "ceramic-filled resin uses thinner layers (shallower cure depth)"
        );
    }

    #[test]
    fn new_rejects_nan_layer_height() {
        let r = Recipe::new(
            f32::NAN,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            None,
        );
        assert!(r.is_err());
    }

    #[test]
    fn new_rejects_zero_exposure() {
        let r = Recipe::new(50.0, 6, 3, 0.0, 25.0, 0.5, 1.0, 0.0, 60.0, 7.5, 5.0, None);
        assert!(r.is_err());
    }

    #[test]
    fn new_rejects_negative_lift_speed() {
        let r = Recipe::new(50.0, 6, 3, 2.5, 25.0, 0.5, 1.0, 0.0, -1.0, 7.5, 5.0, None);
        assert!(r.is_err());
    }

    #[test]
    fn new_rejects_nan_wait_before_cure() {
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            f32::NAN,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            None,
        );
        assert!(r.is_err());
    }

    #[test]
    fn new_accepts_zero_wait_times() {
        // All three wait fields are legitimate at 0 (no settle).
        let r = Recipe::new(50.0, 6, 3, 2.5, 25.0, 0.0, 0.0, 0.0, 60.0, 7.5, 5.0, None)
            .expect("zero wait times are legitimate");
        assert_eq!(r.wait_before_cure_sec(), 0.0);
    }

    #[test]
    fn new_rejects_negative_wait_before_release() {
        let r = Recipe::new(50.0, 6, 3, 2.5, 25.0, 0.5, -0.1, 0.0, 60.0, 7.5, 5.0, None);
        assert!(r.is_err());
    }

    // --- retract_speed_mm_min tests (ADR-0007 / step 3) ---

    #[test]
    fn retract_speed_defaults_to_lift_speed_when_none() {
        // Factory uses None → accessor should return lift_speed_mm_min (60.0 for generic_standard).
        let r = Recipe::generic_standard();
        assert_eq!(r.retract_speed_mm_min(), r.lift_speed_mm_min());
        assert!(!r.has_explicit_retract_speed());
    }

    #[test]
    fn retract_speed_explicit_overrides_lift_speed() {
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(150.0),
        )
        .expect("explicit retract_speed 150 mm/min is valid");
        assert_eq!(r.retract_speed_mm_min(), 150.0);
        assert_eq!(r.lift_speed_mm_min(), 60.0);
        assert!(r.has_explicit_retract_speed());
    }

    #[test]
    fn new_rejects_zero_retract_speed() {
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(0.0),
        );
        assert!(r.is_err());
    }

    #[test]
    fn new_rejects_negative_retract_speed() {
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(-5.0),
        );
        assert!(r.is_err());
    }

    #[test]
    fn new_rejects_nan_retract_speed() {
        let r = Recipe::new(
            50.0,
            6,
            3,
            2.5,
            25.0,
            0.5,
            1.0,
            0.0,
            60.0,
            7.5,
            5.0,
            Some(f32::NAN),
        );
        assert!(r.is_err());
    }

    // --- Parse-path tests locking NaN rejection through serde deserialization. ---

    fn valid_recipe_toml() -> String {
        r#"
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
        .to_string()
    }

    #[test]
    fn parse_toml_then_validate_accepts_valid() {
        let r: Recipe = toml::from_str(&valid_recipe_toml()).expect("valid Recipe TOML must parse");
        r.validate()
            .expect("valid Recipe TOML must satisfy validate()");
    }

    #[test]
    fn parse_toml_then_validate_rejects_nan_layer_height() {
        let toml_str =
            valid_recipe_toml().replace("layer_height_um = 50.0", "layer_height_um = nan");
        let r: Recipe =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        let err = r
            .validate()
            .expect_err("NaN layer_height_um must fail validate()");
        assert!(err.contains("layer_height_um"), "error names field: {err}");
    }

    #[test]
    fn parse_toml_then_validate_rejects_nan_normal_exposure() {
        let toml_str =
            valid_recipe_toml().replace("normal_exposure_sec = 2.5", "normal_exposure_sec = nan");
        let r: Recipe =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        assert!(r.validate().is_err());
    }

    #[test]
    fn parse_toml_then_validate_rejects_negative_lift_distance() {
        let toml_str =
            valid_recipe_toml().replace("lift_distance_mm = 5.0", "lift_distance_mm = -1.0");
        let r: Recipe =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        assert!(r.validate().is_err());
    }

    #[test]
    fn parse_legacy_toml_without_retract_speed_falls_back_to_lift_speed() {
        // valid_recipe_toml() has no retract_speed_mm_min — legacy-format TOML.
        let r: Recipe = toml::from_str(&valid_recipe_toml())
            .expect("legacy Recipe TOML must parse via #[serde(default)]");
        r.validate().expect("legacy Recipe must validate");
        assert_eq!(r.retract_speed_mm_min(), r.lift_speed_mm_min());
        assert!(!r.has_explicit_retract_speed());
    }

    #[test]
    fn parse_toml_with_explicit_retract_speed() {
        let toml_str = valid_recipe_toml() + "retract_speed_mm_min = 180.0\n";
        let r: Recipe = toml::from_str(&toml_str).expect("explicit retract_speed must parse");
        r.validate()
            .expect("explicit valid retract_speed must validate");
        assert_eq!(r.retract_speed_mm_min(), 180.0);
        assert!(r.has_explicit_retract_speed());
    }

    #[test]
    fn parse_toml_with_negative_retract_speed_rejected_at_validate() {
        let toml_str = valid_recipe_toml() + "retract_speed_mm_min = -1.0\n";
        let r: Recipe =
            toml::from_str(&toml_str).expect("TOML parse succeeds; validate() is the gate");
        assert!(r.validate().is_err());
    }

    // --- Validate-on-mutation contract demonstration (per docs/patterns/entity-validate-on-mutation.md). ---

    #[test]
    fn validate_after_mutation_contract() {
        let mut r = Recipe::generic_standard();
        r.validate().expect("baseline recipe must be valid");
        r.normal_exposure_sec = f32::NAN;
        assert!(
            r.validate().is_err(),
            "validate() must be re-called after intra-crate field mutation; \
             NaN normal_exposure_sec should now be rejected"
        );
    }

    // ADR-0005 §1 Axis 2b: Recipe deliberately does NOT derive Default — construction is
    // explicit. Rust stable lacks negative trait bounds, so we cannot assert the absence of
    // `Default` in a test. The intent is documented at the struct-level rustdoc and
    // preserved by review discipline; a future PR that adds `#[derive(Default)]` should
    // cite an ADR update.
}
