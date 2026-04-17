use proptest::prelude::*;
use resinsim_core::services::uniformity_calculator::{UniformityCalculator, UniformityProfile};
use resinsim_core::values::{Energy, PenetrationDepth};

proptest! {
    /// Intensity factor is always positive.
    #[test]
    fn intensity_factor_positive(
        x in 0.0f32..300.0,
        y in 0.0f32..200.0,
        variation in 0.0f32..0.5,
    ) {
        let profile = UniformityProfile {
            variation,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        let f = UniformityCalculator::intensity_factor(x, y, &profile);
        prop_assert!(f > 0.0, "intensity factor must be positive: {f}");
    }

    /// Intensity factor is bounded: [1 - v/2, 1 + v/2].
    #[test]
    fn intensity_factor_bounded(
        x in 0.0f32..300.0,
        y in 0.0f32..200.0,
        variation in 0.0f32..0.5,
    ) {
        let profile = UniformityProfile {
            variation,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        let f = UniformityCalculator::intensity_factor(x, y, &profile);
        let lo = 1.0 - variation / 2.0;
        let hi = 1.0 + variation / 2.0;
        prop_assert!(f >= lo - 0.01 && f <= hi + 0.01,
            "factor {f} outside [{lo}, {hi}] for variation={variation}");
    }

    /// Center is always brightest (for variation > 0).
    #[test]
    fn center_brightest(
        variation in 0.01f32..0.5,
        width in 50.0f32..300.0,
        depth in 50.0f32..200.0,
    ) {
        let profile = UniformityProfile {
            variation,
            plate_width_mm: width,
            plate_depth_mm: depth,
        };
        let f_center = UniformityCalculator::intensity_factor(width / 2.0, depth / 2.0, &profile);
        let f_corner = UniformityCalculator::intensity_factor(0.0, 0.0, &profile);
        prop_assert!(f_center > f_corner,
            "center ({f_center}) should be brighter than corner ({f_corner})");
    }

    /// Zero variation → factor = 1.0 everywhere.
    #[test]
    fn zero_variation_is_uniform(
        x in 0.0f32..300.0,
        y in 0.0f32..200.0,
    ) {
        let profile = UniformityProfile {
            variation: 0.0,
            plate_width_mm: 200.0,
            plate_depth_mm: 120.0,
        };
        let f = UniformityCalculator::intensity_factor(x, y, &profile);
        prop_assert!((f - 1.0).abs() < 1e-6);
    }

    /// Cure depth spread increases with variation.
    #[test]
    fn spread_increases_with_variation(
        v1 in 0.01f32..0.25,
        v2 in 0.01f32..0.25,
    ) {
        let dp = PenetrationDepth::new(170.0).unwrap();
        let ec = Energy::new(5.0).unwrap();
        let e = Energy::new(10.0).unwrap();

        let p1 = UniformityProfile { variation: v1, plate_width_mm: 200.0, plate_depth_mm: 120.0 };
        let p2 = UniformityProfile { variation: v2, plate_width_mm: 200.0, plate_depth_mm: 120.0 };

        let spread1 = UniformityCalculator::cure_depth_spread(e, dp, ec, &p1);
        let spread2 = UniformityCalculator::cure_depth_spread(e, dp, ec, &p2);

        if v1 < v2 {
            prop_assert!(spread1 <= spread2 + 0.1,
                "more variation should give more spread: v1={v1} s1={spread1}, v2={v2} s2={spread2}");
        }
    }

    /// Cure depth spread is non-negative.
    #[test]
    fn spread_non_negative(
        variation in 0.0f32..0.5,
        dp in 40.0f32..600.0,
        ec in 0.5f32..30.0,
        energy in 1.0f32..50.0,
    ) {
        let profile = UniformityProfile { variation, plate_width_mm: 200.0, plate_depth_mm: 120.0 };
        let spread = UniformityCalculator::cure_depth_spread(
            Energy::new(energy).unwrap(), PenetrationDepth::new(dp).unwrap(), Energy::new(ec).unwrap(), &profile,
        );
        prop_assert!(spread >= -0.01, "spread should be non-negative: {spread}");
    }
}
