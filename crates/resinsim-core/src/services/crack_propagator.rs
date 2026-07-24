use crate::values::CrackFront;

/// Domain service: Kendall fracture-limited interlayer-bond knockdown.
/// Stateless — all inputs via parameters.
///
/// # Physics (KB-188 / KB-191 / KB-116 / KB-185 / KB-186)
///
/// Kendall (KB-188): interlayer peel force `F ∝ crack-front width b`, so the
/// resin↔resin bond's effective holding CAPACITY is area-INDEPENDENT. The
/// crack-front-width knockdown normalised to the equal-area SQUARE is
///
/// ```text
///   effective_bonded_fraction = min(1, 4·√A / P)
/// ```
///
/// `= 1` for a square (`P = 4√A`), `< 1` for thin / high-perimeter shapes
/// (walls, edges, cantilevers), and clamped to `1` for shapes MORE compact than
/// a square (a disk, `raw ≈ 1.13`) — the knockdown is reduction-only.
///
/// Anchoring to the existing `interlayer_bond_kpa` at the square reference
/// ABSORBS the unmeasured interlayer fracture line-force `R/(1−cosθ)`, so there
/// is NO new uncalibrated parameter and the model is behaviour-preserving for
/// square/compact layers (knockdown `1`).
///
/// This is the SAME `4√A/L` compactness ADR-0022 Stage 3 applied to the
/// FEP-peel LOAD (`peel_shape_factor`), now applied to the resin↔resin
/// interlayer CAPACITY — a DISTINCT interface (KB-116: weak oxygen-lubricated
/// FEP release vs strong crosslinked interlayer), so it is consistent fracture
/// physics on two interfaces, not double-counting.
///
/// NO `G_c`, NO load, NO threshold: the knockdown is purely geometric. (The
/// KB-191 `1862 J/m²` bilinear-CZM `Gc` is the constrained-surface SEPARATION
/// toughness — the FEP interface, the WRONG interface for the interlayer crack —
/// cited for context only.) The load enters the model ONLY via the Delamination
/// check (reduced interlayer capacity < peel load) and the downstream safety
/// factor. Tier-2 (post-E-series) replaces the square-anchor with a measured
/// interlayer `Gc`/`θ` (KB-190, deferred to harvest).
pub struct CrackPropagator;

impl CrackPropagator {
    /// The remaining bonded fraction after the Kendall crack-front-width
    /// knockdown: `min(1, 4·√area / perimeter_mm)`.
    ///
    /// Returns `1.0` (neutral — no knockdown) when `perimeter_mm <= 0.0` or
    /// `area <= 0.0` (degenerate or placeholder geometry), mirroring the
    /// placeholder guard in `PeelForceCalculator::peel_shape_factor`.
    pub fn effective_bonded_fraction(area: f64, perimeter_mm: f64) -> f64 {
        // Degenerate / placeholder / non-finite geometry ⇒ neutral (no
        // knockdown). The `is_finite` guards keep a stray NaN/∞ from
        // poisoning the capacity — real mask perimeters/areas are finite.
        if !area.is_finite()
            || !perimeter_mm.is_finite()
            || perimeter_mm <= 0.0
            || area <= 0.0
        {
            return 1.0;
        }
        let raw = 4.0 * area.sqrt() / perimeter_mm;
        // Reduction-only clamp: shapes more compact than the equal-area square
        // (a disk, raw ≈ 1.13) floor to 1.0.
        raw.min(1.0)
    }

    /// The crack front implied by the geometry:
    /// `CrackFront(1 − effective_bonded_fraction)`.
    pub fn crack_from_geometry(area: f64, perimeter_mm: f64) -> CrackFront {
        CrackFront::new((1.0 - Self::effective_bonded_fraction(area, perimeter_mm)) as f32)
    }

