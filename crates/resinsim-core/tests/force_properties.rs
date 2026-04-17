use proptest::prelude::*;
use resinsim_core::services::PeelForceCalculator;
use resinsim_core::values::{CrossSectionArea, PeelForce, SafetyFactor, SupportCapacity};

proptest! {
    /// KB-114: Peel force is monotonically increasing with area.
    #[test]
    fn peel_force_monotonic_with_area(
        sigma in 1.0f32..50.0,
        a1 in 0.0f64..10000.0,
        a2 in 0.0f64..10000.0,
    ) {
        let f1 = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(a1).unwrap(), 1.0);
        let f2 = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(a2).unwrap(), 1.0);
        if a1 <= a2 {
            prop_assert!(f1.value() <= f2.value() + 1e-6, "more area should give more force");
        }
    }

    /// Peel force is non-negative for non-negative inputs.
    #[test]
    fn peel_force_non_negative(
        sigma in 0.0f32..50.0,
        area in 0.0f64..10000.0,
        speed_factor in 0.5f32..5.0,
    ) {
        let f = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(area).unwrap(), speed_factor);
        prop_assert!(f.value() >= 0.0, "force should be non-negative: {}", f.value());
    }

    /// KB-114: Peel force scales linearly with area (at fixed sigma, speed).
    #[test]
    fn peel_force_linear_with_area(
        sigma in 1.0f32..50.0,
        area in 1.0f64..10000.0,
        factor in 1.0f64..10.0,
    ) {
        let f1 = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(area).unwrap(), 1.0);
        let f2 = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(area * factor).unwrap(), 1.0);
        let ratio = f2.value() as f64 / f1.value() as f64;
        prop_assert!((ratio - factor).abs() < 0.01,
            "force should scale linearly: ratio {} vs factor {}", ratio, factor);
    }

    /// KB-114: Safety factor = capacity / force (invariant).
    #[test]
    fn safety_factor_is_ratio(
        cap in 0.1f32..1000.0,
        force in 0.1f32..1000.0,
    ) {
        let sf = SafetyFactor::compute(SupportCapacity::new(cap).unwrap(), PeelForce::new(force).unwrap());
        let expected = cap / force;
        prop_assert!((sf.value() - expected).abs() < 1e-4,
            "SF should be cap/force: {} vs {}", sf.value(), expected);
    }

    /// Safety factor > 1 iff capacity > force.
    #[test]
    fn safety_factor_safe_iff_capacity_exceeds_force(
        cap in 0.1f32..1000.0,
        force in 0.1f32..1000.0,
    ) {
        let sf = SafetyFactor::compute(SupportCapacity::new(cap).unwrap(), PeelForce::new(force).unwrap());
        if cap > force {
            prop_assert!(sf.is_safe(), "cap > force should be safe: cap={cap}, force={force}, sf={}", sf.value());
        }
    }

    /// KB-112: Lift speed factor >= 1.0 when speed >= ref_speed.
    #[test]
    fn speed_factor_ge_one_for_faster(
        speed in 1.0f32..1000.0,
        ref_speed in 1.0f32..1000.0,
    ) {
        let f = PeelForceCalculator::lift_speed_factor(speed, ref_speed);
        if speed >= ref_speed {
            prop_assert!(f >= 1.0 - 1e-6, "faster should give factor >= 1: speed={speed}, ref={ref_speed}, f={f}");
        }
    }

    /// Lift speed factor increases monotonically with speed.
    #[test]
    fn speed_factor_monotonic(
        s1 in 1.0f32..500.0,
        s2 in 1.0f32..500.0,
        ref_speed in 1.0f32..100.0,
    ) {
        let f1 = PeelForceCalculator::lift_speed_factor(s1, ref_speed);
        let f2 = PeelForceCalculator::lift_speed_factor(s2, ref_speed);
        if s1 <= s2 {
            prop_assert!(f1 <= f2 + 1e-6, "faster speed should give higher factor");
        }
    }

    /// Support capacity scales linearly with number of supports.
    #[test]
    fn support_capacity_linear_with_count(
        sigma in 10.0f32..80.0,
        radius in 0.1f32..0.5,
        n1 in 1u32..50,
        n2 in 1u32..50,
    ) {
        let c1 = PeelForceCalculator::support_capacity(sigma, radius, n1);
        let c2 = PeelForceCalculator::support_capacity(sigma, radius, n2);
        let ratio = c2.value() / c1.value();
        let expected = n2 as f32 / n1 as f32;
        prop_assert!((ratio - expected).abs() < 0.01,
            "capacity should scale with count: ratio {} vs expected {}", ratio, expected);
    }

    /// Support capacity is non-negative.
    #[test]
    fn support_capacity_non_negative(
        sigma in 0.0f32..80.0,
        radius in 0.0f32..1.0,
        n in 0u32..100,
    ) {
        let c = PeelForceCalculator::support_capacity(sigma, radius, n);
        prop_assert!(c.value() >= 0.0, "capacity should be non-negative: {}", c.value());
    }
}
