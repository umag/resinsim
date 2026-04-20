//! Golden-value tests: exact KB data points verified against simulation output.
//! Each test references a specific KB entry and measured value.
//! If these fail, the physics model diverges from published measurements.

use resinsim_core::services::build_plate::{BuildPlate, PlateAdhesionProfile};
use resinsim_core::services::uniformity_calculator::{UniformityCalculator, UniformityProfile};
use resinsim_core::services::{
    CureCalculator, PeelForceCalculator, ThermalCalculator, ZAxisCompensator,
};
use resinsim_core::values::*;

// --- Test-fixture helpers ---
// KB golden-value tests always pass positive finite inputs drawn from published
// measurements, so domain validators cannot reject them. These helpers carry
// the invariant justification once for the whole file.

fn dp(um: f32) -> PenetrationDepth {
    PenetrationDepth::new(um)
        .expect("KB golden value: positive finite µm is in PenetrationDepth domain")
}

fn energy(mj_cm2: f32) -> Energy {
    Energy::new(mj_cm2).expect("KB golden value: positive finite mJ/cm² is in Energy domain")
}

fn area(mm2: f64) -> CrossSectionArea {
    CrossSectionArea::new(mm2)
        .expect("KB golden value: non-negative finite mm² is in CrossSectionArea domain")
}

fn circle_area(diameter_mm: f64) -> CrossSectionArea {
    CrossSectionArea::circle(diameter_mm)
        .expect("KB golden value: non-negative finite mm is in CrossSectionArea::circle domain")
}

fn peel(n: f32) -> PeelForce {
    PeelForce::new(n).expect("KB golden value: non-negative finite N is in PeelForce domain")
}

fn tau(sec: f32) -> ThermalTimeConstant {
    ThermalTimeConstant::new(sec)
        .expect("KB golden value: positive finite seconds is in ThermalTimeConstant domain")
}

// ============================================================================
// KB-100: Liqcreate resin Dp/Ec at 405nm
// ============================================================================

#[test]
fn kb100_premium_black_cure_depth() {
    // KB-100: Dp=170µm, Ec=5.0 mJ/cm² at 405nm
    // At E=10.0: Cd = 170 × ln(10/5) = 117.8 µm
    let cd = CureCalculator::cure_depth(dp(170.0), energy(10.0), energy(5.0));
    assert!(
        (cd.value() - 117.83).abs() < 0.5,
        "KB-100 Premium Black: got {:.2}",
        cd.value()
    );
}

#[test]
fn kb100_premium_white_cure_depth() {
    // KB-100: Dp=350µm, Ec=6.87 mJ/cm² at 405nm
    // At E=20.0: Cd = 350 × ln(20/6.87) = 374.0 µm
    let cd = CureCalculator::cure_depth(dp(350.0), energy(20.0), energy(6.87));
    assert!(
        (cd.value() - 374.0).abs() < 1.0,
        "KB-100 Premium White: got {:.2}",
        cd.value()
    );
}

#[test]
fn kb100_strong_x_cure_depth() {
    // KB-100: Dp=200µm, Ec=9.88 mJ/cm² at 405nm
    // At E=20.0: Cd = 200 × ln(20/9.88) = 200 × 0.705 = 141.0 µm
    let cd = CureCalculator::cure_depth(dp(200.0), energy(20.0), energy(9.88));
    assert!(
        (cd.value() - 141.0).abs() < 1.0,
        "KB-100 Strong-X: got {:.2}",
        cd.value()
    );
}

#[test]
fn kb100_tough_x_cure_depth() {
    // KB-100: Dp=270µm, Ec=24.20 mJ/cm² at 405nm
    // At E=50.0: Cd = 270 × ln(50/24.2) = 270 × 0.726 = 196.0 µm
    let cd = CureCalculator::cure_depth(dp(270.0), energy(50.0), energy(24.20));
    assert!(
        (cd.value() - 196.0).abs() < 1.0,
        "KB-100 Tough-X: got {:.2}",
        cd.value()
    );
}

// ============================================================================
// KB-101: Academic Dp/Ec measurements
// ============================================================================

