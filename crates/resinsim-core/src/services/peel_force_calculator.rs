use crate::values::{CrossSectionArea, PeelForce, SupportCapacity};

/// Domain service: peel force and support capacity calculations.
/// Stateless — all inputs via parameters.
///
/// Core equations (KB-114):
///   F_peel = σ_peel × A_layer × f(v_lift)
///   F_suction = ΔP × A_sealed
///   F_max = σ_tensile × π × r_tip² × N_supports
pub struct PeelForceCalculator;

impl PeelForceCalculator {
    /// Peel force from FEP adhesion (excluding suction).
    /// KB-114: F = σ_peel (kPa) × A (mm²) × f(v) → Newtons.
    /// Note: kPa × mm² = 1e3 Pa × 1e-6 m² = 1e-3 N, so F = σ × A × 1e-3.
    pub fn peel_force(
        peel_adhesion_kpa: f32,
        area: CrossSectionArea,
        lift_speed_factor: f32,
    ) -> PeelForce {
        PeelForce(peel_adhesion_kpa * area.value() as f32 * 1e-3 * lift_speed_factor)
    }

    /// Suction force from a sealed cavity against FEP.
    /// KB-114: F_suction = ΔP (kPa) × A_sealed (mm²) → Newtons.
    /// Max ΔP ≈ 101 kPa (atmospheric pressure).
    pub fn suction_force(pressure_differential_kpa: f32, sealed_area: CrossSectionArea) -> PeelForce {
        PeelForce(pressure_differential_kpa * sealed_area.value() as f32 * 1e-3)
    }

    /// Total force: adhesion peel + suction.
    pub fn total_force(peel: PeelForce, suction: PeelForce) -> PeelForce {
        PeelForce(peel.value() + suction.value())
    }

    /// Lift speed factor using power-law model.
    /// KB-112: f(v) = (v / v_ref)^0.18 for FEP.
    /// v_ref is the reference speed at which σ_peel was measured.
    pub fn lift_speed_factor(speed_mm_min: f32, ref_speed_mm_min: f32) -> f32 {
        if ref_speed_mm_min <= 0.0 || speed_mm_min <= 0.0 {
            return 1.0;
        }
        (speed_mm_min / ref_speed_mm_min).powf(0.182)
    }

