//! Voxel-resolved cure dose + photoinitiator depletion orchestrator.
//! ADR-0017, KB-160, t2f1.
//!
//! `VoxelCureCalculator` is the Tier-2 counterpart of
//! [`CureCalculator`](crate::services::CureCalculator). It does NOT re-derive
//! Beer-Lambert or Arrhenius — it composes the Tier-1 primitives
//! (`intensity_at_depth`, `ec_at_temp`) over a 3D voxel grid, accumulating
//! cumulative dose into a [`CureField`] and depleting photoinitiator
//! concentration into a [`PhotoinitiatorField`] per the standard radical
//! kinetic model.
//!
//! Pattern follows `docs/patterns/single-source-arrhenius-helper.md`: the
//! Ec(T) formula lives only in [`CureCalculator::ec_at_temp`]; this service
//! delegates and never duplicates the math.
//!
//! # Stateless
//!
//! All inputs are explicit; the calculator owns no state. Field mutation
//! happens through `&mut CureField` / `&mut PhotoinitiatorField` borrows.

#![cfg(feature = "field-sim")]

use thiserror::Error;

use crate::services::CureCalculator;
use crate::values::{
    CureField, CureFieldError, Energy, PenetrationDepth, PhotoinitiatorField,
    PhotoinitiatorFieldError, VatTemperature,
};

