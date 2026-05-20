//! 6-component symmetric stress tensor — `StressTensor` value object.
//!
//! ADR-0018, t2f3. Stores the symmetric Cauchy stress tensor in Voigt
//! notation as six f32 components `[σ_xx, σ_yy, σ_zz, σ_yz, σ_xz, σ_xy]`
//! in MPa. Companion to `StrainTensor`; the linear-elastic mapping
//! `σ = D : ε` lives on this type as `from_strain_linear_elastic`.
//!
//! # NaN policy
//!
//! Two-layer defence per `docs/patterns/nan-two-layer-defence.md` — the
//! constructor checks every component for `is_finite()` before touching
//! state. Stress components are signed (negative = compressive) so the
//! validator does NOT collapse "finite" into "positive" (anti-pattern
//! `rust-nan-positive-validation-gap`).
//!
//! # Closed-form stiffness
//!
//! For an isotropic linear-elastic material with Young's modulus `E` and
//! Poisson's ratio `ν`, the 6×6 Voigt stiffness `D` is
//!
//! ```text
//! D_ii = E·(1 − ν) / ((1 + ν)(1 − 2ν))    for normal-normal diagonal
//! D_ij = E·ν / ((1 + ν)(1 − 2ν))          for normal-normal off-diagonal
//! D_ss = E / (2·(1 + ν))                  for shear-shear (G = shear modulus)
//! ```
//!
//! ν → 0.5 is the incompressible limit and makes the denominator
//! `(1 − 2ν)` go to zero. `ResinProfile.validate()` rejects ν ≥ 0.5 so
//! the calculator does not need to defend against the divide-by-zero,
//! but the `from_strain_linear_elastic` constructor still returns
//! `Result` for the defensive case where a hostile test fixture writes
//! ν directly via `pub(crate)` mutation.
//!
//! See `docs/kb/KB-162-linear-elasticity-stress-accumulator.md` for the
//! derivation, small-strain validity (≈ 5 % upper bound), and
//! literature citations.

#![cfg(feature = "field-sim")]

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::values::strain_tensor::StrainTensor;

/// Errors from `StressTensor` construction and access.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum StressTensorError {
    #[error(
        "StressTensor components must all be finite, got ({xx}, {yy}, {zz}, {yz}, {xz}, {xy})"
    )]
    NonFiniteComponent {
        xx: f32,
        yy: f32,
        zz: f32,
        yz: f32,
        xz: f32,
        xy: f32,
    },
    #[error("from_strain_linear_elastic: Young's modulus must be finite and > 0, got {0}")]
    InvalidYoungsModulus(f32),
    #[error(
        "from_strain_linear_elastic: Poisson's ratio must be finite and strictly in (-1, 0.5), got {0}"
    )]
    InvalidPoissonsRatio(f32),
    #[error("from_strain_linear_elastic: stiffness multiplication produced non-finite stress")]
    InvalidDerivation,
    #[error("von_mises: derived a non-finite value (catastrophic cancellation)")]
    NonFiniteDerivation,
}

/// Symmetric stress tensor in Voigt notation `[σ_xx, σ_yy, σ_zz, σ_yz, σ_xz, σ_xy]`.
///
/// All six components are f32, in MPa. Negative σ_ii is compressive
/// stress, positive is tensile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StressTensor {
    components: [f32; 6],
}

impl StressTensor {
    /// Zero stress — all six components 0. Cannot fail.
    pub fn zero() -> Self {
        Self {
            components: [0.0; 6],
        }
    }

    /// New stress tensor from Voigt-ordered components. Validates each
    /// is finite; rejects NaN / ±∞.
    pub fn new(
        xx: f32,
        yy: f32,
        zz: f32,
        yz: f32,
        xz: f32,
        xy: f32,
    ) -> Result<Self, StressTensorError> {
        if !(xx.is_finite()
            && yy.is_finite()
            && zz.is_finite()
            && yz.is_finite()
            && xz.is_finite()
            && xy.is_finite())
        {
            return Err(StressTensorError::NonFiniteComponent {
                xx,
                yy,
                zz,
                yz,
                xz,
                xy,
            });
        }
        Ok(Self {
            components: [xx, yy, zz, yz, xz, xy],
        })
    }

