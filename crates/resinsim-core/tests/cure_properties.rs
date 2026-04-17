use proptest::prelude::*;
use resinsim_core::services::CureCalculator;
use resinsim_core::values::{Energy, PenetrationDepth};

proptest! {
    /// KB-103: Cure depth is positive iff Energy > Critical Energy.
    #[test]
    fn cure_depth_positive_iff_overcured(
        dp in 40.0f32..600.0,
        ec in 0.5f32..30.0,
        ratio in 1.01f32..100.0,
    ) {
        let energy = Energy::new(ec * ratio).unwrap();
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), energy, Energy::new(ec).unwrap());
        prop_assert!(cd.value() > 0.0, "E > Ec should give positive cure depth, got {}", cd.value());
    }

    /// KB-103: Cure depth is negative when Energy < Critical Energy.
    #[test]
    fn cure_depth_negative_when_undercured(
        dp in 40.0f32..600.0,
        ec in 1.0f32..30.0,
        ratio in 0.01f32..0.99,
    ) {
        let energy = Energy::new(ec * ratio).unwrap();
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), energy, Energy::new(ec).unwrap());
        prop_assert!(cd.value() < 0.0, "E < Ec should give negative cure depth, got {}", cd.value());
    }

    /// KB-103: Cure depth is exactly zero when E = Ec.
    #[test]
    fn cure_depth_zero_at_threshold(
        dp in 40.0f32..600.0,
        ec in 0.5f32..30.0,
    ) {
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), Energy::new(ec).unwrap(), Energy::new(ec).unwrap());
        prop_assert!((cd.value()).abs() < 1e-4, "E = Ec should give ~zero, got {}", cd.value());
    }

    /// Cure depth increases monotonically with energy (at fixed Dp, Ec).
    #[test]
    fn cure_depth_monotonic_with_energy(
        dp in 40.0f32..600.0,
        ec in 0.5f32..30.0,
        e1 in 1.0f32..50.0,
        e2 in 1.0f32..50.0,
    ) {
        let cd1 = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), Energy::new(e1).unwrap(), Energy::new(ec).unwrap());
        let cd2 = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), Energy::new(e2).unwrap(), Energy::new(ec).unwrap());
        if e1 < e2 {
            prop_assert!(cd1.value() <= cd2.value(), "more energy should give deeper cure");
        }
    }

    /// Cure depth scales linearly with Dp (at fixed E/Ec ratio).
    #[test]
    fn cure_depth_scales_with_dp(
        dp1 in 40.0f32..300.0,
        dp2 in 40.0f32..300.0,
        ec in 0.5f32..30.0,
        ratio in 1.1f32..10.0,
    ) {
        let energy = Energy::new(ec * ratio).unwrap();
        let cd1 = CureCalculator::cure_depth(PenetrationDepth::new(dp1).unwrap(), energy, Energy::new(ec).unwrap());
        let cd2 = CureCalculator::cure_depth(PenetrationDepth::new(dp2).unwrap(), energy, Energy::new(ec).unwrap());
        // Cd = Dp × ln(E/Ec), so Cd1/Dp1 = Cd2/Dp2
        let normalized1 = cd1.value() / dp1;
        let normalized2 = cd2.value() / dp2;
        prop_assert!((normalized1 - normalized2).abs() < 1e-4,
            "Cd/Dp should be constant: {} vs {}", normalized1, normalized2);
    }

    /// KB-103: Intensity at depth z is always <= surface intensity.
    #[test]
    fn intensity_never_exceeds_surface(
        i0 in 0.1f32..30.0,
        z in 0.0f32..1000.0,
        dp in 40.0f32..600.0,
    ) {
        let i = CureCalculator::intensity_at_depth(i0, z, PenetrationDepth::new(dp).unwrap());
        prop_assert!(i <= i0 + 1e-6, "intensity at depth should be <= surface: {} > {}", i, i0);
        prop_assert!(i >= 0.0, "intensity cannot be negative: {}", i);
    }

    /// Intensity at z=0 equals surface intensity exactly.
    #[test]
    fn intensity_at_surface_equals_i0(
        i0 in 0.1f32..30.0,
        dp in 40.0f32..600.0,
    ) {
        let i = CureCalculator::intensity_at_depth(i0, 0.0, PenetrationDepth::new(dp).unwrap());
        prop_assert!((i - i0).abs() < 1e-5, "at surface: {} != {}", i, i0);
    }

    /// Intensity decreases monotonically with depth.
    #[test]
    fn intensity_decreases_with_depth(
        i0 in 1.0f32..30.0,
        z1 in 0.0f32..500.0,
        z2 in 0.0f32..500.0,
        dp in 40.0f32..600.0,
    ) {
        let i1 = CureCalculator::intensity_at_depth(i0, z1, PenetrationDepth::new(dp).unwrap());
        let i2 = CureCalculator::intensity_at_depth(i0, z2, PenetrationDepth::new(dp).unwrap());
        if z1 < z2 {
            prop_assert!(i1 >= i2 - 1e-6, "deeper should be dimmer: z1={z1} i1={i1}, z2={z2} i2={i2}");
        }
    }
}
