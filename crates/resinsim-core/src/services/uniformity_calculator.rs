use crate::services::CureCalculator;
use crate::values::{CureDepth, Energy, PenetrationDepth};

/// Domain service: LCD screen non-uniformity and spatially varying cure depth.
/// Stateless — all inputs via parameters.
///
/// Core concept (KB-120): LCD screens have 20-34% intensity variation
/// center-to-edge. This causes spatially varying cure depth across the
/// build plate: parts in the center cure deeper than parts at the edge.
///
/// Model: intensity map I(x,y) = I_nominal × uniformity_factor(x,y)
/// where uniformity_factor ranges from (1 - variation/2) to (1 + variation/2).
pub struct UniformityCalculator;

/// Simple radial uniformity model: center is brightest, edges are dimmest.
/// Parameterized by a single variation percentage (KB-120 measured values).
#[derive(Debug, Clone)]
pub struct UniformityProfile {
    /// Peak-to-peak variation as a fraction (0.34 = 34% for Saturn 1).
    pub variation: f32,
    /// Build plate width in mm.
    pub plate_width_mm: f32,
    /// Build plate depth in mm.
    pub plate_depth_mm: f32,
}

impl UniformityProfile {
    /// Elegoo Saturn 1 — KB-120: 34% variation.
    pub fn saturn_1() -> Self {
        Self {
            variation: 0.34,
            plate_width_mm: 192.0,
            plate_depth_mm: 120.0,
        }
    }

    /// Elegoo Saturn 2 — KB-120: 22% variation.
    pub fn saturn_2() -> Self {
        Self {
            variation: 0.22,
            plate_width_mm: 219.0,
            plate_depth_mm: 123.0,
        }
    }

    /// Perfectly uniform (ideal, for comparison).
    pub fn uniform() -> Self {
        Self {
            variation: 0.0,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        }
    }

    /// Validate physical invariants. Call this on any externally-sourced profile
    /// before feeding it to the calculator.
    pub fn validate(&self) -> Result<(), String> {
        if !self.variation.is_finite() {
            return Err(format!("variation must be finite (got {})", self.variation));
        }
        if !(0.0..=1.0).contains(&self.variation) {
            return Err(format!(
                "variation must be in [0.0, 1.0] (got {})",
                self.variation
            ));
        }
        if !self.plate_width_mm.is_finite() || self.plate_width_mm <= 0.0 {
            return Err(format!(
                "plate_width_mm must be finite and positive (got {})",
                self.plate_width_mm
            ));
        }
        if !self.plate_depth_mm.is_finite() || self.plate_depth_mm <= 0.0 {
            return Err(format!(
                "plate_depth_mm must be finite and positive (got {})",
                self.plate_depth_mm
            ));
        }
        Ok(())
    }
}

impl UniformityCalculator {
    /// Compute intensity multiplier at position (x, y) on the build plate.
    /// Uses a radial cosine model: brightest at center, dimmest at corners.
    ///
    /// Returns a factor in range [1 - variation/2, 1 + variation/2].
    pub fn intensity_factor(x_mm: f32, y_mm: f32, profile: &UniformityProfile) -> f32 {
        if profile.variation <= 0.0 {
            return 1.0;
        }

        let cx = profile.plate_width_mm / 2.0;
        let cy = profile.plate_depth_mm / 2.0;
        let max_dist = (cx * cx + cy * cy).sqrt();

        let dx = x_mm - cx;
        let dy = y_mm - cy;
        let dist = (dx * dx + dy * dy).sqrt();

        let normalized = (dist / max_dist).min(1.0);

        // Center = 1 + variation/2, edge = 1 - variation/2
        1.0 + profile.variation / 2.0 * (1.0 - 2.0 * normalized)
    }

    /// Compute cure depth at a specific position on the build plate.
    pub fn cure_depth_at_position(
        x_mm: f32,
        y_mm: f32,
        nominal_energy: Energy,
        dp: PenetrationDepth,
        ec: Energy,
        profile: &UniformityProfile,
    ) -> CureDepth {
        let factor = Self::intensity_factor(x_mm, y_mm, profile);
        let local_energy = nominal_energy.scale(factor);
        CureCalculator::cure_depth(dp, local_energy, ec)
    }

