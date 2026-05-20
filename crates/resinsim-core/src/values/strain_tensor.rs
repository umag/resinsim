//! 6-component symmetric strain tensor — `StrainTensor` value object.
//!
//! ADR-0018, t2f3. Stores the symmetric Cauchy strain tensor in Voigt
//! notation as six f32 components `[ε_xx, ε_yy, ε_zz, ε_yz, ε_xz, ε_xy]`.
//! The full 3×3 strain tensor is symmetric, so six components suffice
//! (the three off-diagonal symmetric duplicates are implicit).
//!
//! # NaN policy
//!
//! Two-layer defence (`docs/patterns/nan-two-layer-defence.md`):
//! the constructor and every mutating accessor check `is_finite()` on each
//! component before touching state. NaN cannot enter via the public API.
//! Strain components are signed (compressive shrinkage is negative ε_ii),
//! so the validator MUST NOT collapse "finite" into "positive" — anti-
//! pattern `rust-nan-positive-validation-gap`.
//!
//! # Voigt convention
//!
//! Components map to the full 3×3 symmetric matrix as:
//!
//! ```text
//! ⎡ε_xx  ε_xy  ε_xz⎤
//! ⎢ε_xy  ε_yy  ε_yz⎥
//! ⎣ε_xz  ε_yz  ε_zz⎦
//! ```
//!
//! Voigt vector ordering follows the standard engineering convention
//! `(xx, yy, zz, yz, xz, xy)` — diagonals first, then off-diagonals in
//! the order that pairs with `StressTensor` for the 6×6 stiffness
//! multiplication `σ = D : ε`.

#![cfg(feature = "field-sim")]

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from `StrainTensor` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum StrainTensorError {
    #[error("StrainTensor components must all be finite, got ({xx}, {yy}, {zz}, {yz}, {xz}, {xy})")]
    NonFiniteComponent {
        xx: f32,
        yy: f32,
        zz: f32,
        yz: f32,
        xz: f32,
        xy: f32,
    },
}

/// Symmetric strain tensor in Voigt notation `[ε_xx, ε_yy, ε_zz, ε_yz, ε_xz, ε_xy]`.
///
/// All six components are f32. Dimensionless (strain is a ratio of length
/// change to original length). Negative ε_ii values are physical and
/// expected for compressive shrinkage in cured photopolymer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrainTensor {
    components: [f32; 6],
}

impl StrainTensor {
    /// Zero strain — all six components 0. Cannot fail.
    pub fn zero() -> Self {
        Self {
            components: [0.0; 6],
        }
    }

    /// New strain tensor from Voigt-ordered components. Validates each is
    /// finite; rejects NaN / ±∞ before constructing.
    pub fn new(xx: f32, yy: f32, zz: f32, yz: f32, xz: f32, xy: f32) -> Result<Self, StrainTensorError> {
        if !(xx.is_finite() && yy.is_finite() && zz.is_finite()
            && yz.is_finite() && xz.is_finite() && xy.is_finite())
        {
            return Err(StrainTensorError::NonFiniteComponent { xx, yy, zz, yz, xz, xy });
        }
        Ok(Self {
            components: [xx, yy, zz, yz, xz, xy],
        })
    }

    /// Isotropic strain — equal ε on all three normal axes, zero shears.
    /// Models uniform free shrinkage at full cure with no preferred
    /// direction. Accepts negative ε (compressive shrinkage) and positive
    /// ε (expansion); rejects only NaN / ±∞.
    pub fn from_isotropic(epsilon: f32) -> Result<Self, StrainTensorError> {
        Self::new(epsilon, epsilon, epsilon, 0.0, 0.0, 0.0)
    }