    /// Stress from strain via isotropic linear elasticity `σ = D : ε`.
    ///
    /// Closed-form 6×6 Voigt stiffness derived from Young's modulus
    /// `E_mpa` and Poisson's ratio `poissons`. See module docs for the
    /// algebra; KB-162 for the derivation + small-strain validity bound.
    ///
    /// Returns `Err` when `E_mpa` ≤ 0 or non-finite, or `poissons` is
    /// outside the strict physical range `(-1, 0.5)`. The intended
    /// gate is `ResinProfile.validate()` (rejects the same domain), so
    /// well-constructed callers never hit these errors.
    pub fn from_strain_linear_elastic(
        epsilon: &StrainTensor,
        e_mpa: f32,
        poissons: f32,
    ) -> Result<Self, StressTensorError> {
        if !e_mpa.is_finite() || e_mpa <= 0.0 {
            return Err(StressTensorError::InvalidYoungsModulus(e_mpa));
        }
        if !poissons.is_finite() || poissons <= -1.0 || poissons >= 0.5 {
            return Err(StressTensorError::InvalidPoissonsRatio(poissons));
        }

        // Closed-form isotropic stiffness coefficients.
        let denom = (1.0 + poissons) * (1.0 - 2.0 * poissons);
        let d_diag = e_mpa * (1.0 - poissons) / denom; // normal-normal diagonal
        let d_off = e_mpa * poissons / denom; // normal-normal off-diagonal
        let g = e_mpa / (2.0 * (1.0 + poissons)); // shear modulus G

        let eps = epsilon.components();
        // σ_normal_i = D_diag · ε_ii + D_off · (sum of other two ε_jj)
        let s_xx = d_diag * eps[0] + d_off * (eps[1] + eps[2]);
        let s_yy = d_diag * eps[1] + d_off * (eps[0] + eps[2]);
        let s_zz = d_diag * eps[2] + d_off * (eps[0] + eps[1]);
        // Engineering shear strain γ = 2·ε_ij; using ε_ij directly with
        // Voigt convention means D_shear · ε_ij × 2 = 2G · ε_ij. Some
        // texts absorb the 2 into the strain definition; we follow the
        // "stress = 2G × tensor-shear" convention to stay consistent
        // with `StrainTensor`'s shear semantics (engineering shear).
        let s_yz = 2.0 * g * eps[3];
        let s_xz = 2.0 * g * eps[4];
        let s_xy = 2.0 * g * eps[5];

        if !(s_xx.is_finite()
            && s_yy.is_finite()
            && s_zz.is_finite()
            && s_yz.is_finite()
            && s_xz.is_finite()
            && s_xy.is_finite())
        {
            return Err(StressTensorError::InvalidDerivation);
        }
        Ok(Self {
            components: [s_xx, s_yy, s_zz, s_yz, s_xz, s_xy],
        })
    }

    /// `σ_xx` — normal stress along x (MPa).
    pub fn xx(&self) -> f32 {
        self.components[0]
    }
    /// `σ_yy` — normal stress along y (MPa).
    pub fn yy(&self) -> f32 {
        self.components[1]
    }
    /// `σ_zz` — normal stress along z (MPa).
    pub fn zz(&self) -> f32 {
        self.components[2]
    }
    /// `σ_yz` — shear stress in the y-z plane (MPa).
    pub fn yz(&self) -> f32 {
        self.components[3]
    }
    /// `σ_xz` — shear stress in the x-z plane (MPa).
    pub fn xz(&self) -> f32 {
        self.components[4]
    }
    /// `σ_xy` — shear stress in the x-y plane (MPa).
    pub fn xy(&self) -> f32 {
        self.components[5]
    }

    /// Raw Voigt components `[σ_xx, σ_yy, σ_zz, σ_yz, σ_xz, σ_xy]`.
    pub fn components(&self) -> [f32; 6] {
        self.components
    }

    /// Hydrostatic stress (mean normal stress) — `(σ_xx + σ_yy + σ_zz) / 3` (MPa).
    pub fn hydrostatic_mpa(&self) -> f32 {
        (self.components[0] + self.components[1] + self.components[2]) / 3.0
    }

    /// Von Mises equivalent stress (MPa) — scalar yield-style magnitude.
    ///
    /// `σ_vm = √(½·[(σ_xx − σ_yy)² + (σ_yy − σ_zz)² + (σ_zz − σ_xx)²]
    ///             + 3·(σ_yz² + σ_xz² + σ_xy²))`
    ///
    /// Always non-negative on finite inputs. Returns `Err` only on
    /// catastrophic floating-point cancellation producing non-finite
    /// intermediates.
    pub fn von_mises_mpa(&self) -> Result<f32, StressTensorError> {
        let [xx, yy, zz, yz, xz, xy] = self.components;
        let normal_diff_sq = (xx - yy) * (xx - yy) + (yy - zz) * (yy - zz) + (zz - xx) * (zz - xx);
        let shear_sq = yz * yz + xz * xz + xy * xy;
        let inner = 0.5 * normal_diff_sq + 3.0 * shear_sq;
        if !inner.is_finite() || inner < 0.0 {
            return Err(StressTensorError::NonFiniteDerivation);
        }
        let vm = inner.sqrt();
        if !vm.is_finite() {
            return Err(StressTensorError::NonFiniteDerivation);
        }
        Ok(vm)
    }