#[test]
fn kb101_pr48_365nm() {
    // KB-101: PR48 at 365nm, Dp=42µm, Ec=18.3 mJ/cm²
    let cd = CureCalculator::cure_depth(dp(42.0), energy(50.0), energy(18.3));
    assert!(
        (cd.value() - 42.2).abs() < 0.5,
        "KB-101 PR48@365nm: got {:.2}",
        cd.value()
    );
}

#[test]
fn kb101_veroclear_405nm() {
    // KB-101: VeroClear at 405nm, Dp=568µm, Ec=6.9 mJ/cm²
    let cd = CureCalculator::cure_depth(dp(568.0), energy(50.0), energy(6.9));
    assert!(
        (cd.value() - 1125.0).abs() < 5.0,
        "KB-101 VeroClear@405nm: got {:.2}",
        cd.value()
    );
}

#[test]
fn kb101_formlabs_clear_405nm() {
    // KB-101: Formlabs Clear at 405nm, Dp=192µm, Ec=12.6 mJ/cm²
    let cd = CureCalculator::cure_depth(dp(192.0), energy(30.0), energy(12.6));
    // Cd = 192 × ln(30/12.6) = 192 × 0.868 = 166.6 µm
    assert!(
        (cd.value() - 166.6).abs() < 1.0,
        "KB-101 Formlabs Clear: got {:.2}",
        cd.value()
    );
}

// ============================================================================
// KB-102: NIST interlaboratory Dp/Ec for PR48
// ============================================================================

#[test]
fn kb102_nist_pr48_405nm() {
    // KB-102: PR48 at 405nm, Dp=69.3±3.8µm, Ec=17.9±2.3 mJ/cm²
    // At E=50: Cd = 69.3 × ln(50/17.9) = 69.3 × 1.028 = 71.2 µm
    let cd = CureCalculator::cure_depth(dp(69.3), energy(50.0), energy(17.9));
    assert!(
        (cd.value() - 71.2).abs() < 1.0,
        "KB-102 NIST PR48: got {:.2}",
        cd.value()
    );
}

// ============================================================================
// KB-110: Film peel stress measurements
// ============================================================================

#[test]
fn kb110_standard_fep_50mm_square() {
    // KB-110: σ=13 kPa (FEP 50µm), A=2500 mm² → F=32.5 N
    let f = PeelForceCalculator::peel_force(13.0, area(2500.0), 1.0);
    assert!(
        (f.value() - 32.5).abs() < 0.01,
        "KB-110 FEP 50mm²: got {:.2}",
        f.value()
    );
}

#[test]
fn kb110_acf_50mm_square() {
    // KB-110: σ=12 kPa (ACF), A=2500 mm² → F=30.0 N
    let f = PeelForceCalculator::peel_force(12.0, area(2500.0), 1.0);
    assert!(
        (f.value() - 30.0).abs() < 0.01,
        "KB-110 ACF: got {:.2}",
        f.value()
    );
}

#[test]
fn kb110_thick_fep_50mm_square() {
    // KB-110: σ=18 kPa (FEP 127µm), A=2500 mm² → F=45.0 N
    let f = PeelForceCalculator::peel_force(18.0, area(2500.0), 1.0);
    assert!(
        (f.value() - 45.0).abs() < 0.01,
        "KB-110 thick FEP: got {:.2}",
        f.value()
    );
}

#[test]
fn kb110_full_saturn_plate() {
    // KB-110: 120×68mm = 8160 mm², σ=13 kPa → F=106.08 N
    let f = PeelForceCalculator::peel_force(13.0, area(8160.0), 1.0);
    assert!(
        (f.value() - 106.08).abs() < 0.1,
        "KB-110 full Saturn: got {:.2}",
        f.value()
    );
}

// ============================================================================
// KB-112: Peel force vs. lift speed
// ============================================================================

#[test]
fn kb112_96x_speed_factor() {
    // KB-112: 96× speed increase → 230% force (f=2.30)
    let f = PeelForceCalculator::lift_speed_factor(96.0, 1.0);
    assert!((f - 2.30).abs() < 0.05, "KB-112 96× speed: got {:.3}", f);
}

// ============================================================================
// KB-114: Support capacity test vectors
// ============================================================================