    /// Free shrinkage strain at a single voxel given the resin's linear
    /// shrinkage, the local cure-extent fraction, and the Z/XY shrinkage
    /// anisotropy ratio (KB-164).
    ///
    /// `linear_shrinkage_frac` — `ResinProfile.linear_shrinkage_pct / 100`,
    /// dimensionless fraction in `[0, ∞)` (KB-142 standard range ≈
    /// 0.009–0.024).
    ///
    /// `cure_extent_frac` — dimensionless `[0, 1]` from KB-161:
    /// `clamp(cure_depth_at_voxel / effective_layer_height, 0, 1)`. Voxels
    /// below the cure threshold (uncured liquid) pass `0.0` and produce
    /// `Self::zero()`.
    ///
    /// `z_anisotropy_ratio` — KB-164. Z / XY shrinkage anisotropy ratio.
    /// Pass `1.0` for the legacy isotropic behaviour; pass `> 1.0` for
    /// the physically realistic layer-by-layer constraint (XY suppressed,
    /// Z amplified). Volume-conserving mapping: with `ε_iso = -L · C`,
    ///
    /// ```text
    /// factor_xy = 3 / (2 + r)
    /// factor_z  = r · factor_xy = 3 · r / (2 + r)
    /// ε_xx = ε_yy = factor_xy · ε_iso
    /// ε_zz       = factor_z  · ε_iso
    /// ```
    ///
    /// preserves the vendor-data-sheet meaning of `linear_shrinkage_pct`
    /// (trace of ε equals `3·ε_iso` regardless of `r`).
    ///
    /// The sign convention is "shrinkage is negative". Shears stay zero —
    /// directional anisotropy along Z does not introduce shear under the
    /// free-shrinkage model.
    pub fn from_free_shrinkage(
        linear_shrinkage_frac: f32,
        cure_extent_frac: f32,
        z_anisotropy_ratio: f32,
    ) -> Result<Self, StrainTensorError> {
        if !linear_shrinkage_frac.is_finite()
            || !cure_extent_frac.is_finite()
            || !z_anisotropy_ratio.is_finite()
            || z_anisotropy_ratio <= 0.0
        {
            return Err(StrainTensorError::NonFiniteComponent {
                xx: linear_shrinkage_frac,
                yy: cure_extent_frac,
                zz: z_anisotropy_ratio,
                yz: 0.0,
                xz: 0.0,
                xy: 0.0,
            });
        }
        // Clamp cure_extent into [0, 1] defensively. KB-161 says the
        // caller (ShrinkageCalculator) is responsible for the clamp;
        // doing it here too is belt-and-braces against the unit-
        // accounting error described in the plan's step-6 risk.
        let clamped = cure_extent_frac.clamp(0.0, 1.0);
        let eps_iso = -linear_shrinkage_frac * clamped;
        // Volume-conserving redistribution of the isotropic strain into
        // an anisotropic ε field with the given Z/XY ratio. r = 1.0 →
        // legacy isotropic ε_xx = ε_yy = ε_zz = ε_iso.
        let r = z_anisotropy_ratio;
        let factor_xy = 3.0 / (2.0 + r);
        let factor_z = r * factor_xy;
        let eps_xy = factor_xy * eps_iso;
        let eps_z = factor_z * eps_iso;
        Self::new(eps_xy, eps_xy, eps_z, 0.0, 0.0, 0.0)
    }

    /// `ε_xx` — normal strain along x.
    pub fn xx(&self) -> f32 {
        self.components[0]
    }
    /// `ε_yy` — normal strain along y.
    pub fn yy(&self) -> f32 {
        self.components[1]
    }
    /// `ε_zz` — normal strain along z (build axis).
    pub fn zz(&self) -> f32 {
        self.components[2]
    }
    /// `ε_yz` — engineering shear in the y-z plane.
    pub fn yz(&self) -> f32 {
        self.components[3]
    }
    /// `ε_xz` — engineering shear in the x-z plane.
    pub fn xz(&self) -> f32 {
        self.components[4]
    }
    /// `ε_xy` — engineering shear in the x-y plane.
    pub fn xy(&self) -> f32 {
        self.components[5]
    }

    /// Raw Voigt components `[ε_xx, ε_yy, ε_zz, ε_yz, ε_xz, ε_xy]`.
    pub fn components(&self) -> [f32; 6] {
        self.components
    }

    /// Frobenius norm of the 3×3 strain tensor: `√(Σᵢⱼ εᵢⱼ²)`.
    ///
    /// Because the symmetric off-diagonals appear twice in the full
    /// matrix, the Voigt-form expression is
    /// `√(ε_xx² + ε_yy² + ε_zz² + 2·(ε_yz² + ε_xz² + ε_xy²))`.
    /// Returns 0 iff every component is 0; always non-negative on
    /// finite inputs.
    pub fn magnitude(&self) -> f32 {
        let [xx, yy, zz, yz, xz, xy] = self.components;
        let sum_sq = xx * xx + yy * yy + zz * zz + 2.0 * (yz * yz + xz * xz + xy * xy);
        sum_sq.sqrt()
    }