    /// Maximum principal stress (largest eigenvalue of the 3×3 symmetric
    /// tensor, MPa). Closed-form trigonometric solution; see the
    /// `StrainTensor::max_principal` derivation.
    pub fn max_principal_mpa(&self) -> Result<f32, StressTensorError> {
        let [xx, yy, zz, yz, xz, xy] = self.components;
        let p1 = xy * xy + xz * xz + yz * yz;

        if p1 == 0.0 {
            let m = xx.max(yy).max(zz);
            if !m.is_finite() {
                return Err(StressTensorError::NonFiniteDerivation);
            }
            return Ok(m);
        }

        let q = (xx + yy + zz) / 3.0;
        let p2 = (xx - q) * (xx - q) + (yy - q) * (yy - q) + (zz - q) * (zz - q) + 2.0 * p1;
        let p = (p2 / 6.0).sqrt();
        if !p.is_finite() || p == 0.0 {
            // Already handled p1 == 0 above; this guards against degenerate
            // numerical zeros from catastrophic cancellation.
            return Err(StressTensorError::NonFiniteDerivation);
        }

        let b_xx = (xx - q) / p;
        let b_yy = (yy - q) / p;
        let b_zz = (zz - q) / p;
        let b_yz = yz / p;
        let b_xz = xz / p;
        let b_xy = xy / p;

        let det_b = b_xx * (b_yy * b_zz - b_yz * b_yz) - b_xy * (b_xy * b_zz - b_yz * b_xz)
            + b_xz * (b_xy * b_yz - b_yy * b_xz);
        let r = (det_b / 2.0).clamp(-1.0, 1.0);
        let phi = r.acos() / 3.0;
        let eig1 = q + 2.0 * p * phi.cos();
        if !eig1.is_finite() {
            return Err(StressTensorError::NonFiniteDerivation);
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
        assert_eq!(StressTensor::zero().components(), [0.0; 6]);
    }

    #[test]
    fn zero_von_mises_is_zero() {
        assert_eq!(StressTensor::zero().von_mises_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions"), 0.0);
    }

    #[test]
    fn new_rejects_nan() {
        assert!(StressTensor::new(f32::NAN, 0.0, 0.0, 0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn new_rejects_infinity() {
        assert!(StressTensor::new(f32::INFINITY, 0.0, 0.0, 0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn from_strain_zero_strain_gives_zero_stress() {
        let e = StrainTensor::zero();
        let s = StressTensor::from_strain_linear_elastic(&e, 2000.0, 0.35).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert_eq!(s, StressTensor::zero());
    }

    #[test]
    fn from_strain_isotropic_strain_gives_isotropic_stress() {
        // Hydrostatic strain → hydrostatic stress: invariant of the
        // closed-form linear-elasticity model.
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        let s = StressTensor::from_strain_linear_elastic(&e, 2000.0, 0.35).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        // Normal components all equal; shears zero.
        assert!((s.xx() - s.yy()).abs() < 1e-3);
        assert!((s.yy() - s.zz()).abs() < 1e-3);
        assert_eq!(s.yz(), 0.0);
        assert_eq!(s.xz(), 0.0);
        assert_eq!(s.xy(), 0.0);
        // Sign matches strain (compressive ε → compressive σ).
        assert!(s.xx() < 0.0);
    }

    #[test]
    fn from_strain_pure_shear_gives_pure_shear_stress() {
        // ε_xy = γ/2 (tensor shear) → σ_xy = 2G·ε_xy = G·γ.
        let e = StrainTensor::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.005).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        let e_mpa = 2000.0;
        let nu = 0.35;
        let s = StressTensor::from_strain_linear_elastic(&e, e_mpa, nu).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        // Normals all zero.
        assert!(s.xx().abs() < 1e-3);
        assert!(s.yy().abs() < 1e-3);
        assert!(s.zz().abs() < 1e-3);
        // σ_xy = 2G · ε_xy = E / (1+ν) · ε_xy.
        let expected = e_mpa / (1.0 + nu) * 0.005;
        assert!((s.xy() - expected).abs() / expected.abs() < 1e-4);
    }

    #[test]
    fn from_strain_rejects_invalid_e() {
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!(StressTensor::from_strain_linear_elastic(&e, 0.0, 0.35).is_err());
        assert!(StressTensor::from_strain_linear_elastic(&e, -100.0, 0.35).is_err());
        assert!(StressTensor::from_strain_linear_elastic(&e, f32::NAN, 0.35).is_err());
    }

    #[test]
    fn from_strain_rejects_incompressible_nu() {
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!(StressTensor::from_strain_linear_elastic(&e, 2000.0, 0.5).is_err());
        assert!(StressTensor::from_strain_linear_elastic(&e, 2000.0, 0.6).is_err());
    }

    #[test]
    fn from_strain_rejects_invalid_nu_lower() {
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!(StressTensor::from_strain_linear_elastic(&e, 2000.0, -1.0).is_err());
        assert!(StressTensor::from_strain_linear_elastic(&e, 2000.0, -2.0).is_err());
    }

    #[test]
    fn from_strain_accepts_nu_zero_rod_like() {
        let e = StrainTensor::from_isotropic(-0.01).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!(StressTensor::from_strain_linear_elastic(&e, 2000.0, 0.0).is_ok());
    }

    #[test]
    fn von_mises_diagonal_stress() {
        // Uniaxial σ_xx = 100, all others zero → σ_vm = 100.
        let s = StressTensor::new(100.0, 0.0, 0.0, 0.0, 0.0, 0.0).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!((s.von_mises_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions") - 100.0).abs() < 1e-3);
    }

    #[test]
    fn von_mises_pure_shear() {
        // Pure shear σ_xy = τ → σ_vm = √3 · τ.
        let s = StressTensor::new(0.0, 0.0, 0.0, 0.0, 0.0, 10.0).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        let expected = 3.0_f32.sqrt() * 10.0;
        assert!((s.von_mises_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions") - expected).abs() < 1e-3);
    }

    #[test]
    fn von_mises_hydrostatic_is_zero() {
        // σ = σ₀·I → no deviatoric component → σ_vm = 0.
        let s = StressTensor::new(50.0, 50.0, 50.0, 0.0, 0.0, 0.0).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!(s.von_mises_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions") < 1e-3);
    }

    #[test]
    fn hydrostatic_is_mean_normal() {
        let s = StressTensor::new(30.0, -30.0, 60.0, 0.0, 0.0, 0.0).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!((s.hydrostatic_mpa() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn max_principal_diagonal_picks_largest_signed() {
        let s = StressTensor::new(-50.0, 80.0, 10.0, 0.0, 0.0, 0.0).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
        assert!((s.max_principal_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions") - 80.0).abs() < 1e-3);
    }

    proptest! {
        #[test]
        fn von_mises_is_non_negative_on_finite_inputs(
            xx in -1e6_f32..1e6,
            yy in -1e6_f32..1e6,
            zz in -1e6_f32..1e6,
            yz in -1e6_f32..1e6,
            xz in -1e6_f32..1e6,
            xy in -1e6_f32..1e6,
        ) {
            let s = StressTensor::new(xx, yy, zz, yz, xz, xy).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
            let vm = s.von_mises_mpa().expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
            prop_assert!(vm >= 0.0, "von mises must be ≥ 0, got {vm}");
            prop_assert!(vm.is_finite());
        }

        #[test]
        fn from_strain_preserves_zero(e_mpa in 100.0_f32..1e5, nu in -0.99_f32..0.49) {
            let e = StrainTensor::zero();
            let s = StressTensor::from_strain_linear_elastic(&e, e_mpa, nu).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
            prop_assert_eq!(s, StressTensor::zero());
        }

        #[test]
        fn from_strain_sign_follows_isotropic_input(
            epsilon in -0.04_f32..0.04,
            e_mpa in 100.0_f32..1e5,
            nu in 0.0_f32..0.49,
        ) {
            // Skip very small ε that the float math collapses to 0.
            prop_assume!(epsilon.abs() > 1e-6);
            let e = StrainTensor::from_isotropic(epsilon).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
            let s = StressTensor::from_strain_linear_elastic(&e, e_mpa, nu).expect("test fixture: literal finite stress components and validated E + nu satisfy preconditions");
            // For ν < 0.5, denom > 0 and (1−ν) > 0 so D_diag > 0, sign
            // of σ_xx matches sign of ε.
            prop_assert!(s.xx().signum() == epsilon.signum() || s.xx() == 0.0);
        }
    }
}
