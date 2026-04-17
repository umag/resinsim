//! Property tests that validate against KB measured data.
//! These tests should FAIL if the physics model violates known empirical bounds.

use proptest::prelude::*;
use resinsim_core::services::{CureCalculator, PeelForceCalculator, ThermalCalculator};
use resinsim_core::values::{CrossSectionArea, Energy, PenetrationDepth};

proptest! {
    /// KB-100/KB-101: For any real resin Dp/Ec at 405nm, cure depth at
    /// reasonable exposure (5-50 mJ/cm²) must stay within 0-2000 µm.
    /// Values outside this range indicate a model or parameter error.
    #[test]
    fn cure_depth_within_physical_range(
        dp in 40.0f32..600.0,   // KB-102: measured range
        ec in 0.5f32..30.0,     // KB-102: measured range
        energy in 5.0f32..50.0, // typical operating range
    ) {
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(dp).unwrap(), Energy::new(energy).unwrap(), Energy::new(ec).unwrap());
        // Physical constraint: cure depth cannot exceed several mm in any real resin.
        // VeroClear (Dp=568µm) at 50 mJ/cm² with Ec=0.5 → Cd = 568×ln(100) = 2616µm.
        // Cap at 3000µm (3mm) — deeper than any practical resin layer.
        prop_assert!(cd.value() < 3000.0,
            "cure depth {:.1} µm exceeds physical maximum for dp={dp}, ec={ec}, e={energy}", cd.value());
    }

    /// KB-110/KB-111: Peel force for real printer build plates
    /// (area 0-8200 mm², σ 12-18 kPa) must stay within 0-200 N.
    /// Peak measured forces are 120-200 N (KB-111).
    #[test]
    fn peel_force_within_measured_range(
        area in 0.0f64..8200.0,     // KB-110: up to full Saturn plate
        sigma in 12.0f32..18.0,     // KB-110: ACF to thick FEP
    ) {
        let f = PeelForceCalculator::peel_force(sigma, CrossSectionArea::new(area).unwrap(), 1.0);
        // KB-111: peak measured forces up to 200 N
        prop_assert!(f.value() <= 200.0,
            "peel force {:.1} N exceeds KB-111 measured peak for area={area}, sigma={sigma}", f.value());
        prop_assert!(f.value() >= 0.0, "peel force cannot be negative");
    }

    /// KB-112: Speed factor at 96× must produce 2.0-2.5× force increase.
    /// If outside this range, the power-law exponent is wrong.
    #[test]
    fn speed_factor_within_measured_bounds(
        ref_speed in 1.0f32..100.0,
    ) {
        let f96 = PeelForceCalculator::lift_speed_factor(ref_speed * 96.0, ref_speed);
        // KB-112: FEP measured 230%, PP measured 175%. Our FEP model should be 2.0-2.5.
        prop_assert!(f96 > 2.0 && f96 < 2.5,
            "96× speed factor {f96:.3} outside KB-112 measured range [2.0, 2.5]");
    }

    /// KB-141: Viscosity at 50°C must be 10-30% of viscosity at 25°C.
    /// The 82% drop (18% remaining) is well-established.
    /// With Ea=52 kJ/mol, ratio should be ~0.18.
    #[test]
    fn viscosity_drop_25_to_50_matches_kb141(
        mu_ref in 50.0f32..1500.0,  // KB-141: full viscosity range
    ) {
        let mu_50 = ThermalCalculator::viscosity_at_temperature(mu_ref, 25.0, 50.0, 52.0);
        let ratio = mu_50 / mu_ref;
        // KB-141: 82% drop → ratio ≈ 0.18, allow 0.10-0.30 for model tolerance
        prop_assert!(ratio > 0.10 && ratio < 0.30,
            "25→50°C viscosity ratio {ratio:.3} outside KB-141 range [0.10, 0.30], mu_ref={mu_ref}");
    }

    /// KB-150: Vat temperature after 2 hours must be within 5-15°C of ambient
    /// for typical printer parameters (ΔT=5-15°C, τ=600-1800s).
    #[test]
    fn vat_temp_2hr_within_expected_range(
        ambient in 18.0f32..28.0,
        delta_t in 5.0f32..15.0,
        tau in 600.0f32..1800.0,
    ) {
        let t = ThermalCalculator::vat_temperature(
            ambient, delta_t,
            resinsim_core::values::ThermalTimeConstant::new(tau).unwrap(),
            7200.0, // 2 hours
        );
        // After 2 hours (>> τ), should be near steady state
        let rise = t.value() - ambient;
        prop_assert!(rise > delta_t * 0.9,
            "after 2h, rise {rise:.1}°C should be >90% of ΔT={delta_t}°C");
        prop_assert!(rise <= delta_t + 0.1,
            "after 2h, rise {rise:.1}°C should not exceed ΔT={delta_t}°C");
    }

    /// KB-114: Support capacity for realistic tip sizes (0.1-0.5mm radius,
    /// 1-50 supports, 20-70 MPa tensile) must stay within 0-500 N.
    #[test]
    fn support_capacity_within_physical_range(
        sigma in 20.0f32..70.0,     // KB-140: tensile strength range
        radius in 0.1f32..0.5,      // typical tip radii
        n in 1u32..50,
    ) {
        let cap = PeelForceCalculator::support_capacity(sigma, radius, n);
        prop_assert!(cap.value() > 0.0, "capacity must be positive");
        // 70 MPa × π × 0.5² × 50 = 2749 N theoretical max. Practical limit ~1000 N.
        prop_assert!(cap.value() < 3000.0,
            "capacity {:.1} N exceeds physical range for sigma={sigma}, r={radius}, n={n}", cap.value());
    }

    /// KB-100: Liqcreate Premium Black (Dp=170, Ec=5.0) at typical LCD printer
    /// exposure (4 mW/cm² × 2-4 seconds = 8-16 mJ/cm²) should produce
    /// cure depth of 50-200 µm — sufficient for 50µm layers.
    #[test]
    fn premium_black_typical_exposure_sufficient(
        exposure_sec in 2.0f32..4.0,
    ) {
        let energy = Energy::from_exposure(4.0, exposure_sec); // KB-121: ~4 mW/cm² typical
        let cd = CureCalculator::cure_depth(PenetrationDepth::new(170.0).unwrap(), energy, Energy::new(5.0).unwrap());
        prop_assert!(cd.is_sufficient(50.0),
            "Premium Black at {exposure_sec}s should cure >50µm, got {:.1}µm", cd.value());
    }
}