    /// Maximum principal strain (largest eigenvalue of the 3×3 symmetric
    /// tensor). Computed via the closed-form trigonometric solution for
    /// 3×3 symmetric eigenvalues (Smith, 1961; Deledalle et al., 2017).
    ///
    /// Returns `Err` if any intermediate computation produces non-finite
    /// (e.g. catastrophic cancellation on degenerate near-zero tensors);
    /// otherwise returns the largest real eigenvalue. Always returns
    /// `Ok(0.0)` for `Self::zero()`.
    pub fn max_principal(&self) -> Result<f32, StrainTensorError> {
        let [xx, yy, zz, yz, xz, xy] = self.components;

        // p1 = ε_xy² + ε_xz² + ε_yz² (off-diagonal squared-sum).
        let p1 = xy * xy + xz * xz + yz * yz;

        if p1 == 0.0 {
            // Diagonal matrix — eigenvalues are the diagonal entries.
            let m = xx.max(yy).max(zz);
            if !m.is_finite() {
                return Err(StrainTensorError::NonFiniteComponent { xx, yy, zz, yz, xz, xy });
            }
            return Ok(m);
        }

        let q = (xx + yy + zz) / 3.0;
        let p2 = (xx - q) * (xx - q)
            + (yy - q) * (yy - q)
            + (zz - q) * (zz - q)
            + 2.0 * p1;
        let p = (p2 / 6.0).sqrt();

        // B = (1/p) * (A - qI). Determinant of B is then used to find phi.
        let b_xx = (xx - q) / p;
        let b_yy = (yy - q) / p;
        let b_zz = (zz - q) / p;
        let b_yz = yz / p;
        let b_xz = xz / p;
        let b_xy = xy / p;

        // det(B) for a 3×3 symmetric matrix.
        let det_b = b_xx * (b_yy * b_zz - b_yz * b_yz)
            - b_xy * (b_xy * b_zz - b_yz * b_xz)
            + b_xz * (b_xy * b_yz - b_yy * b_xz);
        let r = (det_b / 2.0).clamp(-1.0, 1.0);
        let phi = r.acos() / 3.0;
        let eig1 = q + 2.0 * p * phi.cos();

        if !eig1.is_finite() {
            return Err(StrainTensorError::NonFiniteComponent { xx, yy, zz, yz, xz, xy });
        }
        Ok(eig1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn zero_has_zero_components() {
        let e = StrainTensor::zero();
        assert_eq!(e.components(), [0.0; 6]);
    }

    #[test]
    fn zero_magnitude_is_zero() {
        assert_eq!(StrainTensor::zero().magnitude(), 0.0);
    }

    #[test]
    fn new_accepts_finite_components() {
        let e = StrainTensor::new(0.01, -0.02, 0.005, 0.0, 0.0, 0.0)
            .expect("finite components must construct");
        assert_eq!(e.xx(), 0.01);
        assert_eq!(e.yy(), -0.02);
    }

    #[test]
    fn new_rejects_nan_in_any_component() {
        for slot in 0..6 {
            let mut comps = [0.0_f32; 6];
            comps[slot] = f32::NAN;
            let r = StrainTensor::new(comps[0], comps[1], comps[2], comps[3], comps[4], comps[5]);
            assert!(
                r.is_err(),
                "NaN at slot {slot} must be rejected",
            );
        }
    }

    #[test]
    fn new_rejects_positive_infinity() {
        assert!(StrainTensor::new(f32::INFINITY, 0.0, 0.0, 0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn new_rejects_negative_infinity() {
        assert!(StrainTensor::new(0.0, 0.0, f32::NEG_INFINITY, 0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn from_isotropic_diagonal_only() {
        let e = StrainTensor::from_isotropic(-0.015).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert_eq!(e.xx(), -0.015);
        assert_eq!(e.yy(), -0.015);
        assert_eq!(e.zz(), -0.015);
        assert_eq!(e.yz(), 0.0);
        assert_eq!(e.xz(), 0.0);
        assert_eq!(e.xy(), 0.0);
    }

    #[test]
    fn from_isotropic_nan_rejected() {
        assert!(StrainTensor::from_isotropic(f32::NAN).is_err());
    }

    #[test]
    fn from_free_shrinkage_at_zero_cure_is_zero() {
        // No cure → no shrinkage strain (regardless of anisotropy).
        let e = StrainTensor::from_free_shrinkage(0.015, 0.0, 1.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert_eq!(e, StrainTensor::zero());
        let e = StrainTensor::from_free_shrinkage(0.015, 0.0, 1.5).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert_eq!(e, StrainTensor::zero());
    }

    #[test]
    fn from_free_shrinkage_isotropic_at_full_cure_matches_linear_pct() {
        // Anisotropy ratio = 1.0 → legacy isotropic ε_ii = -L exactly.
        let e = StrainTensor::from_free_shrinkage(0.015, 1.0, 1.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!((e.xx() - (-0.015)).abs() < 1e-6);
        assert!((e.yy() - (-0.015)).abs() < 1e-6);
        assert!((e.zz() - (-0.015)).abs() < 1e-6);
        assert_eq!(e.xy(), 0.0);
    }

    #[test]
    fn from_free_shrinkage_anisotropic_amplifies_z() {
        // KB-164: ratio = 1.5 with volume conservation:
        //   factor_xy = 3 / (2 + 1.5) = 0.857
        //   factor_z  = 1.5 × 0.857   = 1.286
        // At L=0.015, full cure: ε_xx = ε_yy = -0.01286; ε_zz = -0.01929.
        let e = StrainTensor::from_free_shrinkage(0.015, 1.0, 1.5).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        let factor_xy = 3.0_f32 / (2.0 + 1.5);
        let factor_z = 1.5 * factor_xy;
        assert!((e.xx() - (-0.015 * factor_xy)).abs() < 1e-5);
        assert!((e.yy() - (-0.015 * factor_xy)).abs() < 1e-5);
        assert!((e.zz() - (-0.015 * factor_z)).abs() < 1e-5);
        // Volume conservation: trace stays at 3 · ε_iso = -0.045.
        assert!(((e.xx() + e.yy() + e.zz()) - (-0.045)).abs() < 1e-5);
    }

    #[test]
    fn from_free_shrinkage_negative_sign_for_shrinkage() {
        // Positive linear_shrinkage_frac + positive cure_extent →
        // negative ε (compressive) regardless of anisotropy.
        let e = StrainTensor::from_free_shrinkage(0.024, 0.5, 1.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!(e.xx() < 0.0);
        let e = StrainTensor::from_free_shrinkage(0.024, 0.5, 1.5).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!(e.zz() < 0.0);
    }

    #[test]
    fn from_free_shrinkage_clamps_overflow_cure() {
        // cure_extent_frac > 1 silently clamps to 1 (defensive).
        let e_over = StrainTensor::from_free_shrinkage(0.015, 1.5, 1.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        let e_full = StrainTensor::from_free_shrinkage(0.015, 1.0, 1.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert_eq!(e_over, e_full);
    }

    #[test]
    fn from_free_shrinkage_rejects_nan_inputs() {
        assert!(StrainTensor::from_free_shrinkage(f32::NAN, 0.5, 1.0).is_err());
        assert!(StrainTensor::from_free_shrinkage(0.015, f32::NAN, 1.0).is_err());
        assert!(StrainTensor::from_free_shrinkage(0.015, 0.5, f32::NAN).is_err());
    }

    #[test]
    fn from_free_shrinkage_rejects_non_positive_anisotropy() {
        // ratio must be > 0 (physical constraint).
        assert!(StrainTensor::from_free_shrinkage(0.015, 1.0, 0.0).is_err());
        assert!(StrainTensor::from_free_shrinkage(0.015, 1.0, -0.5).is_err());
    }

    #[test]
    fn magnitude_isotropic_compressive() {
        // Pure isotropic ε = -0.01 → magnitude = √(3 × 0.0001) = 0.01√3.
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        let expected = (3.0_f32 * 0.0001).sqrt();
        assert!((e.magnitude() - expected).abs() < 1e-6);
    }

    #[test]
    fn magnitude_shear_counted_twice() {
        // Pure shear: only ε_xy ≠ 0. Frobenius² = 0 + 0 + 0 + 2·(0 + 0 + ε_xy²).
        let e = StrainTensor::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.05).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        let expected = (2.0_f32 * 0.05 * 0.05).sqrt();
        assert!((e.magnitude() - expected).abs() < 1e-6);
    }

    #[test]
    fn max_principal_zero_for_zero_tensor() {
        assert_eq!(StrainTensor::zero().max_principal().expect("test fixture: literal finite tensor components satisfy from_* preconditions"), 0.0);
    }

    #[test]
    fn max_principal_isotropic_compressive() {
        // Diagonal -0.01 on all axes → all eigenvalues = -0.01, max = -0.01.
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!((e.max_principal().expect("test fixture: literal finite tensor components satisfy from_* preconditions") - (-0.01)).abs() < 1e-6);
    }

    #[test]
    fn max_principal_diagonal_picks_largest() {
        // Diagonal {-0.02, 0.005, -0.001} → max = 0.005.
        let e = StrainTensor::new(-0.02, 0.005, -0.001, 0.0, 0.0, 0.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!((e.max_principal().expect("test fixture: literal finite tensor components satisfy from_* preconditions") - 0.005).abs() < 1e-6);
    }

    #[test]
    fn max_principal_with_shear() {
        // Symmetric matrix with non-zero shear — answer is closed-form
        // verifiable via numpy.linalg.eigvalsh. Use a well-conditioned
        // example: diag(1, 2, 3), no shear → eigenvalues (1, 2, 3), max 3.
        let e = StrainTensor::new(1.0, 2.0, 3.0, 0.0, 0.0, 0.0).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!((e.max_principal().expect("test fixture: literal finite tensor components satisfy from_* preconditions") - 3.0).abs() < 1e-5);
    }

    #[test]
    fn max_principal_pure_shear_xy() {
        // [[0, k, 0], [k, 0, 0], [0, 0, 0]] eigenvalues: -k, 0, k.
        // max = k. Pick k = 0.05.
        let e = StrainTensor::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.05).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        let max = e.max_principal().expect("test fixture: literal finite tensor components satisfy from_* preconditions");
        assert!((max - 0.05).abs() < 1e-5);
    }

    // --- Property tests --------------------------------------------------

    proptest! {
        #[test]
        fn magnitude_is_non_negative_on_finite_inputs(
            xx in any::<f32>().prop_filter("finite", |x| x.is_finite()),
            yy in any::<f32>().prop_filter("finite", |x| x.is_finite()),
            zz in any::<f32>().prop_filter("finite", |x| x.is_finite()),
            yz in any::<f32>().prop_filter("finite", |x| x.is_finite()),
            xz in any::<f32>().prop_filter("finite", |x| x.is_finite()),
            xy in any::<f32>().prop_filter("finite", |x| x.is_finite()),
        ) {
            // Clamp to a moderate range so f32 squaring doesn't overflow
            // into infinity (which would still satisfy is_finite per
            // input but produce non-finite magnitude).
            let clamp = |v: f32| v.clamp(-1e10, 1e10);
            let t = StrainTensor::new(
                clamp(xx), clamp(yy), clamp(zz), clamp(yz), clamp(xz), clamp(xy),
            )
            .expect("test fixture: literal finite tensor components satisfy from_* preconditions");
            let m = t.magnitude();
            prop_assert!(m >= 0.0, "magnitude must be non-negative, got {m}");
            prop_assert!(m.is_finite(), "magnitude must be finite, got {m}");
        }

        #[test]
        fn free_shrinkage_trace_invariant_under_anisotropy(
            linear_pct in 0.0_f32..0.05,
            cure_extent in 0.0_f32..=1.0,
            z_ratio in 0.5_f32..3.0,
        ) {
            let t = StrainTensor::from_free_shrinkage(linear_pct, cure_extent, z_ratio).expect("test fixture: literal finite tensor components satisfy from_* preconditions");
            // Volume-conserving redistribution: trace must equal -3·L·C
            // regardless of the anisotropy ratio. This is the
            // "vendor-data-sheet semantic preservation" invariant.
            let expected_trace = -3.0 * linear_pct * cure_extent;
            let trace = t.xx() + t.yy() + t.zz();
            prop_assert!(
                (trace - expected_trace).abs() < 1e-5,
                "trace {} != expected {} (linear_pct={linear_pct}, cure={cure_extent}, r={z_ratio})",
                trace,
                expected_trace,
            );
        }

        #[test]
        fn nan_in_any_slot_rejected(slot in 0u8..6) {
            let mut c = [0.0_f32; 6];
            c[slot as usize] = f32::NAN;
            let r = StrainTensor::new(c[0], c[1], c[2], c[3], c[4], c[5]);
            prop_assert!(r.is_err());
        }
    }
}