#[test]
fn kb114_single_04mm_tip() {
    // KB-114: σ=30 MPa, r=0.2mm, N=1 → F=3.77 N
    let cap = PeelForceCalculator::support_capacity(30.0, 0.2, 1);
    assert!(
        (cap.value() - 3.77).abs() < 0.01,
        "KB-114 single tip: got {:.2}",
        cap.value()
    );
}

#[test]
fn kb114_ten_supports() {
    // KB-114: σ=30, r=0.2, N=10 → 37.7 N
    let cap = PeelForceCalculator::support_capacity(30.0, 0.2, 10);
    assert!(
        (cap.value() - 37.7).abs() < 0.1,
        "KB-114 ten supports: got {:.2}",
        cap.value()
    );
}

#[test]
fn kb114_suction_sealed_30mm_cup() {
    // KB-114: ΔP=101 kPa, A=706 mm² → F=71.3 N
    let f = PeelForceCalculator::suction_force(101.0, area(706.0));
    assert!(
        (f.value() - 71.3).abs() < 0.1,
        "KB-114 sealed 30mm: got {:.2}",
        f.value()
    );
}

#[test]
fn kb114_suction_drained_is_zero() {
    // KB-114: ΔP=0, A=706 → 0 N
    let f = PeelForceCalculator::suction_force(0.0, area(706.0));
    assert!(
        f.value().abs() < 1e-6,
        "KB-114 drained: got {:.6}",
        f.value()
    );
}

// ============================================================================
// KB-120: LCD uniformity
// ============================================================================

#[test]
fn kb120_saturn1_center_brightest() {
    // KB-120: Saturn 1, 34% variation. Center factor ≈ 1.17, corner ≈ 0.83
    let p = UniformityProfile::saturn_1();
    let center = UniformityCalculator::intensity_factor(96.0, 60.0, &p);
    let corner = UniformityCalculator::intensity_factor(0.0, 0.0, &p);
    assert!(
        (center - 1.17).abs() < 0.01,
        "KB-120 center: got {:.3}",
        center
    );
    assert!(
        (corner - 0.83).abs() < 0.01,
        "KB-120 corner: got {:.3}",
        corner
    );
}

#[test]
fn kb120_saturn2_less_variation() {
    // KB-120: Saturn 2, 22% variation. Center ≈ 1.11, corner ≈ 0.89
    let p = UniformityProfile::saturn_2();
    let center = UniformityCalculator::intensity_factor(109.5, 61.5, &p);
    let corner = UniformityCalculator::intensity_factor(0.0, 0.0, &p);
    assert!(
        (center - 1.11).abs() < 0.01,
        "KB-120 S2 center: got {:.3}",
        center
    );
    assert!(
        (corner - 0.89).abs() < 0.01,
        "KB-120 S2 corner: got {:.3}",
        corner
    );
}

// ============================================================================
// KB-130/KB-131: Z-axis precision and deflection
// ============================================================================

#[test]
fn kb131_mrazek_fast_resin_deflection() {
    // KB-131: F=120N, k=460 N/mm → Δz=260.9 µm (Mrazek measured ~260µm)
    let dz = ZAxisCompensator::deflection_um(peel(120.0), 460.0);
    assert!((dz - 260.9).abs() < 0.1, "KB-131 Fast resin: got {:.1}", dz);
}

#[test]
fn kb131_mrazek_sculpt_resin_deflection() {
    // KB-131: F=200N, k=460 → Δz=434.8 µm (Mrazek measured ~340µm + settling)
    let dz = ZAxisCompensator::deflection_um(peel(200.0), 460.0);
    assert!(
        (dz - 434.8).abs() < 0.1,
        "KB-131 Sculpt resin: got {:.1}",
        dz
    );
}

#[test]
fn kb131_derived_stiffness() {
    // KB-131: F=120N, Δz=260µm → k ≈ 461.5 N/mm
    let k = ZAxisCompensator::derive_stiffness(120.0, 260.0);
    assert!((k - 461.5).abs() < 1.0, "KB-131 derived k: got {:.1}", k);
}

// ============================================================================
// KB-141: Viscosity and temperature dependence
// ============================================================================