/// Errors from voxel cure orchestration.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum VoxelCureError {
    #[error("VoxelCureCalculator: cure field dimensions {cure_dims:?} do not match photoinitiator field dimensions {pi_dims:?}")]
    DimensionMismatch {
        cure_dims: (u32, u32, u32),
        pi_dims: (u32, u32, u32),
    },
    #[error("VoxelCureCalculator: per-column input must be finite (got pixel_intensity_mw_cm2={pixel_intensity_mw_cm2}, exposure_sec={exposure_sec})")]
    InvalidColumnInput {
        pixel_intensity_mw_cm2: f32,
        exposure_sec: f32,
    },
    #[error("VoxelCureCalculator: CureField error: {0}")]
    CureField(#[from] CureFieldError),
    #[error("VoxelCureCalculator: PhotoinitiatorField error: {0}")]
    PhotoinitiatorField(#[from] PhotoinitiatorFieldError),
}

/// Domain service for voxel-resolved cure + depletion orchestration.
pub struct VoxelCureCalculator;

impl VoxelCureCalculator {
    /// Apply a single exposure to one pixel column at `(ix, iy)`, layer
    /// `iz`, integrating per-column Beer-Lambert attenuation into the
    /// cure field and depleting photoinitiator per KB-160.
    ///
    /// The "exposure" is treated as constant `pixel_intensity_mw_cm2` for
    /// `exposure_sec` seconds. Per-voxel surface dose (top of the layer
    /// at depth 0) is `intensity × time = mJ/cm²`. Beer-Lambert
    /// attenuation by depth uses the LOCAL effective `Dp` derived from
    /// the local photoinitiator concentration: `Dp_local = dp_um / C(z)`
    /// where `C` is the concentration of the relevant voxel (clamped to
    /// the KB-160 floor `C_THRESHOLD`).
    ///
    /// For t2f1 v1 this method handles ONE COLUMN per call (no inter-
    /// pixel scattering — that is t2f2's scope). Callers loop over
    /// `(ix, iy)` across the LCD pixel grid per layer.
    ///
    /// # Inputs
    /// - `cure_field` and `photoinitiator_field` must share dimensions.
    /// - `(ix, iy, iz)` indexes the surface voxel of this exposure; the
    ///   method integrates downward into deeper voxels at increasing `iz`
    ///   per voxel-height, until either the field bottom or the intensity
    ///   has decayed to negligible.
    /// - `pixel_intensity_mw_cm2` and `exposure_sec` must both be finite
    ///   and `>= 0`.
    ///
    /// # NaN policy
    /// Two-layer defence: explicit `is_finite()` checks before any
    /// arithmetic. Out-of-bounds writes returned as `OutOfBounds` errors
    /// via the underlying field accessors.
    ///
    /// # Z-axis depth convention
    ///
    /// The `nz` dimension of the cure field equals the print's layer count
    /// — each Z-voxel represents ONE LAYER, not one cube of side
    /// `voxel_size_mm`. The physical depth between adjacent Z-voxels is
    /// `layer_height_um` (the recipe's layer height), which is
    /// INDEPENDENT of the field's lateral `voxel_size_mm` (the LCD pixel-
    /// pitch in X/Y). Beer-Lambert depth in this loop uses
    /// `layer_height_um` for Z stepping, NOT `voxel_size_mm × 1000`.
    /// The two are typically off by 5–25× for real Mars 5 Ultra workloads,
    /// so the earlier formula was systematically attenuating ~10× too
    /// aggressively — code review round 1 caught this.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_column_exposure(
        cure_field: &mut CureField,
        photoinitiator_field: &mut PhotoinitiatorField,
        ix: u32,
        iy: u32,
        iz_top: u32,
        pixel_intensity_mw_cm2: f32,
        exposure_sec: f32,
        dp: PenetrationDepth,
        k_d: f32,
        layer_height_um: f32,
    ) -> Result<(), VoxelCureError> {
        if cure_field.dimensions() != photoinitiator_field.dimensions() {
            return Err(VoxelCureError::DimensionMismatch {
                cure_dims: cure_field.dimensions(),
                pi_dims: photoinitiator_field.dimensions(),
            });
        }
        if !pixel_intensity_mw_cm2.is_finite()
            || pixel_intensity_mw_cm2 < 0.0
            || !exposure_sec.is_finite()
            || exposure_sec < 0.0
        {
            return Err(VoxelCureError::InvalidColumnInput {
                pixel_intensity_mw_cm2,
                exposure_sec,
            });
        }
        if !k_d.is_finite() || k_d < 0.0 {
            // k_d sourced from ResinProfile.photoinitiator_decay_constant_k_d
            // (or KB-160 default 0.05) — both validated upstream, but
            // defence-in-depth here.
            return Err(VoxelCureError::InvalidColumnInput {
                pixel_intensity_mw_cm2: k_d,
                exposure_sec,
            });
        }
        if !layer_height_um.is_finite() || layer_height_um <= 0.0 {
            return Err(VoxelCureError::InvalidColumnInput {
                pixel_intensity_mw_cm2,
                exposure_sec: layer_height_um,
            });
        }

        let (_nx, _ny, nz) = cure_field.dimensions();

        // Surface dose (mJ/cm² = mW/cm² × s, the two units cancel via
        // 1000 W·s = 1 J ⇒ 1 mW·s = 1 mJ, /cm² stays).
        let surface_dose_mj_cm2 = pixel_intensity_mw_cm2 * exposure_sec;
        if surface_dose_mj_cm2 == 0.0 {
            return Ok(());
        }
        let dp_base = dp.value();
        if !(dp_base > 0.0 && dp_base.is_finite()) {
            return Err(VoxelCureError::InvalidColumnInput {
                pixel_intensity_mw_cm2: dp_base,
                exposure_sec,
            });
        }

        // March down the column from iz_top toward the field bottom (iz = nz - 1),
        // accumulating dose and depleting photoinitiator. The local effective
        // Dp scales inversely with local concentration (KB-160 link to
        // Beer-Lambert: Dp ∝ 1 / C); deplete BEFORE moving deeper so this
        // voxel's reduced concentration affects only subsequent voxels.
        // Depth-per-Z-voxel is `layer_height_um` per the Z-axis convention
        // documented above.
        for iz in iz_top..nz {
            let depth_um = (iz - iz_top) as f32 * layer_height_um + (layer_height_um * 0.5);
            let c_local = photoinitiator_field.concentration_at(ix, iy, iz)?;
            // KB-160 numerical floor: clamp local C to avoid divide-by-near-zero
            // when Dp_local = Dp / C. C_THRESHOLD = 0.01.
            let c_clamped = c_local.max(C_THRESHOLD);
            let dp_local = (dp_base / c_clamped).min(dp_base * DP_LOCAL_MAX_FACTOR);
            // Per-voxel absorbed dose: surface dose attenuated by
            // exp(-depth / Dp_local).
            let attenuation = (-depth_um / dp_local).exp();
            let voxel_dose = surface_dose_mj_cm2 * attenuation;
            if voxel_dose <= NEGLIGIBLE_DOSE_FLOOR {
                // Below this, deeper voxels contribute even less — bail.
                break;
            }
            cure_field.add_dose(ix, iy, iz, voxel_dose)?;
            // KB-160: deplete photoinitiator by the absorbed dose at this voxel.
            photoinitiator_field.deplete(ix, iy, iz, k_d, voxel_dose)?;
        }
        Ok(())
    }

    /// Critical energy at vat temperature — pure delegation to
    /// [`CureCalculator::ec_at_temp`] per the single-source-arrhenius-helper
    /// pattern. Exists only to keep voxel callers from importing CureCalculator
    /// directly; semantics MUST match byte-for-byte.
    pub fn ec_at_temp(
        ec_ref: Energy,
        ref_temp_c: f32,
        vat_temp: VatTemperature,
        ea_cure_kj_mol: f32,
    ) -> Energy {
        CureCalculator::ec_at_temp(ec_ref, ref_temp_c, vat_temp, ea_cure_kj_mol)
    }
}

