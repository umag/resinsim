//! Per-voxel cure-driven shrinkage strain — `ShrinkageCalculator` service.
//!
//! ADR-0018 / t2f3 / KB-161. Pure-function service: no internal state,
//! no I/O. The orchestrator (`SimulationRunner::apply_voxel_shrinkage_for_layer`)
//! owns per-layer iteration; this module owns the per-voxel unit-chain
//! `(dose, layer-height, resin) → cure_extent → StrainTensor`.
//!
//! # Unit chain (KB-161)
//!
//! 1. `CureField` stores cumulative absorbed dose at each voxel
//!    (mJ/cm², Beer-Lambert input).
//! 2. `CureCalculator::cure_depth_at_temp(dose, ec_t, dp)` returns the
//!    per-voxel cure depth in µm via Beer-Lambert (KB-103).
//! 3. Cure-extent fraction: `clamp(cure_depth / effective_layer_height, 0, 1)`,
//!    dimensionless `[0, 1]`. Voxels with sub-threshold dose are uncured
//!    liquid → cure-extent = 0 → StrainTensor::zero().
//! 4. `StrainTensor::from_free_shrinkage(L, C)` produces the isotropic
//!    compressive strain `ε_ii = -L · C` (KB-142 linear-shrinkage anchor).

#![cfg(feature = "field-sim")]

use crate::values::StrainTensor;

/// Pure-function helpers wrapping the cure-extent → free-shrinkage
/// pipeline.
pub struct ShrinkageCalculator;

impl ShrinkageCalculator {
    /// Cure-extent fraction at a single voxel given the absorbed dose
    /// and the layer's effective thickness. Uses
    /// `CureCalculator::cure_depth_at_temp` for the Beer-Lambert
    /// conversion (single-source Arrhenius helper per
    /// `docs/patterns/single-source-arrhenius-helper.md`).
    ///
    /// Result is clamped to `[0, 1]` defensively. Returns 0 for any
    /// voxel where dose ≤ critical_energy (Beer-Lambert "undercured"
    /// floor) — this matches KB-103's contract.
    pub fn cure_extent_at_voxel(
        absorbed_dose_mj_cm2: f32,
        ec_at_temp_mj_cm2: f32,
        dp_um: f32,
        effective_layer_height_um: f32,
    ) -> f32 {
        if !absorbed_dose_mj_cm2.is_finite()
            || !ec_at_temp_mj_cm2.is_finite()
            || !dp_um.is_finite()
            || !effective_layer_height_um.is_finite()
            || effective_layer_height_um <= 0.0
            || ec_at_temp_mj_cm2 <= 0.0
            || dp_um <= 0.0
        {
            return 0.0;
        }
        // Beer-Lambert (KB-103): Cd = Dp × ln(E / Ec), clamped at 0
        // when E ≤ Ec (undercured). Inlined here to avoid the typed-VO
        // (PenetrationDepth, Energy, VatTemperature, ...) ceremony of
        // `CureCalculator::cure_depth_at_temp`; the upstream pipeline
        // already validated finite-positive inputs via ResinProfile +
        // CureField constructors. Matches the formula in
        // `docs/patterns/single-source-arrhenius-helper.md` — Arrhenius
        // Ec(T) is applied UPSTREAM (the caller passes the already-
        // temperature-adjusted ec_at_temp_mj_cm2), so this function
        // sees only the post-temperature Beer-Lambert primitive.
        if absorbed_dose_mj_cm2 <= ec_at_temp_mj_cm2 {
            return 0.0;
        }
        let cure_depth_um = dp_um * (absorbed_dose_mj_cm2 / ec_at_temp_mj_cm2).ln();
        if !cure_depth_um.is_finite() {
            return 0.0;
        }
        (cure_depth_um / effective_layer_height_um).clamp(0.0, 1.0)
    }