#[test]
fn kb141_82pct_drop_25_to_50() {
    // KB-141: 82% viscosity drop from 25→50°C
    // With Ea=52 kJ/mol: ratio ≈ 0.18
    let ratio = ThermalCalculator::viscosity_ratio(25.0, 50.0, 52.0);
    assert!(
        (ratio - 0.18).abs() < 0.03,
        "KB-141 82% drop: ratio={:.3}",
        ratio
    );
}

#[test]
fn kb141_viscosity_at_30c() {
    // KB-150 vector: at 30°C with Ea=36 kJ/mol, µ/µ₀ ≈ 0.775
    let ratio = ThermalCalculator::viscosity_ratio(25.0, 30.0, 36.0);
    assert!(
        (ratio - 0.775).abs() < 0.02,
        "KB-141 30°C ratio: got {:.3}",
        ratio
    );
}

#[test]
fn kb141_elegoo_abs_like_viscosity() {
    // KB-141: Elegoo ABS-Like V2 = 150-200 mPa·s at 25°C
    // At 50°C with Ea=52: µ = 175 × 0.18 ≈ 31.5 mPa·s
    let mu = ThermalCalculator::viscosity_at_temperature(175.0, 25.0, 50.0, 52.0);
    assert!(
        mu > 20.0 && mu < 50.0,
        "KB-141 ABS-Like at 50°C: got {:.1}",
        mu
    );
}

// ============================================================================
// KB-150: Vat thermal model test vectors
// ============================================================================

#[test]
fn kb150_temperature_at_10min() {
    // KB-150: T_ambient=22, ΔT=10, τ=1200, t=600s → T=25.9°C
    let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 600.0);
    assert!(
        (t.value() - 25.9).abs() < 0.1,
        "KB-150 10min: got {:.1}",
        t.value()
    );
}

#[test]
fn kb150_temperature_at_20min() {
    // KB-150: t=1200s → T=28.3°C
    let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 1200.0);
    assert!(
        (t.value() - 28.3).abs() < 0.1,
        "KB-150 20min: got {:.1}",
        t.value()
    );
}

#[test]
fn kb150_temperature_at_40min() {
    // KB-150: t=2400s → T=30.6°C
    let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 2400.0);
    assert!(
        (t.value() - 30.6).abs() < 0.1,
        "KB-150 40min: got {:.1}",
        t.value()
    );
}

#[test]
fn kb150_steady_state() {
    // KB-150: t→∞ → T=32.0°C
    let t = ThermalCalculator::vat_temperature(22.0, 10.0, tau(1200.0), 100000.0);
    assert!(
        (t.value() - 32.0).abs() < 0.1,
        "KB-150 steady state: got {:.1}",
        t.value()
    );
}

#[test]
fn kb150_duty_cycle_typical() {
    // KB-151: 2.5s exposure / 10s total = 25%
    let dc = ThermalCalculator::duty_cycle(2.5, 7.5);
    assert!((dc - 0.25).abs() < 1e-6, "KB-151 duty: got {:.4}", dc);
}

// ============================================================================
// KB-121: UV intensity by printer model — verify energy dose computation
// ============================================================================

#[test]
fn kb121_photon_m3_plus_energy() {
    // KB-121: Photon M3 Plus = 7.06 mW/cm² at 2.5s → E = 17.65 mJ/cm²
    let e = Energy::from_exposure(7.06, 2.5).expect(
        "KB golden value: positive finite irradiance × seconds is in Energy::from_exposure domain",
    );
    assert!(
        (e.value() - 17.65).abs() < 0.01,
        "KB-121 M3 Plus energy: got {:.2}",
        e.value()
    );
}

#[test]
fn kb121_saturn_energy() {
    // KB-121: Saturn = 3.72 mW/cm² at 2.5s → E = 9.30 mJ/cm²
    let e = Energy::from_exposure(3.72, 2.5).expect(
        "KB golden value: positive finite irradiance × seconds is in Energy::from_exposure domain",
    );
    assert!(
        (e.value() - 9.30).abs() < 0.01,
        "KB-121 Saturn energy: got {:.2}",
        e.value()
    );
}

// ============================================================================
// KB-172: Graduated cylinder calibration — expected forces
// ============================================================================

