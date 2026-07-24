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
        PeelForce::new(peel_adhesion_kpa * area.value() as f32 * 1e-3 * lift_speed_factor)
            .expect("peel adhesion, area, and speed factor are non-negative by construction")
    }

    /// Suction force from a sealed cavity against FEP.
    /// KB-114: F_suction = ΔP (kPa) × A_sealed (mm²) → Newtons.
    /// Max ΔP ≈ 101 kPa (atmospheric pressure).
    pub fn suction_force(
        pressure_differential_kpa: f32,
        sealed_area: CrossSectionArea,
    ) -> PeelForce {
        PeelForce::new(pressure_differential_kpa * sealed_area.value() as f32 * 1e-3)
            .expect("pressure differential and area are non-negative by construction")
    }

    /// First-layer base-adhesion force (KB-116 oxygen-freshness σ-relaxation).
    /// An elevated release-layer adhesion Δσ₀ (kPa) at layer 0 relaxes
    /// exponentially over `relaxation_layers` (τ) toward zero, area-scaled:
    ///   F_base = Δσ₀ · exp(-layer/τ) · A · 1e-3 → Newtons.
    /// Kept SEPARATE from [`peel_force`](Self::peel_force) so the KB-114 test
    /// vectors stay valid. ADR-0022 Stage 1. Zero when Δσ₀=0, area=0, or τ<=0.
    pub fn base_adhesion_force(
        base_elevation_kpa: f32,
        area: CrossSectionArea,
        layer: u32,
        relaxation_layers: f32,
    ) -> PeelForce {
        if base_elevation_kpa <= 0.0 || relaxation_layers <= 0.0 {
            return PeelForce::new(0.0).expect("zero is a non-negative finite PeelForce");
        }
        // Exponential relaxation of the elevated release-layer adhesion:
        // full Δσ₀ at layer 0, ×(1/e) at layer == τ, →0 for deep layers.
        let decay = (-(layer as f32) / relaxation_layers).exp();
        PeelForce::new(base_elevation_kpa * area.value() as f32 * 1e-3 * decay)
            .expect("elevation, area, and decay factor are non-negative by construction")
    }

    /// Aspect-ratio shape factor for the peel term (ADR-0022 Stage 3, KB-185
    /// Tier-1). Modulates σ_peel by the layer's compactness: a dimensionless
    /// factor in `(0, 1]`, `1.0` for a square (the KB-181 baseline
    /// cross-section) and `< 1.0` for thin / high-aspect-ratio shapes. Both
    /// separation regimes rank thin below compact at equal area — Stefan suction
    /// scales with A/L, and Kendall peel with the instantaneous crack-front
    /// width (KB-185).
    ///
    /// Square-anchored and reduction-only:
    /// ```text
    ///   raw    = 4·√A / L                     (=1 square, >1 disk, <1 thin)
    ///   factor = 1 − strength·(1 − min(1, raw))
    /// ```
    /// `strength ∈ [0, 1]` interpolates between no correction (`0`) and the full
    /// square-anchored reduction (`1`); `strength = 0.5` reproduces the Pan
    /// Fig.9 314 mm² cylinder→star force ratio (≈0.795). The `min(1, raw)` clamp
    /// is reduction-only: shapes MORE compact than a square (a disk, `raw ≈
    /// 1.13`) floor to `1.0` — the disk>square gradient is intentionally dropped
    /// for this Tier-1 factor. Returns `1.0` (no correction) when `strength ≤
    /// 0`, `perimeter_mm ≤ 0`, or `area == 0`, so an unset resin strength is
    /// behaviour-preserving. Kept OUT of [`peel_force`](Self::peel_force) so the
    /// KB-114 vectors + the `force_properties` area-linearity proptest stay valid.
    pub fn peel_shape_factor(area: CrossSectionArea, perimeter_mm: f32, strength: f32) -> f32 {
        if strength <= 0.0 || perimeter_mm <= 0.0 {
            return 1.0;
        }
        let a = area.value() as f32;
        if a <= 0.0 {
            return 1.0;
        }
        let raw = 4.0 * a.sqrt() / perimeter_mm;
        let clamped = raw.min(1.0);
        1.0 - strength * (1.0 - clamped)
    }

    /// Total separation force: peel adhesion + suction + first-layer base
    /// adhesion (ADR-0022 Stage 1). All three components are non-negative.
    pub fn total_force(peel: PeelForce, suction: PeelForce, base: PeelForce) -> PeelForce {
        PeelForce::new(peel.value() + suction.value() + base.value())
            .expect("sum of three non-negative finite values is non-negative finite")
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
        SupportCapacity::new(tensile_strength_mpa * area_per_tip * n_supports as f32)
            .expect("product of non-negative tensile strength, area, and count is non-negative")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(mm2: f64) -> CrossSectionArea {
        CrossSectionArea::new(mm2)
            .expect("test fixture: non-negative finite mm² is in CrossSectionArea domain")
    }

    fn peel(n: f32) -> PeelForce {
        PeelForce::new(n).expect("test fixture: non-negative finite N is in PeelForce domain")
    }

    // --- KB-114 peel force test vectors ---

    #[test]
    fn peel_force_50mm_square_standard_fep() {
        // KB-114: σ=13 kPa, A=2500 mm², f(v)=1.0 → 32.5 N
        let f = PeelForceCalculator::peel_force(13.0, area(2500.0), 1.0);
        assert!((f.value() - 32.5).abs() < 0.01);
    }

    #[test]
    fn peel_force_small_cross_section() {
        // KB-114: σ=13, A=100 mm², f(v)=1.0 → 1.3 N
        let f = PeelForceCalculator::peel_force(13.0, area(100.0), 1.0);
        assert!((f.value() - 1.3).abs() < 0.01);
    }

    #[test]
    fn peel_force_full_saturn_plate() {
        // KB-114: σ=13, A=8160 mm² (120×68mm), f(v)=1.0 → 106.08 N
        let f = PeelForceCalculator::peel_force(13.0, area(8160.0), 1.0);
        assert!((f.value() - 106.08).abs() < 0.01);
    }

    #[test]
    fn peel_force_thick_fep() {
        // KB-114: σ=18, A=2500, f(v)=1.0 → 45.0 N
        let f = PeelForceCalculator::peel_force(18.0, area(2500.0), 1.0);
        assert!((f.value() - 45.0).abs() < 0.01);
    }

    #[test]
    fn peel_force_acf_film() {
        // KB-114: σ=12 (ACF), A=2500, f(v)=1.0 → 30.0 N
        let f = PeelForceCalculator::peel_force(12.0, area(2500.0), 1.0);
        assert!((f.value() - 30.0).abs() < 0.01);
    }

    #[test]
    fn peel_force_with_fast_lift() {
        // KB-114: σ=13, A=2500, f(v)=2.3 → 74.75 N
        let f = PeelForceCalculator::peel_force(13.0, area(2500.0), 2.3);
        assert!((f.value() - 74.75).abs() < 0.01);
    }

    #[test]
    fn peel_force_zero_area_is_zero() {
        let f = PeelForceCalculator::peel_force(13.0, area(0.0), 1.0);
        assert!((f.value()).abs() < 1e-6);
    }

    // --- KB-114 suction force test vectors ---

    #[test]
    fn suction_force_sealed_10mm_cup() {
        // KB-114: ΔP=101, A=100 mm² → 10.1 N
        let f = PeelForceCalculator::suction_force(101.0, area(100.0));
        assert!((f.value() - 10.1).abs() < 0.01);
    }

    #[test]
    fn suction_force_sealed_30mm_cup() {
        // KB-114: ΔP=101, A=706 mm² → 71.306 N
        let f = PeelForceCalculator::suction_force(101.0, area(706.0));
        assert!((f.value() - 71.306).abs() < 0.01);
    }

    #[test]
    fn suction_force_drained_cup_is_zero() {
        // KB-114: ΔP=0, A=706 → 0 N
        let f = PeelForceCalculator::suction_force(0.0, area(706.0));
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
    fn total_force_combines_peel_suction_and_base() {
        let peel_f = peel(32.5);
        let suction = peel(10.1);
        let base = peel(4.0);
        let total = PeelForceCalculator::total_force(peel_f, suction, base);
        assert!((total.value() - 46.6).abs() < 0.01);
    }

    // --- KB-116 base adhesion σ-relaxation (ADR-0022 Stage 1) ---

    #[test]
    fn base_adhesion_layer0_is_full_elevation() {
        // Δσ₀=20 kPa, A=500 mm², layer 0, τ=5 → 20·500·1e-3·exp(0) = 10.0 N.
        let f = PeelForceCalculator::base_adhesion_force(20.0, area(500.0), 0, 5.0);
        assert!((f.value() - 10.0).abs() < 1e-4, "got {}", f.value());
    }

    #[test]
    fn base_adhesion_decays_to_one_over_e_at_tau() {
        // layer == τ → factor exp(-1) ≈ 0.3679.
        let f = PeelForceCalculator::base_adhesion_force(20.0, area(500.0), 5, 5.0);
        let expected = 20.0 * 500.0 * 1e-3 * (-1.0f32).exp();
        assert!(
            (f.value() - expected).abs() < 1e-4,
            "got {} expected {expected}",
            f.value()
        );
    }

    #[test]
    fn base_adhesion_zero_when_elevation_zero() {
        assert_eq!(
            PeelForceCalculator::base_adhesion_force(0.0, area(500.0), 0, 5.0).value(),
            0.0
        );
    }

    #[test]
    fn base_adhesion_zero_when_area_zero() {
        assert_eq!(
            PeelForceCalculator::base_adhesion_force(20.0, area(0.0), 0, 5.0).value(),
            0.0
        );
    }

    #[test]
    fn base_adhesion_zero_when_tau_non_positive() {
        assert_eq!(
            PeelForceCalculator::base_adhesion_force(20.0, area(500.0), 0, 0.0).value(),
            0.0
        );
    }

    #[test]
    fn base_adhesion_non_increasing_in_layer() {
        let a = area(500.0);
        let f0 = PeelForceCalculator::base_adhesion_force(20.0, a, 0, 5.0).value();
        let f2 = PeelForceCalculator::base_adhesion_force(20.0, a, 2, 5.0).value();
        let f10 = PeelForceCalculator::base_adhesion_force(20.0, a, 10, 5.0).value();
        assert!(f0 >= f2 && f2 >= f10 && f0 > f10, "f0={f0} f2={f2} f10={f10}");
    }

    // --- KB-185 A/L peel shape factor (ADR-0022 Stage 3) ---

    #[test]
    fn shape_factor_square_is_one() {
        // 10×10 square: A=100, L=40 → raw = 4·10/40 = 1.0 → factor 1.0.
        let f = PeelForceCalculator::peel_shape_factor(area(100.0), 40.0, 1.0);
        assert!((f - 1.0).abs() < 1e-6, "got {f}");
    }

    #[test]
    fn shape_factor_thin_rectangle_below_one() {
        // 4:1 rectangle: A=4, L=10 → raw = 4·2/10 = 0.8 → factor 0.8 at full strength.
        let f = PeelForceCalculator::peel_shape_factor(area(4.0), 10.0, 1.0);
        assert!((f - 0.8).abs() < 1e-6, "got {f}");
    }

    #[test]
    fn shape_factor_zero_strength_is_one() {
        let f = PeelForceCalculator::peel_shape_factor(area(4.0), 10.0, 0.0);
        assert_eq!(f, 1.0);
    }

    #[test]
    fn shape_factor_zero_perimeter_is_one() {
        let f = PeelForceCalculator::peel_shape_factor(area(4.0), 0.0, 1.0);
        assert_eq!(f, 1.0);
    }

    #[test]
    fn shape_factor_zero_area_is_one() {
        let f = PeelForceCalculator::peel_shape_factor(area(0.0), 10.0, 1.0);
        assert_eq!(f, 1.0);
    }

    #[test]
    fn shape_factor_disk_clamps_at_one() {
        // Disk of A=100: L = 2·√(π·100) ≈ 35.45 → raw ≈ 1.128 → clamp to 1.0.
        let disk_perimeter = 2.0 * (std::f32::consts::PI * 100.0).sqrt();
        let f = PeelForceCalculator::peel_shape_factor(area(100.0), disk_perimeter, 1.0);
        assert!((f - 1.0).abs() < 1e-6, "disk should clamp to 1.0, got {f}");
    }

    #[test]
    fn shape_factor_monotonic_in_aspect_ratio() {
        // 4:1 (A=4,L=10) vs 9:1 (A=9,L=20): thinner → smaller factor.
        let r4 = PeelForceCalculator::peel_shape_factor(area(4.0), 10.0, 1.0);
        let r9 = PeelForceCalculator::peel_shape_factor(area(9.0), 20.0, 1.0);
        assert!(r9 < r4 && r4 < 1.0, "r4={r4} r9={r9}");
    }

    #[test]
    fn shape_factor_strength_half_matches_pan_star_ratio() {
        // Pan Fig.9 star: A=314 mm², A/L=2.58 → L≈121.7 mm. At strength 0.5 the
        // factor ≈ 0.79, matching the measured star/cylinder force ratio 4.9/6.16 = 0.795.
        let f = PeelForceCalculator::peel_shape_factor(area(314.0), 121.7, 0.5);
        assert!((f - 0.791).abs() < 0.005, "got {f}");
        assert!((f - 0.795).abs() < 0.01, "should track Pan's 0.795, got {f}");
    }
}