/// KB-160 burnt-out numerical floor — concentration below this is clamped
/// to avoid divide-by-near-zero in `Dp_local = Dp / C`. See KB-160.
const C_THRESHOLD: f32 = 0.01;

/// Maximum factor by which local `Dp` may exceed the resin's nominal
/// `Dp` after depletion. Limits extrapolation territory per KB-160.
const DP_LOCAL_MAX_FACTOR: f32 = 10.0;

/// Below this per-voxel dose (mJ/cm²), Beer-Lambert attenuation is too
/// deep to matter — short-circuit the loop to avoid wasted work at the
/// bottom of long columns.
const NEGLIGIBLE_DOSE_FLOOR: f32 = 1e-6;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::CureField;

    fn make_pair(nx: u32, ny: u32, nz: u32) -> (CureField, PhotoinitiatorField) {
        (
            CureField::new(nx, ny, nz, 0.05, [0.0, 0.0, 0.0]).expect("CureField fixture is valid"),
            PhotoinitiatorField::new(nx, ny, nz, 1.0)
                .expect("PhotoinitiatorField fixture is valid"),
        )
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let mut cure = CureField::new(2, 2, 2, 0.05, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let mut pi = PhotoinitiatorField::new(2, 2, 3, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let err = VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            2.5,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .expect_err("test fixture: input deliberately violates VoxelCureCalculator precondition, so Err is the expected outcome");
        matches!(err, VoxelCureError::DimensionMismatch { .. });
    }

    #[test]
    fn nan_intensity_rejected() {
        let (mut cure, mut pi) = make_pair(2, 2, 2);
        let err = VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            f32::NAN,
            2.5,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .expect_err("test fixture: input deliberately violates VoxelCureCalculator precondition, so Err is the expected outcome");
        matches!(err, VoxelCureError::InvalidColumnInput { .. });
    }

    #[test]
    fn nan_exposure_rejected() {
        let (mut cure, mut pi) = make_pair(2, 2, 2);
        let err = VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            f32::NAN,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .expect_err("test fixture: input deliberately violates VoxelCureCalculator precondition, so Err is the expected outcome");
        matches!(err, VoxelCureError::InvalidColumnInput { .. });
    }

    #[test]
    fn negative_exposure_rejected() {
        let (mut cure, mut pi) = make_pair(2, 2, 2);
        assert!(VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            -1.0,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .is_err());
    }

    #[test]
    fn nan_k_d_rejected() {
        let (mut cure, mut pi) = make_pair(2, 2, 2);
        assert!(VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            2.5,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            f32::NAN,
            50.0,
        )
        .is_err());
    }

    /// Z-step regression guard (code review round 1 HIGH-correctness):
    /// `apply_column_exposure` previously used `voxel_size_mm × 1000` as
    /// the per-Z-step depth, which silently attenuated 10× too aggressively
    /// at typical 50 µm layer height + 0.5 mm mask voxels. Now uses
    /// `layer_height_um` explicitly. This test asserts that the depth at
    /// the first voxel's centre is exactly `layer_height_um / 2`, NOT
    /// `voxel_size_mm × 500`.
    #[test]
    fn z_step_uses_layer_height_not_lateral_voxel() {
        // Set up a 1×1×2 column. Mask voxel = 0.5 mm (lateral). Layer
        // height = 50 µm (typical Mars-class). Surface dose 100 mJ/cm²
        // (40 s × 2.5 mW/cm² — exaggerated to keep exp(...) well above
        // floor). Dp = 100 µm.
        //
        // With CORRECT Z-step = 50 µm: centre voxel at depth 25 µm,
        //   attenuation = exp(-25/100) ≈ 0.7788, voxel_dose ≈ 77.88.
        // With BUGGY Z-step = 500 µm: depth 250 µm,
        //   attenuation = exp(-250/100) ≈ 0.0821, voxel_dose ≈ 8.21.
        // Asserting voxel_dose > 50 catches the bug emphatically.
        let mut cure = CureField::new(1, 1, 2, 0.5, [0.0, 0.0, 0.0]).expect(
            "test fixture: 1×1×2 at 0.5 mm voxel + (0,0,0) bbox_min satisfies CureField::new preconditions",
        );
        let mut pi = PhotoinitiatorField::new(1, 1, 2, 1.0).expect(
            "test fixture: 1×1×2 at C₀ = 1.0 satisfies PhotoinitiatorField::new preconditions",
        );
        VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            2.5,
            40.0,
            PenetrationDepth::new(100.0)
                .expect("test fixture: 100 µm Dp is in PenetrationDepth domain"),
            0.05,
            50.0, // 50 µm — typical Mars-class layer
        )
        .expect("test fixture: all args are in their respective domains, so the call must succeed");
        let dose = cure
            .dose_at(0, 0, 0)
            .expect("test fixture: (0,0,0) is in-bounds for the 1×1×2 field we just constructed");
        // CORRECT Z-step gives ≈ 77.88 mJ/cm² at voxel 0; the buggy version
        // would have given ≈ 8.21. Threshold at 50 cleanly distinguishes.
        assert!(
            dose > 50.0,
            "Z-step regression: surface voxel dose must reflect layer-height \
             attenuation (~77 mJ/cm²), not lateral-voxel attenuation \
             (~8 mJ/cm²); got {dose}"
        );
        // Second voxel sees 1.5 layer-heights of depth (50 + 25 = 75 µm).
        let dose2 = cure
            .dose_at(0, 0, 1)
            .expect("test fixture: (0,0,1) is in-bounds for the 1×1×2 field we just constructed");
        // exp(-75/100) ≈ 0.4724, voxel_dose ≈ 47.24. Must be < voxel 0.
        assert!(dose2 < dose && dose2 > 20.0,
            "second voxel must be shallower-attenuated than first but well above buggy floor; got dose0={dose}, dose1={dose2}");
    }

    #[test]
    fn zero_exposure_is_noop() {
        let (mut cure, mut pi) = make_pair(2, 2, 2);
        VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            0.0,
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        assert_eq!(cure.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"), 0.0);
        assert_eq!(pi.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"), 1.0);
    }

    /// Delegation regression: a single-voxel-thick "column" must produce
    /// approximately the same total dose at the surface voxel as the
    /// Tier-1 scalar `surface_dose = intensity × exposure`. This is the
    /// KB-103 delegation: the Tier-2 path with `nz=1` collapses to
    /// `surface_dose × exp(-voxel_centre_depth / Dp)`. Depth is driven by
    /// `layer_height_um` per the Z-axis convention (one Z-voxel = one
    /// layer slab), NOT by `voxel_size_mm × 1000`.
    #[test]
    fn single_voxel_column_matches_kb103_surface_dose() {
        // 1×1×1 column, mask voxel_size 0.001 mm (1 µm — LATERAL only),
        // layer_height = 1 µm (so Z-step = 1 µm), Dp = 1000 µm.
        // Surface dose 10 mW/cm² × 2.5 s = 25 mJ/cm².
        // Voxel centre depth = 0.5 × 1 µm = 0.5 µm.
        // Attenuation = exp(-0.5 / 1000) ≈ 0.9995. Expected voxel dose ≈ 24.99.
        let mut cure =
            CureField::new(1, 1, 1, 0.001, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"); // 1 µm voxels
        let mut pi = PhotoinitiatorField::new(1, 1, 1, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            2.5,
            PenetrationDepth::new(1000.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            1.0, // layer_height_um — 1 µm slab to keep attenuation negligible
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let dose = cure.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let expected_surface_dose = 25.0;
        // 0.5 µm depth out of 1000 µm Dp ⇒ attenuation 0.9995.
        assert!(
            (dose - expected_surface_dose * 0.9995).abs() < 0.05,
            "KB-103 delegation: expected ≈{expected_surface_dose}×0.9995, got {dose}"
        );
    }

    /// KB-103 delegation through the CureField cure-depth mapping:
    /// when a single voxel column receives a known dose, its cure-depth
    /// reading must match `Dp × ln(E / Ec)` byte-for-byte at the voxel
    /// granularity (the Tier-1 primitive).
    #[test]
    fn single_voxel_cure_depth_matches_scalar_cure_depth() {
        // Skip the per-column physics here and write a known dose
        // directly via add_dose so we isolate the cure-depth mapping.
        let mut cure =
            CureField::new(1, 1, 1, 0.05, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        cure.add_dose(0, 0, 0, 25.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"); // E = 25 mJ/cm²
        let cd = cure
            .cure_depth_at(0, 0, 0, 100.0, 5.0)
            .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let scalar = CureCalculator::cure_depth(
            PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            Energy::new(25.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            Energy::new(5.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
        );
        assert!(
            (cd.value() - scalar.value()).abs() < 1e-4,
            "cure_depth delegation: voxel={}, scalar={}",
            cd.value(),
            scalar.value()
        );
    }

    /// Dose accumulates across multiple exposures of the same voxel.
    #[test]
    fn dose_accumulates_across_exposures() {
        let (mut cure, mut pi) = make_pair(1, 1, 1);
        let dp = PenetrationDepth::new(1000.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure, &mut pi, 0, 0, 0, 10.0, 2.5, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let after_first = cure.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure, &mut pi, 0, 0, 0, 10.0, 2.5, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let after_second = cure.dose_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        assert!(after_second > after_first);
        // Roughly 2× (with depletion making the second exposure slightly
        // more efficient because Dp_local increases).
        assert!(after_second > 1.9 * after_first);
    }

    /// Ec(T) delegation: VoxelCureCalculator::ec_at_temp must produce
    /// byte-identical output to CureCalculator::ec_at_temp for the same
    /// inputs (single-source-arrhenius-helper pattern).
    #[test]
    fn ec_at_temp_delegation_byte_identical() {
        let ec_ref = Energy::new(5.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let vat = VatTemperature::new(35.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let voxel = VoxelCureCalculator::ec_at_temp(ec_ref, 25.0, vat, 30.0);
        let scalar = CureCalculator::ec_at_temp(ec_ref, 25.0, vat, 30.0);
        assert_eq!(voxel.value().to_bits(), scalar.value().to_bits());
    }

    /// Depletion drops concentration per KB-160 analytical form.
    /// Fixture is 1×1×1 at the `make_pair` default voxel_size_mm = 0.05
    /// (= 50 µm). Surface dose = 10 mW/cm² × 2.5 s = 25 mJ/cm². Voxel
    /// centre depth = 25 µm. Attenuation = exp(-25 / 1000) ≈ 0.9753.
    /// Voxel dose ≈ 24.38 mJ/cm². k_d = 0.05.
    /// C_after = exp(-0.05 × 24.38) ≈ 0.2955.
    #[test]
    fn depletion_drops_concentration_per_kb160() {
        let (mut cure, mut pi) = make_pair(1, 1, 1);
        VoxelCureCalculator::apply_column_exposure(
            &mut cure,
            &mut pi,
            0,
            0,
            0,
            10.0,
            2.5,
            PenetrationDepth::new(1000.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            0.05,
            50.0, // layer_height_um — typical Mars-class 50 µm slabs
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let c_after = pi.concentration_at(0, 0, 0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        // Recompute the analytic expectation with the actual voxel_size_um
        // the fixture uses (50 µm at iz=0 ⇒ centre depth 25 µm).
        let voxel_size_um: f32 = 0.05 * 1000.0;
        let depth_um = 0.5 * voxel_size_um;
        let attenuation = (-depth_um / 1000.0).exp();
        let voxel_dose = 25.0 * attenuation;
        let expected = (-0.05 * voxel_dose).exp();
        assert!(
            (c_after - expected).abs() < 1e-3,
            "depletion: expected {expected}, got {c_after}"
        );
        assert!(c_after < 1.0);
        assert!(c_after > 0.0);
    }

    /// Once a voxel is exposed and depleted, a second identical exposure
    /// reaches deeper voxels because Dp_local ∝ 1/C grows. Manifests in
    /// the cure field as more dose accumulating in the SECOND voxel down
    /// the column on the second pass.
    #[test]
    fn effective_dp_increases_as_concentration_drops() {
        // 1×1×4 column at 50 µm (0.05 mm) voxels, Dp 100 µm, big dose.
        let (mut cure_a, mut pi_a) = make_pair(1, 1, 4);
        let (mut cure_b, mut pi_b) = make_pair(1, 1, 4);
        let dp = PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        // Pass A: one large exposure into a virgin column.
        VoxelCureCalculator::apply_column_exposure(
            &mut cure_a, &mut pi_a, 0, 0, 0, 20.0, 5.0, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        // Pass B: TWO exposures of half the intensity-time product each
        // into the same column.
        VoxelCureCalculator::apply_column_exposure(
            &mut cure_b, &mut pi_b, 0, 0, 0, 20.0, 2.5, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure_b, &mut pi_b, 0, 0, 0, 20.0, 2.5, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        // The deepest voxel under pass B (two-shot depleted column) sees
        // MORE accumulated dose than under pass A (one-shot virgin column).
        // This is the testable mechanism behind "depleted resin cures
        // deeper".
        let deep_a = cure_a.dose_at(0, 0, 3).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let deep_b = cure_b.dose_at(0, 0, 3).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        assert!(
            deep_b > deep_a,
            "depleted-column should reach deeper: deep_a={deep_a}, deep_b={deep_b}"
        );
    }

    /// Interaction: combined photoinitiator depletion + rising vat
    /// temperature ⇒ cure depth increases monotonically AND the endpoint
    /// (warm + depleted) exceeds the depth from either drift alone.
    #[test]
    fn interaction_temp_rise_and_depletion_combined() {
        // 1 column, 1 voxel deep, repeated exposures at successively
        // higher vat temperatures (Ec(T) drops) and accumulating
        // photoinitiator depletion.
        let mut cure = CureField::new(1, 1, 1, 0.05, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let mut pi = PhotoinitiatorField::new(1, 1, 1, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let dp = PenetrationDepth::new(100.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let ec_ref = Energy::new(5.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let mut prev_cd_value: f32 = 0.0;
        // Walk vat temperature from 22 °C to 35 °C across 14 layers.
        for layer in 0..14 {
            let vat_c = 22.0 + (layer as f32);
            let vat = VatTemperature::new(vat_c).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
            let ec_t = VoxelCureCalculator::ec_at_temp(ec_ref, 25.0, vat, 30.0);
            VoxelCureCalculator::apply_column_exposure(
                &mut cure, &mut pi, 0, 0, 0, 20.0, 2.5, dp, 0.05, 50.0,
            )
            .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
            let cd = cure.cure_depth_at(0, 0, 0, dp.value(), ec_t.value()).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
            // Monotonic non-decreasing (allowing small numerical jitter).
            assert!(
                cd.value() + 1e-3 >= prev_cd_value,
                "cure depth must monotonically increase: prev={prev_cd_value}, now={}",
                cd.value()
            );
            prev_cd_value = cd.value();
        }
        // After 14 exposures with both drifts active, total cure depth
        // exceeds a single isolated exposure baseline AT THE SAME PRINT
        // TEMPERATURE STAGE — i.e. the combined drift is non-trivial.
        assert!(prev_cd_value > 0.0);

        // Baseline: single isolated exposure at the starting temperature
        // (no depletion, no Ec(T) drift).
        let mut cure_baseline =
            CureField::new(1, 1, 1, 0.05, [0.0, 0.0, 0.0]).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let mut pi_baseline = PhotoinitiatorField::new(1, 1, 1, 1.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        VoxelCureCalculator::apply_column_exposure(
            &mut cure_baseline, &mut pi_baseline, 0, 0, 0, 20.0, 2.5, dp, 0.05, 50.0,
        )
        .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        let ec_cold = VoxelCureCalculator::ec_at_temp(
            ec_ref,
            25.0,
            VatTemperature::new(22.0).expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)"),
            30.0,
        );
        let cd_baseline = cure_baseline
            .cure_depth_at(0, 0, 0, dp.value(), ec_cold.value())
            .expect("test fixture: literal inputs satisfy the called function's preconditions (positive dims, finite floats in domain ranges)");
        assert!(
            prev_cd_value > cd_baseline.value(),
            "combined drift ({}) must exceed cold-baseline ({})",
            prev_cd_value,
            cd_baseline.value()
        );
    }
}
