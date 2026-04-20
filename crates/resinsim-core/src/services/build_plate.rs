use crate::values::CrossSectionArea;

/// Domain service: build plate adhesion model.
/// Stateless — all inputs via parameters.
///
/// The build plate provides holding force for the first N layers (bottom layers)
/// via mechanical interlock with the textured plate surface.
/// After bottom layers, adhesion comes from interlayer curing bond only.
///
/// Adhesion capacity model:
///   F_plate = σ_plate × A_contact
///
/// where σ_plate is the plate adhesion strength (kPa), typically much higher
/// than the FEP peel adhesion because bottom layers get 3-10× exposure and
/// the textured plate provides mechanical interlock.
pub struct BuildPlate;

/// Build plate adhesion profile.
#[derive(Debug, Clone)]
pub struct PlateAdhesionProfile {
    /// Adhesion strength of the textured build plate. Unit: kPa.
    /// Typically 50-200 kPa (much higher than FEP's 12-18 kPa).
    /// Depends on plate texture condition, resin, and bottom exposure.
    pub plate_adhesion_kpa: f32,

    /// Number of bottom layers with extra exposure.
    pub bottom_layer_count: u32,

    /// Interlayer bond strength for normal layers. Unit: kPa.
    /// This is the adhesion between consecutive cured layers.
    /// Typically 30-100 kPa depending on exposure and resin.
    pub interlayer_bond_kpa: f32,
}

impl PlateAdhesionProfile {
    /// Conservative defaults for a textured plate in good condition.
    pub fn default_textured() -> Self {
        Self {
            plate_adhesion_kpa: 100.0, // textured plate, well-leveled
            bottom_layer_count: 6,
            interlayer_bond_kpa: 50.0, // typical interlayer at normal exposure
        }
    }

    /// Validate physical invariants. Call this on any externally-sourced profile
    /// before feeding it to the predictor.
    pub fn validate(&self) -> Result<(), String> {
        if !self.plate_adhesion_kpa.is_finite() || self.plate_adhesion_kpa < 0.0 {
            return Err(format!(
                "plate_adhesion_kpa must be finite and non-negative (got {})",
                self.plate_adhesion_kpa
            ));
        }
        if !self.interlayer_bond_kpa.is_finite() || self.interlayer_bond_kpa < 0.0 {
            return Err(format!(
                "interlayer_bond_kpa must be finite and non-negative (got {})",
                self.interlayer_bond_kpa
            ));
        }
        Ok(())
    }
}

impl BuildPlate {
    /// Holding capacity from build plate adhesion at a given layer.
    /// Bottom layers: plate adhesion × contact area.
    /// Normal layers: interlayer bond × contact area (if no supports).
    /// Returns force in Newtons.
    ///
    /// Note: kPa × mm² = 1e3 Pa × 1e-6 m² = 1e-3 N
    pub fn holding_capacity(
        layer: u32,
        area: CrossSectionArea,
        profile: &PlateAdhesionProfile,
    ) -> f32 {
        let sigma_kpa = if layer < profile.bottom_layer_count {
            profile.plate_adhesion_kpa
        } else {
            profile.interlayer_bond_kpa
        };
        sigma_kpa * area.value() as f32 * 1e-3
    }

    /// Total holding capacity: build plate/interlayer + supports.
    /// This is what the FailurePredictor should use instead of just support capacity.
    pub fn total_capacity(plate_capacity_n: f32, support_capacity_n: f32) -> f32 {
        // Plate adhesion and supports act in parallel
        plate_capacity_n + support_capacity_n
    }