    /// The effective bonded area = `area · effective_bonded_fraction` (`≤ area`).
    pub fn effective_bonded_area(area: f64, perimeter_mm: f64) -> f64 {
        area * Self::effective_bonded_fraction(area, perimeter_mm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- effective_bonded_fraction: the Kendall crack-front-width knockdown ---

    #[test]
    fn fraction_square_is_one() {
        // 10×10 square: A=100, P=40 → 4·10/40 = 1.0 (behaviour-preserving).
        let f = CrackPropagator::effective_bonded_fraction(100.0, 40.0);
        assert!((f - 1.0).abs() < 1e-9, "square must be 1.0, got {f}");
    }

    #[test]
    fn fraction_thin_rectangle_below_one_exact() {
        // 4:1 rectangle A=4, P=10 → 4·2/10 = 0.8.
        let f = CrackPropagator::effective_bonded_fraction(4.0, 10.0);
        assert!((f - 0.8).abs() < 1e-9, "got {f}");
    }

    #[test]
    fn fraction_thin_wall_matches_4_sqrt_a_over_p() {
        // 2mm × 50mm wall: A=100, P=2·(2+50)=104 → 4·10/104 = 0.384615…
        let f = CrackPropagator::effective_bonded_fraction(100.0, 104.0);
        let expected = 4.0 * 100.0_f64.sqrt() / 104.0;
        assert!((f - expected).abs() < 1e-9, "got {f} expected {expected}");
        assert!(f < 0.4, "thin wall must knock down well below 1.0, got {f}");
    }

    #[test]
    fn fraction_disk_clamps_to_one() {
        // Disk A=100: P = 2·√(π·100) ≈ 35.45 → raw ≈ 1.128 → clamp to 1.0.
        let disk_perimeter = 2.0 * (std::f64::consts::PI * 100.0).sqrt();
        let f = CrackPropagator::effective_bonded_fraction(100.0, disk_perimeter);
        assert!((f - 1.0).abs() < 1e-9, "disk must clamp to 1.0, got {f}");
    }

    #[test]
    fn fraction_zero_perimeter_is_neutral() {
        assert_eq!(CrackPropagator::effective_bonded_fraction(100.0, 0.0), 1.0);
        assert_eq!(CrackPropagator::effective_bonded_fraction(100.0, -5.0), 1.0);
    }

    #[test]
    fn fraction_zero_area_is_neutral() {
        assert_eq!(CrackPropagator::effective_bonded_fraction(0.0, 40.0), 1.0);
        assert_eq!(CrackPropagator::effective_bonded_fraction(-1.0, 40.0), 1.0);
    }

    #[test]
    fn fraction_non_finite_input_is_neutral() {
        // Defensive: real perimeters/areas are finite by construction (validated
        // masks), but a non-finite input must degrade to the neutral 1.0, never
        // NaN-poison the capacity. Pins the contract for NaN / ±∞.
        assert_eq!(
            CrackPropagator::effective_bonded_fraction(100.0, f64::NAN),
            1.0
        );
        assert_eq!(
            CrackPropagator::effective_bonded_fraction(f64::NAN, 40.0),
            1.0
        );
        assert_eq!(
            CrackPropagator::effective_bonded_fraction(100.0, f64::INFINITY),
            1.0
        );
        assert_eq!(
            CrackPropagator::effective_bonded_fraction(f64::INFINITY, 40.0),
            1.0
        );
        // And the derived quantities stay well-defined (no crack, full area).
        assert_eq!(
            CrackPropagator::crack_from_geometry(100.0, f64::NAN).value(),
            0.0
        );
        assert_eq!(
            CrackPropagator::effective_bonded_area(100.0, f64::NAN),
            100.0
        );
    }

    #[test]
    fn fraction_monotonic_in_aspect_ratio() {
        // Thinner (higher P at equal A) → smaller bonded fraction.
        let compact = CrackPropagator::effective_bonded_fraction(100.0, 44.0);
        let thin = CrackPropagator::effective_bonded_fraction(100.0, 104.0);
        assert!(thin < compact && compact <= 1.0, "compact={compact} thin={thin}");
    }

    #[test]
    fn fraction_never_exceeds_one() {
        for (a, p) in [(100.0, 40.0), (100.0, 30.0), (4.0, 10.0), (706.0, 94.0)] {
            let f = CrackPropagator::effective_bonded_fraction(a, p);
            assert!(f <= 1.0 + 1e-12, "fraction must be ≤ 1, got {f} for A={a} P={p}");
            assert!(f > 0.0, "fraction must be > 0 for real geometry, got {f}");
        }
    }

    // --- crack_from_geometry = 1 − effective_bonded_fraction ---

    #[test]
    fn crack_square_is_zero() {
        let c = CrackPropagator::crack_from_geometry(100.0, 40.0);
        assert!((c.value() - 0.0).abs() < 1e-6, "square → no crack, got {}", c.value());
    }

    #[test]
    fn crack_thin_rectangle_is_one_minus_fraction() {
        // A=4, P=10 → fraction 0.8 → crack 0.2.
        let c = CrackPropagator::crack_from_geometry(4.0, 10.0);
        assert!((c.value() - 0.2).abs() < 1e-6, "got {}", c.value());
    }

    #[test]
    fn crack_thin_wall_is_large() {
        // A=100, P=104 → fraction ≈ 0.385 → crack ≈ 0.615.
        let c = CrackPropagator::crack_from_geometry(100.0, 104.0);
        assert!(c.value() > 0.6, "thin wall → large crack, got {}", c.value());
    }

    #[test]
    fn crack_degenerate_geometry_is_zero() {
        assert_eq!(CrackPropagator::crack_from_geometry(100.0, 0.0).value(), 0.0);
        assert_eq!(CrackPropagator::crack_from_geometry(0.0, 40.0).value(), 0.0);
    }

    #[test]
    fn crack_equals_one_minus_fraction_relation() {
        let (a, p) = (100.0, 104.0);
        let frac = CrackPropagator::effective_bonded_fraction(a, p);
        let crack = CrackPropagator::crack_from_geometry(a, p);
        assert!(
            (crack.value() as f64 - (1.0 - frac)).abs() < 1e-6,
            "crack {} should equal 1 − fraction {}",
            crack.value(),
            1.0 - frac
        );
    }

    // --- effective_bonded_area = area · fraction ≤ area ---

    #[test]
    fn bonded_area_square_is_full_area() {
        let a = CrackPropagator::effective_bonded_area(100.0, 40.0);
        assert!((a - 100.0).abs() < 1e-9, "square → full area, got {a}");
    }

    #[test]
    fn bonded_area_thin_is_reduced() {
        // A=4, P=10 → 4·0.8 = 3.2.
        let a = CrackPropagator::effective_bonded_area(4.0, 10.0);
        assert!((a - 3.2).abs() < 1e-9, "got {a}");
    }

    #[test]
    fn bonded_area_never_exceeds_area() {
        for (a, p) in [(100.0, 40.0), (100.0, 104.0), (4.0, 10.0), (706.0, 94.0)] {
            let bonded = CrackPropagator::effective_bonded_area(a, p);
            assert!(bonded <= a + 1e-9, "bonded {bonded} must be ≤ area {a}");
        }
    }

    #[test]
    fn bonded_area_degenerate_is_full_area() {
        assert_eq!(CrackPropagator::effective_bonded_area(100.0, 0.0), 100.0);
    }
}