    /// Free shrinkage strain at a single voxel. Wrapper around
    /// `StrainTensor::from_free_shrinkage` that threads the resin's
    /// Z/XY anisotropy ratio through to the constructor. Kept as a
    /// service-named function so call sites read consistently with the
    /// orchestrator (`apply_voxel_shrinkage_for_layer →
    /// free_shrinkage_strain_at_voxel`).
    pub fn free_shrinkage_strain_at_voxel(
        cure_extent_frac: f32,
        linear_shrinkage_frac: f32,
        z_anisotropy_ratio: f32,
    ) -> StrainTensor {
        // The constructor returns Result for NaN inputs; in this code
        // path all three inputs are already in clamped/known-finite
        // domain (cure_extent_at_voxel above, ResinProfile.validate()).
        // Defensively fall through to StrainTensor::zero() rather than
        // bubbling — the orchestrator can't usefully recover from a
        // per-voxel NaN at this point, and returning zero matches
        // KB-161's "uncured ⇒ no strain" floor.
        StrainTensor::from_free_shrinkage(
            linear_shrinkage_frac,
            cure_extent_frac,
            z_anisotropy_ratio,
        )
        .unwrap_or_else(|_| StrainTensor::zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cure_extent_zero_for_undercured_dose() {
        // dose < Ec → cure_depth = 0 → cure_extent = 0.
        let e = ShrinkageCalculator::cure_extent_at_voxel(1.0, 5.0, 170.0, 50.0);
        assert_eq!(e, 0.0);
    }

    #[test]
    fn cure_extent_at_full_threshold_is_zero() {
        // dose = Ec exactly → ln(1) = 0 → cure_depth = 0 → cure_extent = 0.
        let e = ShrinkageCalculator::cure_extent_at_voxel(5.0, 5.0, 170.0, 50.0);
        assert_eq!(e, 0.0);
    }

    #[test]
    fn cure_extent_grows_with_dose_above_threshold() {
        // Layer height generous enough that neither dose saturates the
        // clamp at 1.0, so we can see the monotonic ramp.
        let e_low = ShrinkageCalculator::cure_extent_at_voxel(10.0, 5.0, 170.0, 1000.0);
        let e_high = ShrinkageCalculator::cure_extent_at_voxel(100.0, 5.0, 170.0, 1000.0);
        assert!(e_high > e_low, "cure_extent must grow with dose ({e_low} vs {e_high})");
        assert!(e_low >= 0.0);
        assert!(e_high <= 1.0);
    }

    #[test]
    fn cure_extent_clamped_to_one_at_overdose() {
        // 1000 × Ec → cure_depth = Dp · ln(200) ≈ 170 × 5.3 ≈ 900 µm,
        // > 50 µm layer height → cure_extent clamps to 1.0.
        let e = ShrinkageCalculator::cure_extent_at_voxel(1000.0, 5.0, 170.0, 50.0);
        assert_eq!(e, 1.0);
    }

    #[test]
    fn cure_extent_zero_for_zero_layer_height() {
        // Defensive — avoids division by zero.
        let e = ShrinkageCalculator::cure_extent_at_voxel(10.0, 5.0, 170.0, 0.0);
        assert_eq!(e, 0.0);
    }

    #[test]
    fn cure_extent_zero_for_nan_inputs() {
        assert_eq!(
            ShrinkageCalculator::cure_extent_at_voxel(f32::NAN, 5.0, 170.0, 50.0),
            0.0
        );
        assert_eq!(
            ShrinkageCalculator::cure_extent_at_voxel(10.0, f32::NAN, 170.0, 50.0),
            0.0
        );
    }

    #[test]
    fn free_shrinkage_zero_for_uncured_voxel() {
        let t = ShrinkageCalculator::free_shrinkage_strain_at_voxel(0.0, 0.015, 1.5);
        assert_eq!(t, StrainTensor::zero());
    }

    #[test]
    fn free_shrinkage_isotropic_full_cure() {
        // r = 1.0 → legacy isotropic mapping.
        let t = ShrinkageCalculator::free_shrinkage_strain_at_voxel(1.0, 0.015, 1.0);
        assert!((t.xx() - (-0.015)).abs() < 1e-6);
        assert!((t.yy() - (-0.015)).abs() < 1e-6);
        assert!((t.zz() - (-0.015)).abs() < 1e-6);
    }

    #[test]
    fn free_shrinkage_anisotropic_full_cure() {
        // r = 1.5 → KB-164 default. ε_zz amplified by 1.5x relative to ε_xy.
        let t = ShrinkageCalculator::free_shrinkage_strain_at_voxel(1.0, 0.015, 1.5);
        // |ε_zz| > |ε_xx| (the warping-relevant break of hydrostatic symmetry).
        assert!(t.zz().abs() > t.xx().abs(), "ε_zz must dominate under r=1.5");
        // Trace invariance: vendor linear_shrinkage_pct meaning preserved.
        let trace = t.xx() + t.yy() + t.zz();
        assert!((trace - (-0.045)).abs() < 1e-5);
    }

    #[test]
    fn free_shrinkage_falls_through_zero_on_nan() {
        // NaN cure_extent shouldn't panic — falls through to zero.
        let t = ShrinkageCalculator::free_shrinkage_strain_at_voxel(f32::NAN, 0.015, 1.5);
        assert_eq!(t, StrainTensor::zero());
    }

    #[test]
    fn free_shrinkage_trace_bounded_by_linear_pct() {
        // For any C ∈ [0, 1] and any positive r, |trace(ε)| ≤ 3·L
        // (volume-conserving invariant; replaces the old per-axis bound
        // since anisotropy redistributes magnitude across axes).
        let l = 0.024_f32;
        for &r in &[1.0_f32, 1.5, 2.0] {
            for &c in &[0.0_f32, 0.25, 0.5, 0.75, 1.0] {
                let t = ShrinkageCalculator::free_shrinkage_strain_at_voxel(c, l, r);
                let trace_abs = (t.xx() + t.yy() + t.zz()).abs();
                assert!(
                    trace_abs <= 3.0 * l + 1e-6,
                    "|trace| {trace_abs} exceeded 3·L = {} (r={r}, c={c})",
                    3.0 * l,
                );
            }
        }
    }
}