    /// Whether this layer is a bottom layer (uses plate adhesion).
    pub fn is_bottom_layer(layer: u32, profile: &PlateAdhesionProfile) -> bool {
        layer < profile.bottom_layer_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> PlateAdhesionProfile {
        PlateAdhesionProfile::default_textured()
    }

    fn area(mm2: f64) -> CrossSectionArea {
        CrossSectionArea::new(mm2)
            .expect("test fixture: non-negative finite mm² is in CrossSectionArea domain")
    }

    // --- Bottom layer adhesion ---

    #[test]
    fn bottom_layer_uses_plate_adhesion() {
        // Layer 0 (bottom): σ=100 kPa, A=2500 mm² → 250 N
        let cap = BuildPlate::holding_capacity(0, area(2500.0), &profile());
        assert!((cap - 250.0).abs() < 0.01);
    }

    #[test]
    fn all_bottom_layers_use_plate_adhesion() {
        let p = profile();
        for layer in 0..p.bottom_layer_count {
            let cap = BuildPlate::holding_capacity(layer, area(1000.0), &p);
            // 100 kPa × 1000 mm² = 100 N
            assert!(
                (cap - 100.0).abs() < 0.01,
                "layer {layer} should use plate adhesion"
            );
        }
    }

    #[test]
    fn normal_layer_uses_interlayer_bond() {
        // Layer 10 (normal): σ=50 kPa, A=2500 mm² → 125 N
        let cap = BuildPlate::holding_capacity(10, area(2500.0), &profile());
        assert!((cap - 125.0).abs() < 0.01);
    }

    #[test]
    fn transition_at_bottom_boundary() {
        let p = profile();
        let last_bottom = BuildPlate::holding_capacity(p.bottom_layer_count - 1, area(1000.0), &p);
        let first_normal = BuildPlate::holding_capacity(p.bottom_layer_count, area(1000.0), &p);
        // Bottom: 100 kPa, Normal: 50 kPa → bottom has 2× adhesion
        assert!(last_bottom > first_normal);
        assert!((last_bottom / first_normal - 2.0).abs() < 0.01);
    }

    // --- 50mm cube scenario (from user test) ---

    #[test]
    fn cube_50mm_no_supports_bottom_layers_hold() {
        // 50mm cube: A=2500 mm², peel force = 32.5 N
        // Bottom plate adhesion: 100 kPa × 2500 mm² = 250 N >> 32.5 N
        let plate_cap = BuildPlate::holding_capacity(0, area(2500.0), &profile());
        let total = BuildPlate::total_capacity(plate_cap, 0.0); // no supports
        assert!(
            total > 32.5,
            "plate adhesion ({total} N) should hold 32.5 N peel force"
        );
    }

    #[test]
    fn cube_50mm_no_supports_normal_layers_hold() {
        // Normal layers: interlayer bond 50 kPa × 2500 mm² = 125 N > 32.5 N
        let interlayer_cap = BuildPlate::holding_capacity(50, area(2500.0), &profile());
        let total = BuildPlate::total_capacity(interlayer_cap, 0.0);
        assert!(
            total > 32.5,
            "interlayer bond ({total} N) should hold 32.5 N peel force"
        );
    }

    #[test]
    fn total_capacity_combines_plate_and_supports() {
        let plate = 100.0;
        let supports = 50.0;
        let total = BuildPlate::total_capacity(plate, supports);
        assert!((total - 150.0).abs() < 1e-6);
    }

    // --- Zero area ---

    #[test]
    fn zero_area_zero_adhesion() {
        let cap = BuildPlate::holding_capacity(0, area(0.0), &profile());
        assert!((cap).abs() < 1e-6);
    }

    #[test]
    fn is_bottom_layer_check() {
        let p = profile();
        assert!(BuildPlate::is_bottom_layer(0, &p));
        assert!(BuildPlate::is_bottom_layer(5, &p));
        assert!(!BuildPlate::is_bottom_layer(6, &p));
        assert!(!BuildPlate::is_bottom_layer(100, &p));
    }

    // --- PlateAdhesionProfile::validate ---

    #[test]
    fn plate_adhesion_default_passes_validation() {
        PlateAdhesionProfile::default_textured()
            .validate()
            .expect("PlateAdhesionProfile::default_textured() factory must satisfy validate()");
    }

    #[test]
    fn plate_adhesion_rejects_nan_plate() {
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: f32::NAN,
            bottom_layer_count: 6,
            interlayer_bond_kpa: 50.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn plate_adhesion_rejects_negative_plate() {
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: -1.0,
            bottom_layer_count: 6,
            interlayer_bond_kpa: 50.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn plate_adhesion_rejects_nan_interlayer() {
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: 100.0,
            bottom_layer_count: 6,
            interlayer_bond_kpa: f32::NAN,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn plate_adhesion_rejects_negative_interlayer() {
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: 100.0,
            bottom_layer_count: 6,
            interlayer_bond_kpa: -0.5,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn plate_adhesion_rejects_infinity() {
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: f32::INFINITY,
            bottom_layer_count: 6,
            interlayer_bond_kpa: 50.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn plate_adhesion_accepts_zero() {
        // Zero means "no build plate adhesion" (e.g. fresh FEP, resin test).
        let p = PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0,
            bottom_layer_count: 0,
            interlayer_bond_kpa: 0.0,
        };
        assert!(p.validate().is_ok());
    }
}
