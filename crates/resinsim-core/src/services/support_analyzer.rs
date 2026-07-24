use crate::entities::{FailureEvent, FailureType, ResinProfile, Severity};
use crate::services::build_plate::{BuildPlate, PlateAdhesionProfile};
use crate::services::failure_predictor::SupportConfig;
use crate::services::peel_force_calculator::PeelForceCalculator;
use crate::values::{CrackFront, CrossSectionArea, PeelForce, SafetyFactor, SupportCapacity};

/// Domain service: single entry point for per-layer support + build-plate
/// safety assessment. Stateless — all inputs via parameters.
///
/// Restored per the physics plan v3 §6 1:1 service-to-responsibility mapping
/// (see `projects/000-global/research/resinsim-verification-findings.md`
/// §Item 6). Composes `PeelForceCalculator::support_capacity`,
/// `BuildPlate::holding_capacity`, `BuildPlate::total_capacity`, and
/// `SafetyFactor::compute` — no physics change.
pub struct SupportAnalyzer;

/// Aggregated per-layer safety assessment.
///
/// - `support_capacity`: tips-only capacity (plate excluded).
/// - `plate_capacity_n`: build-plate adhesion contribution, already-typed
///   as raw Newtons because `BuildPlate::holding_capacity` returns `f32`.
/// - `total_capacity`: `plate_capacity_n + support_capacity.value()` wrapped
///   as a `SupportCapacity` value object. This matches what
///   `FailurePredictor` previously recorded as `LayerResult.support_capacity_n`.
/// - `safety_factor`: `None` when no load (`total_force == 0`) per ADR-0002.
/// - `overload`: `Some(event)` with `Severity::Critical` when
///   `safety_factor.filter(|s| !s.is_safe())` is `Some` — i.e. SF ≤ 1.0
///   inclusive, because `SafetyFactor::is_safe()` is `self.0 > 1.0`.
#[derive(Debug, Clone)]
pub struct SupportAssessment {
    pub support_capacity: SupportCapacity,
    pub plate_capacity_n: f32,
    pub total_capacity: SupportCapacity,
    pub safety_factor: Option<SafetyFactor>,
    pub overload: Option<FailureEvent>,
}

impl SupportAnalyzer {
    /// Tips-only support capacity.
    ///
    /// Delegates to `PeelForceCalculator::support_capacity` — the physics
    /// source of truth. Exposed here so the safety-assessment surface has
    /// a single topical entry point per plan v3 §6.
    pub fn support_capacity(
        tensile_strength_mpa: f32,
        tip_radius_mm: f32,
        n_supports: u32,
    ) -> SupportCapacity {
        PeelForceCalculator::support_capacity(tensile_strength_mpa, tip_radius_mm, n_supports)
    }