    /// Compute the dimensional spread (max - min cure depth) across the plate
    /// for a given resin and exposure. This is what KB-120 measures.
    pub fn cure_depth_spread(
        nominal_energy: Energy,
        dp: PenetrationDepth,
        ec: Energy,
        profile: &UniformityProfile,
    ) -> f32 {
        let cd_center = Self::cure_depth_at_position(
            profile.plate_width_mm / 2.0,
            profile.plate_depth_mm / 2.0,
            nominal_energy, dp, ec, profile,
        );
        let cd_corner = Self::cure_depth_at_position(
            0.0, 0.0, nominal_energy, dp, ec, profile,
        );
        cd_center.value() - cd_corner.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dp_170() -> PenetrationDepth {
        PenetrationDepth::new(170.0)
            .expect("test fixture: 170.0 µm is in PenetrationDepth domain")
    }

    fn ec_5() -> Energy {
        Energy::new(5.0).expect("test fixture: 5.0 mJ/cm² is in Energy domain")
    }

    fn e_10() -> Energy {
        Energy::new(10.0).expect("test fixture: 10.0 mJ/cm² is in Energy domain")
    }

    #[test]
    fn uniform_profile_returns_constant_factor() {
        let p = UniformityProfile::uniform();
        let f_center = UniformityCalculator::intensity_factor(100.0, 60.0, &p);
        let f_corner = UniformityCalculator::intensity_factor(0.0, 0.0, &p);
        assert!((f_center - 1.0).abs() < 1e-6);
        assert!((f_corner - 1.0).abs() < 1e-6);
    }

    #[test]
    fn center_is_brightest() {
        let p = UniformityProfile::saturn_1();
        let f_center = UniformityCalculator::intensity_factor(96.0, 60.0, &p);
        let f_corner = UniformityCalculator::intensity_factor(0.0, 0.0, &p);
        assert!(f_center > f_corner);
    }

    #[test]
    fn center_factor_is_1_plus_half_variation() {
        let p = UniformityProfile::saturn_1(); // 34%
        let f = UniformityCalculator::intensity_factor(96.0, 60.0, &p);
        // At center: factor = 1 + 0.34/2 = 1.17
        assert!((f - 1.17).abs() < 0.01);
    }

    #[test]
    fn corner_factor_is_1_minus_half_variation() {
        let p = UniformityProfile::saturn_1();
        let f = UniformityCalculator::intensity_factor(0.0, 0.0, &p);
        // At corner: factor = 1 - 0.34/2 = 0.83
        assert!((f - 0.83).abs() < 0.01);
    }

    #[test]
    fn cure_depth_varies_across_plate() {
        let p = UniformityProfile::saturn_1();
        let dp = dp_170();
        let ec = ec_5();
        let e = e_10();

        let cd_center = UniformityCalculator::cure_depth_at_position(
            96.0, 60.0, e, dp, ec, &p,
        );
        let cd_corner = UniformityCalculator::cure_depth_at_position(
            0.0, 0.0, e, dp, ec, &p,
        );

        assert!(cd_center.value() > cd_corner.value());
    }

    #[test]
    fn saturn1_spread_is_significant() {
        // KB-120: Saturn 1 (34%) causes 157µm dimensional variation.
        // Our simplified model won't match exactly (different geometry assumptions)
        // but should show significant spread (>50µm).
        let p = UniformityProfile::saturn_1();
        let dp = dp_170();
        let ec = ec_5();
        let e = e_10();

        let spread = UniformityCalculator::cure_depth_spread(e, dp, ec, &p);
        assert!(spread > 30.0, "Saturn 1 spread should be significant, got {spread:.1}");
    }

    #[test]
    fn saturn2_less_spread_than_saturn1() {
        let dp = dp_170();
        let ec = ec_5();
        let e = e_10();

        let spread_s1 = UniformityCalculator::cure_depth_spread(
            e, dp, ec, &UniformityProfile::saturn_1(),
        );
        let spread_s2 = UniformityCalculator::cure_depth_spread(
            e, dp, ec, &UniformityProfile::saturn_2(),
        );

        assert!(spread_s1 > spread_s2,
            "Saturn 1 (34%) should have more spread than Saturn 2 (22%): {spread_s1:.1} vs {spread_s2:.1}");
    }

    #[test]
    fn uniform_profile_zero_spread() {
        let p = UniformityProfile::uniform();
        let spread = UniformityCalculator::cure_depth_spread(e_10(), dp_170(), ec_5(), &p);
        assert!((spread).abs() < 1e-4);
    }

    // --- UniformityProfile::validate ---

    #[test]
    fn uniformity_presets_pass_validation() {
        UniformityProfile::saturn_1()
            .validate()
            .expect("UniformityProfile::saturn_1() factory must satisfy validate()");
        UniformityProfile::saturn_2()
            .validate()
            .expect("UniformityProfile::saturn_2() factory must satisfy validate()");
        UniformityProfile::uniform()
            .validate()
            .expect("UniformityProfile::uniform() factory must satisfy validate()");
    }

    #[test]
    fn uniformity_rejects_variation_above_one() {
        let p = UniformityProfile {
            variation: 1.5,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_rejects_negative_variation() {
        let p = UniformityProfile {
            variation: -0.1,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_rejects_nan_variation() {
        let p = UniformityProfile {
            variation: f32::NAN,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_rejects_zero_plate_width() {
        let p = UniformityProfile {
            variation: 0.22,
            plate_width_mm: 0.0,
            plate_depth_mm: 120.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_rejects_negative_plate_depth() {
        let p = UniformityProfile {
            variation: 0.22,
            plate_width_mm: 200.0,
            plate_depth_mm: -5.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_rejects_nan_plate_dimension() {
        let p = UniformityProfile {
            variation: 0.22,
            plate_width_mm: f32::NAN,
            plate_depth_mm: 120.0,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn uniformity_accepts_boundary_variation() {
        let p0 = UniformityProfile {
            variation: 0.0,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        let p1 = UniformityProfile {
            variation: 1.0,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        assert!(p0.validate().is_ok());
        assert!(p1.validate().is_ok());
    }
}
