use proptest::prelude::*;
use resinsim_core::services::z_axis_compensator::ZDeflectionSeverity;
use resinsim_core::services::ZAxisCompensator;
use resinsim_core::values::PeelForce;

proptest! {
    /// KB-131: Deflection is non-negative for non-negative force.
    #[test]
    fn deflection_non_negative(
        force in 0.0f32..500.0,
        k in 100.0f32..5000.0,
    ) {
        let dz = ZAxisCompensator::deflection_um(PeelForce::new(force).expect("proptest strategy produces non-negative finite N"), k);
        prop_assert!(dz >= 0.0, "deflection should be non-negative: {}", dz);
    }

    /// Deflection increases monotonically with force.
    #[test]
    fn deflection_monotonic_with_force(
        f1 in 0.0f32..500.0,
        f2 in 0.0f32..500.0,
        k in 100.0f32..5000.0,
    ) {
        let dz1 = ZAxisCompensator::deflection_um(PeelForce::new(f1).expect("proptest strategy 0..500 N produces valid PeelForce"), k);
        let dz2 = ZAxisCompensator::deflection_um(PeelForce::new(f2).expect("proptest strategy 0..500 N produces valid PeelForce"), k);
        if f1 <= f2 {
            prop_assert!(dz1 <= dz2 + 1e-4, "more force should give more deflection");
        }
    }

    /// Deflection decreases with stiffer axis (higher k).
    #[test]
    fn deflection_decreases_with_stiffness(
        force in 1.0f32..200.0,
        k1 in 100.0f32..2500.0,
        k2 in 100.0f32..2500.0,
    ) {
        let dz1 = ZAxisCompensator::deflection_um(PeelForce::new(force).expect("proptest strategy produces non-negative finite N"), k1);
        let dz2 = ZAxisCompensator::deflection_um(PeelForce::new(force).expect("proptest strategy produces non-negative finite N"), k2);
        if k1 <= k2 {
            prop_assert!(dz1 >= dz2 - 1e-4, "stiffer axis should deflect less");
        }
    }

    /// Effective layer height = commanded - deflection.
    #[test]
    fn effective_height_is_commanded_minus_deflection(
        commanded in 20.0f32..200.0,
        deflection in 0.0f32..500.0,
    ) {
        let h = ZAxisCompensator::effective_layer_height_um(commanded, deflection);
        prop_assert!((h - (commanded - deflection)).abs() < 1e-4,
            "h_eff should be commanded - deflection");
    }

    /// Severity is catastrophic iff effective height < 0.
    #[test]
    fn catastrophic_iff_negative_height(
        commanded in 20.0f32..200.0,
        deflection in 0.0f32..500.0,
    ) {
        let severity = ZAxisCompensator::severity(commanded, deflection);
        let effective = commanded - deflection;
        if effective < 0.0 {
            prop_assert_eq!(severity, ZDeflectionSeverity::Catastrophic);
        }
        if severity == ZDeflectionSeverity::Normal {
            prop_assert!(effective >= commanded * 0.5 - 1e-4);
        }
    }

    /// Derived stiffness round-trips: derive k from F,dz then recompute dz.
    #[test]
    fn stiffness_roundtrip(
        force in 1.0f32..200.0,
        k_original in 100.0f32..5000.0,
    ) {
        let dz = ZAxisCompensator::deflection_um(PeelForce::new(force).expect("proptest strategy produces non-negative finite N"), k_original);
        if dz > 0.1 { // avoid divide-by-near-zero
            let k_derived = ZAxisCompensator::derive_stiffness(force, dz);
            prop_assert!((k_derived - k_original).abs() < 1.0,
                "roundtrip stiffness: {} vs {}", k_derived, k_original);
        }
    }
}
