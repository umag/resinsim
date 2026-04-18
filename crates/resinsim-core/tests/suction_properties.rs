use proptest::prelude::*;
use resinsim_core::services::suction_detector::SuctionDetector;
use resinsim_core::values::CrossSectionArea;

proptest! {
    /// Constant-area layers (solid geometry) never produce suction risks.
    #[test]
    fn constant_area_no_suction(
        area in 1.0f64..10000.0,
        n_layers in 10usize..200,
    ) {
        let areas = vec![CrossSectionArea::new(area).expect("proptest strategy produces positive finite mm²"); n_layers];
        let risks = SuctionDetector::detect_from_areas(&areas, None);
        prop_assert!(risks.is_empty(),
            "constant area should never flag suction, got {} risks", risks.len());
    }

    /// Monotonically increasing areas (growing solid) never produce suction risks
    /// (no sharp drops).
    #[test]
    fn increasing_area_no_suction(
        start in 10.0f64..100.0,
        step in 0.1f64..5.0,
        n_layers in 10usize..100,
    ) {
        let areas: Vec<CrossSectionArea> = (0..n_layers)
            .map(|i| CrossSectionArea::new(start + step * i as f64).expect("proptest strategy: start + step × i is positive finite mm²"))
            .collect();
        let risks = SuctionDetector::detect_from_areas(&areas, None);
        prop_assert!(risks.is_empty(),
            "increasing area should never flag suction");
    }

    /// When outer_area equals solid_area (no hollows), no suction detected.
    #[test]
    fn no_hollow_no_suction(
        area in 10.0f64..5000.0,
        n_layers in 5usize..50,
    ) {
        let solid = vec![CrossSectionArea::new(area).expect("proptest strategy produces positive finite mm²"); n_layers];
        let outer = vec![CrossSectionArea::new(area).expect("proptest strategy produces positive finite mm²"); n_layers]; // same = no hollow
        let risks = SuctionDetector::detect_from_areas(&solid, Some(&outer));
        prop_assert!(risks.is_empty(),
            "no hollow region should produce no suction");
    }

    /// Suction force is always non-negative.
    #[test]
    fn suction_force_non_negative(
        base_area in 50.0f64..500.0,
        wall_fraction in 0.05f64..0.45,
    ) {
        let n = 20;
        let wall_area = base_area * wall_fraction;

        let mut solid = vec![CrossSectionArea::new(base_area).expect("proptest strategy 50..500 mm² produces valid CrossSectionArea"); n];
        let mut outer = vec![CrossSectionArea::new(base_area).expect("proptest strategy 50..500 mm² produces valid CrossSectionArea"); n];
        // Transition to ring at layer 5
        for i in 5..n {
            solid[i] = CrossSectionArea::new(wall_area).expect("proptest strategy: base × 0.05..0.45 is positive finite mm²");
            outer[i] = CrossSectionArea::new(base_area).expect("proptest strategy 50..500 mm² produces valid CrossSectionArea");
        }

        let risks = SuctionDetector::detect_from_areas(&solid, Some(&outer));
        for r in &risks {
            prop_assert!(r.suction_force_n >= 0.0,
                "suction force must be non-negative: {}", r.suction_force_n);
            prop_assert!(r.sealed_area_mm2 >= 0.0,
                "sealed area must be non-negative: {}", r.sealed_area_mm2);
        }
    }

    /// Larger cavities produce larger suction forces.
    #[test]
    fn larger_cavity_more_suction(
        base1 in 100.0f64..300.0,
        base2 in 300.0f64..600.0,
    ) {
        let n = 15;
        let wall = 30.0; // fixed wall area

        let make_cup = |base: f64| {
            let mut solid = vec![CrossSectionArea::new(base).expect("proptest strategy: base is positive finite mm²"); n];
            let mut outer = vec![CrossSectionArea::new(base).expect("proptest strategy: base is positive finite mm²"); n];
            for i in 5..n {
                solid[i] = CrossSectionArea::new(wall).expect("test fixture: wall=30.0 mm² is in CrossSectionArea domain");
                outer[i] = CrossSectionArea::new(base).expect("proptest strategy: base is positive finite mm²");
            }
            SuctionDetector::detect_from_areas(&solid, Some(&outer))
        };

        let risks1 = make_cup(base1);
        let risks2 = make_cup(base2);

        if !risks1.is_empty() && !risks2.is_empty() {
            prop_assert!(risks2[0].suction_force_n >= risks1[0].suction_force_n,
                "larger cavity should produce more suction: {} vs {}",
                risks2[0].suction_force_n, risks1[0].suction_force_n);
        }
    }
}