#[test]
fn kb172_cylinder_5mm_force() {
    // KB-172: 5mm dia → A=19.6 mm², σ=13 kPa → F=0.255 N
    let a = circle_area(5.0);
    let f = PeelForceCalculator::peel_force(13.0, a, 1.0);
    assert!(
        (f.value() - 0.255).abs() < 0.01,
        "KB-172 5mm cyl: got {:.3}",
        f.value()
    );
}

#[test]
fn kb172_cylinder_20mm_force() {
    // KB-172: 20mm dia → A=314.2 mm², σ=13 kPa → F=4.08 N
    let a = circle_area(20.0);
    let f = PeelForceCalculator::peel_force(13.0, a, 1.0);
    assert!(
        (f.value() - 4.08).abs() < 0.02,
        "KB-172 20mm cyl: got {:.3}",
        f.value()
    );
}

#[test]
fn kb172_cylinder_30mm_force() {
    // KB-172: 30mm dia → A=706.9 mm², σ=13 kPa → F=9.19 N
    let a = circle_area(30.0);
    let f = PeelForceCalculator::peel_force(13.0, a, 1.0);
    assert!(
        (f.value() - 9.19).abs() < 0.02,
        "KB-172 30mm cyl: got {:.3}",
        f.value()
    );
}

#[test]
fn kb172_force_linearity() {
    // KB-172: force should be strictly proportional to area across all 6 cylinders
    let diameters = [5.0, 10.0, 15.0, 20.0, 25.0, 30.0];
    let forces: Vec<f32> = diameters
        .iter()
        .map(|&d| PeelForceCalculator::peel_force(13.0, circle_area(d), 1.0).value())
        .collect();
    let areas: Vec<f64> = diameters.iter().map(|&d| circle_area(d).value()).collect();

    // Check F/A ratio is constant
    let ratio_first = forces[0] as f64 / areas[0];
    for i in 1..6 {
        let ratio = forces[i] as f64 / areas[i];
        assert!(
            (ratio - ratio_first).abs() < 1e-4,
            "KB-172 linearity: cyl {i} ratio {ratio:.6} vs first {ratio_first:.6}"
        );
    }
}

// ============================================================================
// KB-173: Suction cup — sealed vs drained force comparison
// ============================================================================

#[test]
fn kb173_sealed_20mm_cup_force() {
    // KB-173: sealed 20mm cup, ΔP=50 kPa, A_sealed=254.5 mm²
    // F_suction = 50 × 254.5 × 1e-3 = 12.7 N
    let f = PeelForceCalculator::suction_force(50.0, area(254.5));
    assert!(
        (f.value() - 12.73).abs() < 0.1,
        "KB-173 sealed 20mm: got {:.2}",
        f.value()
    );
}

#[test]
fn kb173_sealed_dominates_peel() {
    // KB-173: sealed cup total force >> wall-only peel force
    // Wall area ≈ 59.7 mm², peel = 0.78 N
    // Suction = 12.7 N → suction/peel ratio > 5×
    let wall_peel = PeelForceCalculator::peel_force(13.0, area(59.7), 1.0);
    let suction = PeelForceCalculator::suction_force(50.0, area(254.5));
    let ratio = suction.value() / wall_peel.value();
    assert!(
        ratio > 5.0,
        "KB-173: suction should dominate peel by >5×, got {ratio:.1}×"
    );
}

// ============================================================================
// KB-103: Beer-Lambert intensity at depth
// ============================================================================

#[test]
fn kb103_intensity_at_dp() {
    // KB-103: at z=Dp, I = I₀/e = I₀ × 0.3679
    let i = CureCalculator::intensity_at_depth(5.0, 170.0, dp(170.0));
    assert!((i - 1.839).abs() < 0.001, "KB-103 at Dp: got {:.4}", i);
}

#[test]
fn kb103_intensity_at_2dp() {
    // KB-103: at z=2×Dp, I = I₀/e² = I₀ × 0.1353
    let i = CureCalculator::intensity_at_depth(5.0, 340.0, dp(170.0));
    assert!((i - 0.677).abs() < 0.001, "KB-103 at 2Dp: got {:.4}", i);
}
