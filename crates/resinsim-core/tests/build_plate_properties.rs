use proptest::prelude::*;
use resinsim_core::services::build_plate::{BuildPlate, PlateAdhesionProfile};
use resinsim_core::values::CrossSectionArea;

proptest! {
    /// Holding capacity is non-negative for non-negative inputs.
    #[test]
    fn holding_capacity_non_negative(
        layer in 0u32..1000,
        area in 0.0f64..10000.0,
        plate_kpa in 0.0f32..200.0,
        bond_kpa in 0.0f32..100.0,
        bottom_count in 0u32..20,
    ) {
        let profile = PlateAdhesionProfile {
            plate_adhesion_kpa: plate_kpa,
            bottom_layer_count: bottom_count,
            interlayer_bond_kpa: bond_kpa,
        };
        let cap = BuildPlate::holding_capacity(layer, CrossSectionArea::new(area).unwrap(), &profile);
        prop_assert!(cap >= 0.0, "capacity must be non-negative: {cap}");
    }

    /// Holding capacity scales linearly with area.
    #[test]
    fn holding_capacity_linear_with_area(
        layer in 0u32..100,
        area in 1.0f64..5000.0,
        factor in 1.0f64..10.0,
    ) {
        let profile = PlateAdhesionProfile::default_textured();
        let cap1 = BuildPlate::holding_capacity(layer, CrossSectionArea::new(area).unwrap(), &profile);
        let cap2 = BuildPlate::holding_capacity(layer, CrossSectionArea::new(area * factor).unwrap(), &profile);
        if cap1 > 0.0 {
            let ratio = cap2 as f64 / cap1 as f64;
            prop_assert!((ratio - factor).abs() < 0.01,
                "capacity should scale linearly: ratio {ratio} vs factor {factor}");
        }
    }

    /// Bottom layers always have higher or equal capacity than normal layers
    /// (plate adhesion >= interlayer bond in default profile).
    #[test]
    fn bottom_layers_stronger_than_normal(
        area in 1.0f64..10000.0,
    ) {
        let profile = PlateAdhesionProfile::default_textured();
        let bottom_cap = BuildPlate::holding_capacity(0, CrossSectionArea::new(area).unwrap(), &profile);
        let normal_cap = BuildPlate::holding_capacity(
            profile.bottom_layer_count, CrossSectionArea::new(area).unwrap(), &profile,
        );
        prop_assert!(bottom_cap >= normal_cap,
            "bottom should be >= normal: {bottom_cap} vs {normal_cap}");
    }

    /// Total capacity = plate + supports (additive, both non-negative).
    #[test]
    fn total_capacity_additive(
        plate in 0.0f32..500.0,
        supports in 0.0f32..500.0,
    ) {
        let total = BuildPlate::total_capacity(plate, supports);
        prop_assert!((total - (plate + supports)).abs() < 1e-4);
        prop_assert!(total >= plate);
        prop_assert!(total >= supports);
    }

    /// is_bottom_layer transitions exactly at bottom_layer_count.
    #[test]
    fn bottom_layer_boundary(
        bottom_count in 1u32..20,
        layer in 0u32..100,
    ) {
        let profile = PlateAdhesionProfile {
            plate_adhesion_kpa: 100.0,
            bottom_layer_count: bottom_count,
            interlayer_bond_kpa: 50.0,
        };
        let is_bottom = BuildPlate::is_bottom_layer(layer, &profile);
        if layer < bottom_count {
            prop_assert!(is_bottom, "layer {layer} < {bottom_count} should be bottom");
        } else {
            prop_assert!(!is_bottom, "layer {layer} >= {bottom_count} should not be bottom");
        }
    }
}