    /// Support capacity for N supports with given tip radius and resin tensile strength.
    /// KB-114: F_max = σ_tensile (MPa) × π × r² (mm²) × N → Newtons.
    /// Note: MPa × mm² = 1e6 Pa × 1e-6 m² = 1 N.
    pub fn support_capacity(
        tensile_strength_mpa: f32,
        tip_radius_mm: f32,
        n_supports: u32,
    ) -> SupportCapacity {
        let area_per_tip = std::f32::consts::PI * tip_radius_mm * tip_radius_mm;
        SupportCapacity(tensile_strength_mpa * area_per_tip * n_supports as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- KB-114 peel force test vectors ---

    #[test]
    fn peel_force_50mm_square_standard_fep() {
        // KB-114: σ=13 kPa, A=2500 mm², f(v)=1.0 → 32.5 N
        let f = PeelForceCalculator::peel_force(13.0, CrossSectionArea(2500.0), 1.0);
        assert!((f.value() - 32.5).abs() < 0.01);
    }

    #[test]
    fn peel_force_small_cross_section() {
        // KB-114: σ=13, A=100 mm², f(v)=1.0 → 1.3 N
        let f = PeelForceCalculator::peel_force(13.0, CrossSectionArea(100.0), 1.0);
        assert!((f.value() - 1.3).abs() < 0.01);
    }

    #[test]
    fn peel_force_full_saturn_plate() {
        // KB-114: σ=13, A=8160 mm² (120×68mm), f(v)=1.0 → 106.08 N
        let f = PeelForceCalculator::peel_force(13.0, CrossSectionArea(8160.0), 1.0);
        assert!((f.value() - 106.08).abs() < 0.01);
    }

    #[test]
    fn peel_force_thick_fep() {
        // KB-114: σ=18, A=2500, f(v)=1.0 → 45.0 N
        let f = PeelForceCalculator::peel_force(18.0, CrossSectionArea(2500.0), 1.0);
        assert!((f.value() - 45.0).abs() < 0.01);
    }

    #[test]
    fn peel_force_acf_film() {
        // KB-114: σ=12 (ACF), A=2500, f(v)=1.0 → 30.0 N
        let f = PeelForceCalculator::peel_force(12.0, CrossSectionArea(2500.0), 1.0);
        assert!((f.value() - 30.0).abs() < 0.01);
    }

    #[test]
    fn peel_force_with_fast_lift() {
        // KB-114: σ=13, A=2500, f(v)=2.3 → 74.75 N
        let f = PeelForceCalculator::peel_force(13.0, CrossSectionArea(2500.0), 2.3);
        assert!((f.value() - 74.75).abs() < 0.01);
    }

    #[test]
    fn peel_force_zero_area_is_zero() {
        let f = PeelForceCalculator::peel_force(13.0, CrossSectionArea(0.0), 1.0);
        assert!((f.value()).abs() < 1e-6);
    }

    // --- KB-114 suction force test vectors ---

    #[test]
    fn suction_force_sealed_10mm_cup() {
        // KB-114: ΔP=101, A=100 mm² → 10.1 N
        let f = PeelForceCalculator::suction_force(101.0, CrossSectionArea(100.0));
        assert!((f.value() - 10.1).abs() < 0.01);
    }

    #[test]
    fn suction_force_sealed_30mm_cup() {
        // KB-114: ΔP=101, A=706 mm² → 71.306 N
        let f = PeelForceCalculator::suction_force(101.0, CrossSectionArea(706.0));
        assert!((f.value() - 71.306).abs() < 0.01);
    }

    #[test]
    fn suction_force_drained_cup_is_zero() {
        // KB-114: ΔP=0, A=706 → 0 N
        let f = PeelForceCalculator::suction_force(0.0, CrossSectionArea(706.0));
        assert!((f.value()).abs() < 1e-6);
    }

    // --- KB-112 lift speed factor ---

    #[test]
    fn lift_speed_factor_at_reference_is_one() {
        let f = PeelForceCalculator::lift_speed_factor(60.0, 60.0);
        assert!((f - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lift_speed_factor_96x_is_2_30() {
        // KB-112: 96× speed → 230% force → f(v) = 2.30
        let f = PeelForceCalculator::lift_speed_factor(96.0, 1.0);
        assert!((f - 2.30).abs() < 0.02);
    }

    #[test]
    fn lift_speed_factor_increases_with_speed() {
        let slow = PeelForceCalculator::lift_speed_factor(30.0, 60.0);
        let fast = PeelForceCalculator::lift_speed_factor(120.0, 60.0);
        assert!(fast > slow);
    }

    // --- KB-114 support capacity test vectors ---

    #[test]
    fn support_capacity_single_04mm_tip() {
        // KB-114: σ=30 MPa, r=0.2mm, N=1 → π×0.04×30 = 3.77 N
        let cap = PeelForceCalculator::support_capacity(30.0, 0.2, 1);
        assert!((cap.value() - 3.77).abs() < 0.01);
    }

    #[test]
    fn support_capacity_ten_supports() {
        // KB-114: σ=30, r=0.2, N=10 → 37.7 N
        let cap = PeelForceCalculator::support_capacity(30.0, 0.2, 10);
        assert!((cap.value() - 37.7).abs() < 0.1);
    }

    #[test]
    fn support_capacity_stronger_resin_larger_tips() {
        // KB-114: σ=50, r=0.25, N=5 → 50 × π × 0.0625 × 5 = 49.09 N
        let cap = PeelForceCalculator::support_capacity(50.0, 0.25, 5);
        assert!((cap.value() - 49.09).abs() < 0.1);
    }

    #[test]
    fn support_capacity_many_small_tips() {
        // KB-114: σ=30, r=0.1, N=20 → 30 × π × 0.01 × 20 = 18.85 N
        let cap = PeelForceCalculator::support_capacity(30.0, 0.1, 20);
        assert!((cap.value() - 18.85).abs() < 0.01);
    }

    // --- Total force ---

    #[test]
    fn total_force_combines_peel_and_suction() {
        let peel = PeelForce(32.5);
        let suction = PeelForce(10.1);
        let total = PeelForceCalculator::total_force(peel, suction);
        assert!((total.value() - 42.6).abs() < 0.01);
    }
}