    /// Assess one layer's support + plate safety against the total peel
    /// force. Returns all intermediate values plus an overload event
    /// when the combined capacity is insufficient.
    ///
    /// Overload predicate mirrors the previous in-line FailurePredictor
    /// block byte-for-byte: `safety_factor.filter(|s| !s.is_safe())` where
    /// `SafetyFactor::is_safe()` is `self.0 > 1.0` — so SF = 1.0 exactly
    /// triggers the overload (inclusive boundary).
    pub fn assess(
        layer: u32,
        area: CrossSectionArea,
        total_force: PeelForce,
        resin: &ResinProfile,
        supports: &SupportConfig,
        plate: &PlateAdhesionProfile,
        crack: CrackFront,
    ) -> SupportAssessment {
        let support_cap = Self::support_capacity(
            resin.tensile_strength_mpa,
            supports.tip_radius_mm,
            supports.n_supports,
        );
        // KB-188 Kendall knockdown: the resin↔resin interlayer bond is
        // fracture-limited, so for NORMAL layers its holding capacity is scaled
        // by the remaining bonded fraction `1 − crack_fraction`. Bottom-layer
        // plate adhesion (mechanical textured-plate interlock, not the
        // interlayer bond) is NEVER knocked down. `no_crack()` ⇒ factor 1.0, so
        // callers without crack geometry are behaviour-preserving.
        let plate_cap = {
            let raw = BuildPlate::holding_capacity(layer, area, plate);
            if BuildPlate::is_bottom_layer(layer, plate) {
                raw
            } else {
                raw * crack.effective_fraction()
            }
        };
        let total_capacity_n = BuildPlate::total_capacity(plate_cap, support_cap.value());
        let total_capacity = SupportCapacity::new(total_capacity_n)
            .expect("sum of non-negative plate and support capacity is non-negative");

        let safety_factor = SafetyFactor::compute(total_capacity, total_force);

        let overload = safety_factor.filter(|s| !s.is_safe()).map(|sf| {
            let source = if plate_cap > 0.0 && support_cap.value() > 0.0 {
                format!(
                    "plate {:.1} N + supports {:.1} N = {:.1} N",
                    plate_cap,
                    support_cap.value(),
                    total_capacity_n
                )
            } else if plate_cap > 0.0 {
                format!("plate adhesion {:.1} N (no supports)", plate_cap)
            } else {
                format!("supports {:.1} N (no plate adhesion)", support_cap.value())
            };
            FailureEvent {
                layer,
                failure_type: FailureType::SupportOverload,
                severity: Severity::Critical,
                message: format!(
                    "Peel force {:.1} N exceeds capacity {} (SF={:.2})",
                    total_force.value(),
                    source,
                    sf.value()
                ),
            }
        });

        SupportAssessment {
            support_capacity: support_cap,
            plate_capacity_n: plate_cap,
            total_capacity,
            safety_factor,
            overload,
        }
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

    fn plate_zero() -> PlateAdhesionProfile {
        PlateAdhesionProfile {
            plate_adhesion_kpa: 0.0,
            bottom_layer_count: 0,
            interlayer_bond_kpa: 0.0,
        }
    }

    fn resin_tensile(mpa: f32) -> ResinProfile {
        let mut r = ResinProfile::generic_standard();
        r.tensile_strength_mpa = mpa;
        r
    }

    // --- Delegator ---

    #[test]
    fn support_capacity_delegates_to_peel_force_calculator() {
        // KB-114: σ=30 MPa, r=0.2 mm, N=1 → π × 0.04 × 30 = 3.77 N
        let cap = SupportAnalyzer::support_capacity(30.0, 0.2, 1);
        assert!((cap.value() - 3.77).abs() < 0.01);

        // KB-114: σ=30 MPa, r=0.2 mm, N=10 → 37.7 N
        let cap = SupportAnalyzer::support_capacity(30.0, 0.2, 10);
        assert!((cap.value() - 37.7).abs() < 0.1);

        // Agreement with PeelForceCalculator — byte-identical
        assert_eq!(
            SupportAnalyzer::support_capacity(50.0, 0.25, 5).value(),
            PeelForceCalculator::support_capacity(50.0, 0.25, 5).value()
        );
    }

    // --- assess: zero-force ---

    #[test]
    fn assess_zero_force_no_safety_factor_no_overload() {
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(50, area(100.0), peel(0.0), &resin, &supports, &plate, CrackFront::no_crack());
        assert!(assessment.safety_factor.is_none());
        assert!(assessment.overload.is_none());
    }

    // --- assess: safe load ---

    #[test]
    fn assess_safe_load_produces_factor_no_overload() {
        // 50mm cube bottom layer — 250 N plate cap + ~37.7 N supports = 287.7 N
        // vs 32.5 N peel force → SF ≈ 8.85 (safe)
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(0, area(2500.0), peel(32.5), &resin, &supports, &plate, CrackFront::no_crack());
        let sf = assessment
            .safety_factor
            .expect("non-zero force must yield Some(SafetyFactor)");
        assert!(sf.is_safe(), "SF {:?} should be > 1.0 (safe)", sf);
        assert!(assessment.overload.is_none());
    }

    // --- assess: overload → Critical event ---

    #[test]
    fn assess_overload_produces_critical_event() {
        // Force 500 N vs ~287.7 N capacity at bottom layer 50mm cube
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(0, area(2500.0), peel(500.0), &resin, &supports, &plate, CrackFront::no_crack());
        let event = assessment
            .overload
            .as_ref()
            .expect("overload load must produce a FailureEvent");
        assert_eq!(event.failure_type, FailureType::SupportOverload);
        assert_eq!(event.severity, Severity::Critical);
        assert_eq!(event.layer, 0);
        // Outer message-format lock — "Peel force X.X N exceeds capacity {source} (SF=Y.YY)"
        assert!(
            event.message.starts_with("Peel force "),
            "message must start with 'Peel force ', got: {}",
            event.message
        );
        assert!(
            event.message.contains(" exceeds capacity "),
            "message must contain ' exceeds capacity ', got: {}",
            event.message
        );
        assert!(
            event.message.contains(" (SF="),
            "message must contain ' (SF=', got: {}",
            event.message
        );
        assert!(
            event.message.ends_with(')'),
            "message must end with ')', got: {}",
            event.message
        );
    }

    // --- assess: inclusive SF=1.0 boundary (A-MED1 lock) ---

    #[test]
    fn assess_at_sf_exactly_one_fires_overload() {
        // σ=1.0 MPa, r=1.0 mm, N=1 → support_cap = 1 × π × 1 × 1 = π (f32 const)
        // plate all-zero → total_capacity = π exact
        // peel = π → SF = 1.0 exact
        let resin = resin_tensile(1.0);
        let supports = SupportConfig {
            tip_radius_mm: 1.0,
            n_supports: 1,
        };
        let plate = plate_zero();
        let assessment = SupportAnalyzer::assess(
            50,
            area(100.0),
            peel(std::f32::consts::PI),
            &resin,
            &supports,
            &plate,
            CrackFront::no_crack(),
        );
        let sf = assessment
            .safety_factor
            .expect("non-zero force yields Some(SafetyFactor)");
        assert!(
            (sf.value() - 1.0).abs() < 1e-6,
            "SF should be ~1.0 exactly, got {}",
            sf.value()
        );
        // is_safe() is `self.0 > 1.0` so SF=1.0 is NOT safe → overload fires
        assert!(!sf.is_safe(), "SF=1.0 must not be is_safe()");
        assert!(
            assessment.overload.is_some(),
            "SF=1.0 must trigger overload (inclusive boundary)"
        );
    }

    // --- assess: decomposition invariant (A-MED2 lock) ---

    #[test]
    fn assess_total_capacity_equals_plate_plus_supports() {
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(0, area(2500.0), peel(32.5), &resin, &supports, &plate, CrackFront::no_crack());
        let expected = assessment.plate_capacity_n + assessment.support_capacity.value();
        assert!(
            (assessment.total_capacity.value() - expected).abs() < 1e-4,
            "total_capacity {} should equal plate ({}) + supports ({}) = {}",
            assessment.total_capacity.value(),
            assessment.plate_capacity_n,
            assessment.support_capacity.value(),
            expected
        );
    }

    // --- assess: three message-format branches ---

    #[test]
    fn assess_overload_message_plate_plus_supports() {
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(0, area(2500.0), peel(500.0), &resin, &supports, &plate, CrackFront::no_crack());
        let event = assessment.overload.expect("overload must fire");
        assert!(
            event.message.contains(" + supports ")
                && event.message.contains("plate ")
                && event.message.contains(" = "),
            "expected 'plate X N + supports Y N = Z N', got: {}",
            event.message
        );
    }

    #[test]
    fn assess_overload_message_plate_only() {
        // n_supports=0 → support_cap=0; plate cap > 0
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.0,
            n_supports: 0,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let assessment =
            SupportAnalyzer::assess(0, area(2500.0), peel(500.0), &resin, &supports, &plate, CrackFront::no_crack());
        let event = assessment.overload.expect("overload must fire");
        assert!(
            event.message.contains("plate adhesion ") && event.message.contains("(no supports)"),
            "expected 'plate adhesion X N (no supports)', got: {}",
            event.message
        );
    }

    #[test]
    fn assess_overload_message_supports_only() {
        // plate_zero → plate_cap=0; supports > 0
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = plate_zero();
        let assessment =
            SupportAnalyzer::assess(50, area(100.0), peel(500.0), &resin, &supports, &plate, CrackFront::no_crack());
        let event = assessment.overload.expect("overload must fire");
        assert!(
            event.message.contains("supports ") && event.message.contains("(no plate adhesion)"),
            "expected 'supports X N (no plate adhesion)', got: {}",
            event.message
        );
    }

    // --- Kendall crack knockdown (peel-crack-propagation-tier1) ---

    #[test]
    fn assess_normal_layer_crack_reduces_interlayer_capacity() {
        // Normal layer 50 (>= bottom_layer_count 6): plate_cap is the interlayer
        // bond 50 kPa × 2500 mm² = 125 N. A crack with effective_fraction 0.4
        // (crack fraction 0.6) must scale the interlayer portion to 0.4 × 125 = 50 N.
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let base = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::no_crack(),
        );
        let cracked = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::new(0.6),
        );
        assert!(
            (base.plate_capacity_n - 125.0).abs() < 0.5,
            "baseline interlayer capacity should be 125 N, got {}",
            base.plate_capacity_n
        );
        assert!(
            (cracked.plate_capacity_n - 50.0).abs() < 0.5,
            "crack (eff 0.4) should reduce interlayer capacity to 50 N, got {}",
            cracked.plate_capacity_n
        );
    }

    #[test]
    fn assess_normal_layer_crack_lowers_safety_factor() {
        // Capacity-only: the peel LOAD is identical across both runs; only the
        // crack-reduced interlayer capacity moves, so the safety factor drops.
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let base = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::no_crack(),
        );
        let cracked = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::new(0.6),
        );
        let sf_base = base
            .safety_factor
            .expect("non-zero force yields Some")
            .value();
        let sf_cracked = cracked
            .safety_factor
            .expect("non-zero force yields Some")
            .value();
        assert!(
            sf_cracked < sf_base,
            "crack must lower the safety factor: base={sf_base} cracked={sf_cracked}"
        );
    }

    #[test]
    fn assess_bottom_layer_crack_does_not_reduce_plate_adhesion() {
        // Bottom layer 0 (< bottom_layer_count 6): plate_cap is the plate
        // adhesion 100 kPa × 2500 = 250 N. The crack knockdown targets the
        // interlayer bond ONLY — bottom-layer plate adhesion is crack-invariant.
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let base = SupportAnalyzer::assess(
            0,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::no_crack(),
        );
        let cracked = SupportAnalyzer::assess(
            0,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::new(0.6),
        );
        assert!(
            (cracked.plate_capacity_n - base.plate_capacity_n).abs() < 1e-3,
            "bottom-layer plate adhesion must be crack-invariant: base={} cracked={}",
            base.plate_capacity_n,
            cracked.plate_capacity_n
        );
        assert!(
            (cracked.plate_capacity_n - 250.0).abs() < 0.5,
            "bottom-layer plate adhesion should stay 250 N, got {}",
            cracked.plate_capacity_n
        );
    }

    #[test]
    fn assess_support_capacity_invariant_under_crack() {
        // The tips-only support capacity is never touched by the interlayer crack.
        let resin = ResinProfile::generic_standard();
        let supports = SupportConfig {
            tip_radius_mm: 0.2,
            n_supports: 10,
        };
        let plate = PlateAdhesionProfile::default_textured();
        let base = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::no_crack(),
        );
        let cracked = SupportAnalyzer::assess(
            50,
            area(2500.0),
            peel(32.5),
            &resin,
            &supports,
            &plate,
            CrackFront::new(0.9),
        );
        assert_eq!(
            base.support_capacity.value(),
            cracked.support_capacity.value(),
            "support capacity must be crack-invariant"
        );
    }
}
