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
        let energy = Energy::new(ec * ratio).expect("proptest strategy: ec × positive ratio produces finite positive Energy");
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"), energy, Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
        prop_assert!(cd.value() > 0.0, "E > Ec should give positive cure depth, got {}", cd.value());
    }

    /// KB-103: Cure depth is negative when Energy < Critical Energy.
    #[test]
    fn cure_depth_negative_when_undercured(
        dp in 40.0f32..600.0,
        ec in 1.0f32..30.0,
        ratio in 0.01f32..0.99,
    ) {
        let energy = Energy::new(ec * ratio).expect("proptest strategy: ec × positive ratio produces finite positive Energy");
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"), energy, Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
        prop_assert!(cd.value() < 0.0, "E < Ec should give negative cure depth, got {}", cd.value());
    }

    /// KB-103: Cure depth is exactly zero when E = Ec.
    #[test]
    fn cure_depth_zero_at_threshold(
        dp in 40.0f32..600.0,
        ec in 0.5f32..30.0,
    ) {
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"), Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"), Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
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
        let cd1 = CureCalculator::cure_depth(PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"), Energy::new(e1).expect("proptest strategy 1..50 mJ/cm² produces valid Energy"), Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
        let cd2 = CureCalculator::cure_depth(PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"), Energy::new(e2).expect("proptest strategy 1..50 mJ/cm² produces valid Energy"), Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
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
        let energy = Energy::new(ec * ratio).expect("proptest strategy: ec × positive ratio produces finite positive Energy");
        let cd1 = CureCalculator::cure_depth(PenetrationDepth::new(dp1).expect("proptest strategy 40..300 µm produces valid PenetrationDepth"), energy, Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
        let cd2 = CureCalculator::cure_depth(PenetrationDepth::new(dp2).expect("proptest strategy 40..300 µm produces valid PenetrationDepth"), energy, Energy::new(ec).expect("proptest strategy 0.5..30 mJ/cm² produces valid Energy"));
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
        let i = CureCalculator::intensity_at_depth(i0, z, PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"));
        prop_assert!(i <= i0 + 1e-6, "intensity at depth should be <= surface: {} > {}", i, i0);
        prop_assert!(i >= 0.0, "intensity cannot be negative: {}", i);
    }

    /// Intensity at z=0 equals surface intensity exactly.
    #[test]
    fn intensity_at_surface_equals_i0(
        i0 in 0.1f32..30.0,
        dp in 40.0f32..600.0,
    ) {
        let i = CureCalculator::intensity_at_depth(i0, 0.0, PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"));
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
        let i1 = CureCalculator::intensity_at_depth(i0, z1, PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"));
        let i2 = CureCalculator::intensity_at_depth(i0, z2, PenetrationDepth::new(dp).expect("proptest strategy 40..600 µm produces valid PenetrationDepth"));
        if z1 < z2 {
            prop_assert!(i1 >= i2 - 1e-6, "deeper should be dimmer: z1={z1} i1={i1}, z2={z2} i2={i2}");
        }
    }

    // --- KB-153: Arrhenius-corrected Ec(T) properties ----------------------

    /// Ec(T = T_ref) = Ec_ref ⇒ cure_depth_at_temp delegates to cure_depth.
    /// Ranges cover useful operating envelope without overflow corners.
    #[test]
    fn ec_at_ref_temp_equals_ec_ref(
        dp in 40.0f32..600.0,
        ec_ref in 1.0f32..50.0,
        e_ratio in 1.01f32..20.0,
        ref_temp in 15.0f32..35.0,
        ea in 5.0f32..100.0,
    ) {
        let energy = Energy::new(ec_ref * e_ratio)
            .expect("proptest: ec_ref × positive ratio produces valid Energy");
        let baseline = CureCalculator::cure_depth(
            PenetrationDepth::new(dp).expect("proptest 40..600 µm"),
            energy,
            Energy::new(ec_ref).expect("proptest 1..50 mJ/cm²"),
        );
        let at_ref = CureCalculator::cure_depth_at_temp(
            PenetrationDepth::new(dp).expect("proptest 40..600 µm"),
            energy,
            Energy::new(ec_ref).expect("proptest 1..50 mJ/cm²"),
            ref_temp,
            ref_temp,
            ea,
        );
        let rel_err = (at_ref.value() - baseline.value()).abs()
            / baseline.value().abs().max(1e-4);
        prop_assert!(rel_err < 1e-4,
            "T = T_ref should give Cd = Cd_baseline: {} vs {} (rel {})",
            at_ref.value(), baseline.value(), rel_err);
    }

    /// Monotonic: T2 > T1 ⇒ Cd(T2) > Cd(T1). At fixed exposure, warmer ⇒
    /// lower Ec ⇒ deeper cure.
    #[test]
    fn ec_decreases_with_temperature(
        dp in 40.0f32..600.0,
        ec_ref in 1.0f32..50.0,
        e_ratio in 1.1f32..20.0,
        ref_temp in 15.0f32..35.0,
        t1 in 10.0f32..40.0,
        delta in 1.0f32..35.0,
        ea in 5.0f32..100.0,
    ) {
        let t2 = t1 + delta;
        // Bound t2 to the property's useful range; proptest strategies can overshoot.
        prop_assume!(t2 <= 80.0);
        let energy = Energy::new(ec_ref * e_ratio)
            .expect("proptest: ec_ref × positive ratio produces valid Energy");
        let dp_v = PenetrationDepth::new(dp).expect("proptest 40..600 µm");
        let ec_v = Energy::new(ec_ref).expect("proptest 1..50 mJ/cm²");
        let cd1 = CureCalculator::cure_depth_at_temp(dp_v, energy, ec_v, ref_temp, t1, ea);
        let cd2 = CureCalculator::cure_depth_at_temp(dp_v, energy, ec_v, ref_temp, t2, ea);
        prop_assert!(cd2.value() >= cd1.value() - 1e-4,
            "warmer vat should give deeper (or equal) cure: T1={t1} Cd1={}, T2={t2} Cd2={}",
            cd1.value(), cd2.value());
    }

    /// Arrhenius is linear in 1/T, so symmetry holds in 1/T-space:
    /// given T1, derive T2 from 1/T2 = 2/T_ref - 1/T1. Then
    /// Ec(T1) × Ec(T2) = Ec_ref², equivalently
    /// (Ec_ref - Cd1 factor) × (Ec_ref - Cd2 factor) = Ec_ref².
    /// Direct test: Ec(T1) × Ec(T2) / Ec_ref² ≈ 1.
    #[test]
    fn ec_arrhenius_symmetric_in_inverse_temp(
        ec_ref in 1.0f32..50.0,
        ref_temp_c in 15.0f32..35.0,
        t1_c in 10.0f32..60.0,
        ea in 5.0f32..100.0,
    ) {
        const R: f32 = 8.314;
        let t_ref_k = ref_temp_c + 273.15;
        let t1_k = t1_c + 273.15;
        // Arrhenius symmetry: 1/T2 = 2/T_ref - 1/T1.
        let t2_k_inv = 2.0 / t_ref_k - 1.0 / t1_k;
        prop_assume!(t2_k_inv > 0.0);
        let t2_k = 1.0 / t2_k_inv;
        prop_assume!(t2_k > 273.15 - 20.0 && t2_k < 273.15 + 100.0);
        // Compute Ec(T1), Ec(T2) directly via the Arrhenius formula to isolate
        // the math from Beer-Lambert arithmetic.
        let ea_j = ea * 1000.0;
        let ec_t1 = ec_ref * ((ea_j / R) * (1.0 / t1_k - 1.0 / t_ref_k)).exp();
        let ec_t2 = ec_ref * ((ea_j / R) * (1.0 / t2_k - 1.0 / t_ref_k)).exp();
        let product = ec_t1 * ec_t2;
        let ref_sq = ec_ref * ec_ref;
        let rel_err = (product - ref_sq).abs() / ref_sq;
        prop_assert!(rel_err < 1e-3,
            "Arrhenius 1/T-space symmetry: Ec(T1) × Ec(T2) = Ec_ref² failed: \
             T1_K={t1_k} T2_K={t2_k} product={product} ref²={ref_sq} rel={rel_err}");
    }
}
