//! Light crosstalk kernel construction + separable convolution.
//! ADR-0018, t2f2.
//!
//! `LightCrosstalkCalculator` is the kernel-primitive sibling of
//! [`VoxelCureCalculator`](crate::services::VoxelCureCalculator). It does NOT
//! orchestrate cure deposition; it provides the discrete Gaussian kernel
//! and the in-place convolution operators that the orchestrator
//! (`simulation_runner::apply_voxel_cure_for_layer`) wires together with
//! `VoxelCureCalculator::compute_column_exposure` to model intra-LCD +
//! intra-resin 3D light crosstalk.
//!
//! # Scope
//!
//! **IN scope:**
//! - [`build_separable_kernel`](Self::build_separable_kernel): construct a
//!   symmetric 1D normalised Gaussian kernel for radius `⌈3σ⌉`. Used for
//!   BOTH the XY separable convolution and the Z 1D convolution.
//! - [`apply_separable_2d`](Self::apply_separable_2d): in-place separable
//!   2D Gaussian convolution on the per-layer pixel intensity grid
//!   (XY-plane LCD-pixel crosstalk + lateral component of resin scatter).
//! - [`apply_separable_1d_z`](Self::apply_separable_1d_z): in-place 1D
//!   Gaussian convolution along Z on the per-column cure dose column
//!   (axial component of resin volumetric scatter, applied
//!   post-attenuation per ADR-0018 §2 Decision).
//!
//! **OUT of scope:**
//! - Z-direction dispatch logic — the orchestrator
//!   `simulation_runner::apply_voxel_cure_for_layer` reads PI columns,
//!   calls `compute_column_exposure`, applies the 1D Z conv to the
//!   resulting dose column, then deposits via `add_dose` + `deplete`.
//!   That orchestration is NOT a service concern.
//! - Full 3D tensor convolution — rejected for v1 per ADR-0018 §4(a)
//!   (temporal structure: each layer is exposed at a different print
//!   step, so a "full-print intensity tensor" doesn't naturally exist).
//!
//! # Edge handling
//!
//! Both `apply_separable_2d` and `apply_separable_1d_z` use **clamp-to-zero**
//! at the buffer boundaries: out-of-bounds samples contribute zero to the
//! convolution, and the in-bounds output is NOT renormalised to compensate.
//! This is the **SKIP** policy described in ADR-0018 §2: energy past the
//! field boundary is physically lost (XY: absorbed by the build-envelope
//! wall; Z: absorbed by the vat floor / above-resin headspace). The
//! alternative (clamp-onto-boundary, i.e. fold the missing weight back onto
//! the boundary cell) would represent reflective boundaries — wrong physics
//! for an mSLA build envelope.
//!
//! # NaN policy
//!
//! Two-layer defence per docs/patterns/nan-two-layer-defence.md: explicit
//! `is_finite()` checks before any arithmetic. `CrosstalkError::NonFiniteInput`
//! returned on any non-finite input.
//!
//! # Stateless
//!
//! All inputs are explicit; the calculator owns no state. Buffers are
//! caller-owned (caller pays for allocation + can reuse across layers).

#![cfg(feature = "field-sim")]

use ndarray::Array2;
use thiserror::Error;

/// Errors from light-crosstalk kernel construction + convolution.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CrosstalkError {
    /// Sigma input was not finite or was negative.
    #[error("CrosstalkCalculator: sigma_voxels must be finite and >= 0 (got {sigma_voxels})")]
    InvalidSigma { sigma_voxels: f32 },
    /// Intensity / column buffer contained a non-finite value.
    #[error("CrosstalkCalculator: input buffer contains a non-finite value")]
    NonFiniteInput,
    /// Scratch buffer dimensions did not match the input.
    #[error(
        "CrosstalkCalculator: scratch buffer dims {scratch:?} do not match input dims {input:?}"
    )]
    DimensionMismatch {
        input: (usize, usize),
        scratch: (usize, usize),
    },
}

/// Domain service for light-crosstalk kernel primitives.
pub struct LightCrosstalkCalculator;

impl LightCrosstalkCalculator {
    /// Build a symmetric 1D normalised Gaussian kernel for the given σ in
    /// voxel units. Radius is `⌈3σ⌉`; total length `2·radius + 1`.
    ///
    /// - `σ <= 0.0` returns the identity kernel `vec![1.0]` (length 1,
    ///   radius 0). Caller code that applies this kernel via
    ///   `apply_separable_2d` or `apply_separable_1d_z` sees no-op convolution.
    /// - For `σ > 0`, weights are `w_k = exp(-k² / (2 σ²))` for
    ///   `k in -radius..=radius`, then normalised so that `sum(w) == 1`.
    ///
    /// **Anisotropy note:** for σ_xy the input is `σ_xy_um / (voxel_size_mm × 1000)`;
    /// for σ_z the input is `σ_z_um / layer_height_um`. The same routine
    /// produces the kernel for both axes — anisotropy lives in the
    /// caller's choice of σ_voxels, not in this method (ADR-0018 §1).
    pub fn build_separable_kernel(sigma_voxels: f32) -> Result<Vec<f32>, CrosstalkError> {
        if !sigma_voxels.is_finite() {
            return Err(CrosstalkError::InvalidSigma { sigma_voxels });
        }
        if sigma_voxels < 0.0 {
            return Err(CrosstalkError::InvalidSigma { sigma_voxels });
        }
        if sigma_voxels == 0.0 {
            return Ok(vec![1.0]);
        }
        let radius = (3.0 * sigma_voxels).ceil() as i32;
        let radius = radius.max(1);
        let len = (2 * radius + 1) as usize;
        let two_sigma_sq = 2.0 * sigma_voxels * sigma_voxels;
        let mut weights: Vec<f32> = (0..len)
            .map(|i| {
                let k = i as i32 - radius;
                (-(k * k) as f32 / two_sigma_sq).exp()
            })
            .collect();
        let sum: f32 = weights.iter().sum();
        // sum is always > 0 for finite sigma_voxels > 0, but guard against
        // pathological underflow on huge σ at the kernel-radius tails.
        if !sum.is_finite() || sum <= 0.0 {
            return Err(CrosstalkError::InvalidSigma { sigma_voxels });
        }
        for w in weights.iter_mut() {
            *w /= sum;
        }
        Ok(weights)
    }

    /// In-place separable 2D Gaussian convolution on `intensity` using a
    /// single caller-owned `scratch` buffer of identical shape.
    ///
    /// X-pass: read `intensity → scratch`. Y-pass: read `scratch → intensity`.
    /// Both passes use clamp-to-zero edges (SKIP policy per ADR-0018 §2).
    ///
    /// For identity kernel `[1.0]` (length 1, σ ≤ 0), this is a no-op
    /// (X-pass copies intensity → scratch; Y-pass copies scratch → intensity).
    pub fn apply_separable_2d(
        intensity: &mut Array2<f32>,
        kernel: &[f32],
        scratch: &mut Array2<f32>,
    ) -> Result<(), CrosstalkError> {
        if intensity.dim() != scratch.dim() {
            return Err(CrosstalkError::DimensionMismatch {
                input: intensity.dim(),
                scratch: scratch.dim(),
            });
        }
        if !intensity.iter().all(|v| v.is_finite()) {
            return Err(CrosstalkError::NonFiniteInput);
        }
        if !kernel.iter().all(|w| w.is_finite()) {
            return Err(CrosstalkError::NonFiniteInput);
        }
        let (nx, ny) = intensity.dim();
        let len = kernel.len();
        if len == 0 {
            return Err(CrosstalkError::InvalidSigma { sigma_voxels: 0.0 });
        }
        let radius = (len as i32 - 1) / 2;

        // X-pass: convolve along axis 0 (x), reading intensity, writing scratch.
        for iy in 0..ny {
            for ix in 0..nx {
                let mut acc = 0.0_f32;
                for k in 0..len {
                    let src_ix = ix as i32 + (k as i32 - radius);
                    if src_ix < 0 || src_ix >= nx as i32 {
                        continue; // SKIP — clamp-to-zero edge
                    }
                    acc += kernel[k] * intensity[(src_ix as usize, iy)];
                }
                scratch[(ix, iy)] = acc;
            }
        }

        // Y-pass: convolve along axis 1 (y), reading scratch, writing intensity.
        for ix in 0..nx {
            for iy in 0..ny {
                let mut acc = 0.0_f32;
                for k in 0..len {
                    let src_iy = iy as i32 + (k as i32 - radius);
                    if src_iy < 0 || src_iy >= ny as i32 {
                        continue; // SKIP — clamp-to-zero edge
                    }
                    acc += kernel[k] * scratch[(ix, src_iy as usize)];
                }
                intensity[(ix, iy)] = acc;
            }
        }
        Ok(())
    }

    /// In-place 1D Gaussian convolution along Z on a per-column buffer
    /// using a single caller-owned `scratch` buffer of identical length.
    ///
    /// Writes scratch ← convolved(column), then copies scratch → column.
    /// Clamp-to-zero edges (SKIP policy per ADR-0018 §2): out-of-bounds
    /// samples contribute zero; in-bounds output is NOT renormalised.
    ///
    /// For identity kernel `[1.0]` (length 1, σ ≤ 0), this is a no-op.
    pub fn apply_separable_1d_z(
        column: &mut [f32],
        kernel: &[f32],
        scratch: &mut [f32],
    ) -> Result<(), CrosstalkError> {
        if column.len() != scratch.len() {
            return Err(CrosstalkError::DimensionMismatch {
                input: (column.len(), 1),
                scratch: (scratch.len(), 1),
            });
        }
        if !column.iter().all(|v| v.is_finite()) {
            return Err(CrosstalkError::NonFiniteInput);
        }
        if !kernel.iter().all(|w| w.is_finite()) {
            return Err(CrosstalkError::NonFiniteInput);
        }
        let len = kernel.len();
        if len == 0 {
            return Err(CrosstalkError::InvalidSigma { sigma_voxels: 0.0 });
        }
        let radius = (len as i32 - 1) / 2;
        let nz = column.len() as i32;
        for iz in 0..column.len() {
            let mut acc = 0.0_f32;
            for k in 0..len {
                let src = iz as i32 + (k as i32 - radius);
                if src < 0 || src >= nz {
                    continue; // SKIP — clamp-to-zero edge
                }
                acc += kernel[k] * column[src as usize];
            }
            scratch[iz] = acc;
        }
        column.copy_from_slice(scratch);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use ndarray::Array2;

    // ---- build_separable_kernel ----

    #[test]
    fn sigma_zero_returns_identity_kernel() {
        let k = LightCrosstalkCalculator::build_separable_kernel(0.0)
            .expect("σ = 0 must succeed and return identity");
        assert_eq!(k, vec![1.0]);
    }

    #[test]
    fn negative_sigma_rejected() {
        assert!(matches!(
            LightCrosstalkCalculator::build_separable_kernel(-0.5),
            Err(CrosstalkError::InvalidSigma { .. })
        ));
    }

    #[test]
    fn nan_sigma_rejected() {
        assert!(matches!(
            LightCrosstalkCalculator::build_separable_kernel(f32::NAN),
            Err(CrosstalkError::InvalidSigma { .. })
        ));
    }

    #[test]
    fn kernel_sigma_one_has_expected_radius_and_normalisation() {
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0)
            .expect("σ = 1 must succeed");
        // radius = ⌈3·1⌉ = 3; length 7.
        assert_eq!(k.len(), 7);
        let sum: f32 = k.iter().sum();
        assert_relative_eq!(sum, 1.0, epsilon = 1e-5);
    }

    #[test]
    fn kernel_sigma_point_eight_yields_seven_taps() {
        // σ_z_um = 40, layer_height = 50 ⇒ σ_layers = 0.8 ⇒ radius ⌈2.4⌉ = 3 ⇒ length 7.
        let k = LightCrosstalkCalculator::build_separable_kernel(0.8)
            .expect("σ = 0.8 must succeed");
        assert_eq!(k.len(), 7);
        // Symmetry about centre (index 3).
        for i in 0..3 {
            assert_relative_eq!(k[i], k[6 - i], epsilon = 1e-6);
        }
        // Max at centre, monotone-decreasing outward.
        assert!(k[3] > k[2]);
        assert!(k[2] > k[1]);
        assert!(k[1] > k[0]);
        // Sum = 1.
        let sum: f32 = k.iter().sum();
        assert_relative_eq!(sum, 1.0, epsilon = 1e-5);
    }

    proptest::proptest! {
        // (vii) Kernel normalisation property: for σ ∈ (0, 5], kernel sums to 1.0.
        #[test]
        fn kernel_normalisation_property(sigma_voxels in 0.01_f32..5.0) {
            let k = LightCrosstalkCalculator::build_separable_kernel(sigma_voxels)
                .expect("σ in (0, 5] must succeed");
            let sum: f32 = k.iter().sum();
            proptest::prop_assert!((sum - 1.0).abs() < 1e-5,
                "kernel sum {sum} not within 1e-5 of 1.0 for σ = {sigma_voxels}");
        }
    }

    // ---- apply_separable_2d ----

    #[test]
    fn xy_sigma_zero_identity_is_noop() {
        let mut intensity = Array2::from_shape_vec((3, 3), vec![1., 2., 3., 4., 5., 6., 7., 8., 9.])
            .expect("3x3 init");
        let mut scratch = Array2::<f32>::zeros((3, 3));
        let before = intensity.clone();
        let k = LightCrosstalkCalculator::build_separable_kernel(0.0)
            .expect("σ = 0 must succeed");
        LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
            .expect("identity kernel must succeed");
        for ((ix, iy), v) in intensity.indexed_iter() {
            assert_eq!(*v, before[(ix, iy)],
                "identity kernel must be no-op at ({ix},{iy})");
        }
    }

    #[test]
    fn xy_impulse_sigma_one_matches_analytical_separable_gaussian() {
        // 7x7 grid, impulse at centre (3, 3), σ = 1, kernel radius 3.
        let mut intensity = Array2::<f32>::zeros((7, 7));
        intensity[(3, 3)] = 1.0;
        let mut scratch = Array2::<f32>::zeros((7, 7));
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0)
            .expect("σ = 1 must succeed");
        LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
            .expect("σ = 1 must succeed");
        // Analytical separable Gaussian: I(ix, iy) = k[ix - 3 + 3] * k[iy - 3 + 3]
        for ix in 0..7 {
            for iy in 0..7 {
                let expected = k[ix] * k[iy];
                assert_relative_eq!(intensity[(ix, iy)], expected, epsilon = 1e-6);
            }
        }
    }

    #[test]
    fn xy_rotation_symmetry_90deg() {
        // Property: if Gaussian σ_x == σ_y, then 2D conv commutes with 90°
        // rotation: rotated(conv(a)) == conv(rotated(a)).
        //
        // 5×5 grid. Forward 90° CW rotation: (ix, iy) → (iy, n-1-ix).
        // Inverse 90° CW (used to LOOK UP into the un-rotated source from a
        // rotated-target position): (ix, iy) → (n-1-iy, ix).
        //
        // Place impulse in a at (1, 2). Place impulse in b at the rotated
        // position: forward map of (1, 2) is (2, 4-1) = (2, 3).
        let mut a = Array2::<f32>::zeros((5, 5));
        a[(1, 2)] = 1.0;
        let mut b = Array2::<f32>::zeros((5, 5));
        b[(2, 3)] = 1.0;
        let mut sa = Array2::<f32>::zeros((5, 5));
        let mut sb = Array2::<f32>::zeros((5, 5));
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        LightCrosstalkCalculator::apply_separable_2d(&mut a, &k, &mut sa).expect("a");
        LightCrosstalkCalculator::apply_separable_2d(&mut b, &k, &mut sb).expect("b");
        // b ≡ rotated(a) ⇒ at each position (ix, iy) of b, look up the
        // pre-rotation source in a at the inverse-rotated position
        // (n-1-iy, ix). For n = 5, that's (4-iy, ix).
        for ix in 0..5 {
            for iy in 0..5 {
                let a_pre_rotated_at_this_position = a[(4 - iy, ix)];
                assert_relative_eq!(
                    b[(ix, iy)],
                    a_pre_rotated_at_this_position,
                    epsilon = 1e-6
                );
            }
        }
    }

    #[test]
    fn xy_dimension_mismatch_rejected() {
        let mut intensity = Array2::<f32>::zeros((3, 3));
        let mut scratch = Array2::<f32>::zeros((3, 4)); // mismatched
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        let err = LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
            .expect_err("dim mismatch must Err");
        assert!(matches!(err, CrosstalkError::DimensionMismatch { .. }));
    }

    #[test]
    fn xy_nan_input_rejected() {
        let mut intensity = Array2::<f32>::zeros((3, 3));
        intensity[(0, 0)] = f32::NAN;
        let mut scratch = Array2::<f32>::zeros((3, 3));
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        let err = LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
            .expect_err("NaN input must Err");
        assert!(matches!(err, CrosstalkError::NonFiniteInput));
    }

    proptest::proptest! {
        // (iv-a) General positive grids: sum_after ≤ sum_before × 1.0001 (upper bound, lossy).
        #![proptest_config(proptest::test_runner::Config {
            cases: 50,
            .. proptest::test_runner::Config::default()
        })]

        #[test]
        fn xy_energy_upper_bound(
            sigma_voxels in 0.1_f32..3.0,
            nx in 4usize..10,
            ny in 4usize..10,
        ) {
            let total: usize = nx * ny;
            // Random positive grid via deterministic hash of dims.
            let mut intensity = Array2::<f32>::from_shape_fn((nx, ny), |(ix, iy)| {
                ((ix * 13 + iy * 7) % 11) as f32 + 0.5
            });
            let sum_before: f32 = intensity.iter().sum();
            let _ = total; // silence unused; kept for clarity.
            let mut scratch = Array2::<f32>::zeros((nx, ny));
            let k = LightCrosstalkCalculator::build_separable_kernel(sigma_voxels)
                .expect("σ in (0, 3] must succeed");
            LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
                .expect("conv must succeed");
            let sum_after: f32 = intensity.iter().sum();
            proptest::prop_assert!(sum_after <= sum_before * 1.0001 + 1e-5,
                "sum_after {sum_after} > sum_before {sum_before} × 1.0001");
        }

        // (iv-b) Zero-padded margin: interior conservation exact within 1e-5.
        #[test]
        fn xy_energy_interior_exact_padded(
            sigma_voxels in 0.1_f32..2.0,
        ) {
            // Build 11x11 grid; only inner 5x5 (indices 3..=7) populated.
            // Kernel radius for σ ≤ 2 is ⌈6⌉ = 6, so 3 cells from edge
            // is the minimum padding margin to be safe. Adjust to 11×11
            // grid with central 5x5 populated for kernel ≤ 6 the inner
            // 5x5 stays away from edges.
            let mut intensity = Array2::<f32>::zeros((11, 11));
            for ix in 3..=7 {
                for iy in 3..=7 {
                    intensity[(ix, iy)] = ((ix + iy) % 5) as f32 + 1.0;
                }
            }
            let sum_before: f32 = intensity.iter().sum();
            let mut scratch = Array2::<f32>::zeros((11, 11));
            let k = LightCrosstalkCalculator::build_separable_kernel(sigma_voxels)
                .expect("σ in (0, 2] must succeed");
            // Kernel radius for σ ≤ 2 is ⌈3·2⌉ = 6 ⇒ need ≥ 6 padding.
            // We have 3 cells, so for σ > 1 the kernel may reach edges;
            // skip those cases via prop_assume.
            let radius = (k.len() as i32 - 1) / 2;
            proptest::prop_assume!(radius <= 3);
            LightCrosstalkCalculator::apply_separable_2d(&mut intensity, &k, &mut scratch)
                .expect("conv must succeed");
            let sum_after: f32 = intensity.iter().sum();
            proptest::prop_assert!(
                (sum_after - sum_before).abs() < 1e-5 * sum_before,
                "interior conservation violated: before = {sum_before}, after = {sum_after}"
            );
        }
    }

    // ---- apply_separable_1d_z ----

    #[test]
    fn z_sigma_zero_identity_is_noop() {
        let mut col = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0];
        let before = col.clone();
        let mut scratch = vec![0.0_f32; 5];
        let k = LightCrosstalkCalculator::build_separable_kernel(0.0).expect("σ = 0");
        LightCrosstalkCalculator::apply_separable_1d_z(&mut col, &k, &mut scratch)
            .expect("identity kernel succeeds");
        assert_eq!(col, before);
    }

    #[test]
    fn z_impulse_matches_kernel_shape() {
        // Place impulse at index 5 of length-11 column; σ = 1 → length 7 kernel.
        // Output at index iz = k[iz - 5 + 3] for iz in [2..=8], else 0.
        let mut col = vec![0.0_f32; 11];
        col[5] = 1.0;
        let mut scratch = vec![0.0_f32; 11];
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        LightCrosstalkCalculator::apply_separable_1d_z(&mut col, &k, &mut scratch)
            .expect("σ = 1 succeeds");
        for iz in 0..11 {
            let off = (iz as i32) - 5;
            let kernel_idx = off + 3; // kernel centre at index 3
            let expected = if (0..k.len() as i32).contains(&kernel_idx) {
                k[kernel_idx as usize]
            } else {
                0.0
            };
            assert_relative_eq!(col[iz], expected, epsilon = 1e-6);
        }
    }

    #[test]
    fn z_edge_skip_at_index_zero() {
        // Impulse at index 0 with σ = 1 (kernel radius 3): output at index 0
        // sees ONLY k[3] (centre) — kernel[0], k[1], k[2] sample positions
        // -3, -2, -1 are out-of-bounds and SKIPPED (clamp-to-zero), NOT
        // folded back onto index 0. This is the load-bearing SKIP semantics
        // test from the v5 plan.
        let mut col = vec![0.0_f32; 7];
        col[0] = 1.0;
        let mut scratch = vec![0.0_f32; 7];
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        LightCrosstalkCalculator::apply_separable_1d_z(&mut col, &k, &mut scratch)
            .expect("σ = 1 succeeds");
        // Output at iz=0: only kernel weight at offset 0 (= kernel[3]).
        let expected_zero = k[3] * 1.0; // SKIP — no contribution from out-of-bounds
        let bad_clamp_value = k.iter().take(4).sum::<f32>(); // what CLAMP would produce
        assert_relative_eq!(col[0], expected_zero, epsilon = 1e-6);
        // Sanity-check that the test would actually catch a clamp bug:
        assert!(
            (col[0] - bad_clamp_value).abs() > 1e-3,
            "expected SKIP value ({expected_zero}) and CLAMP value ({bad_clamp_value}) must \
             differ enough that the test discriminates"
        );
    }

    #[test]
    fn z_nan_input_rejected() {
        let mut col = vec![1.0_f32, f32::NAN, 3.0];
        let mut scratch = vec![0.0_f32; 3];
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        let err = LightCrosstalkCalculator::apply_separable_1d_z(&mut col, &k, &mut scratch)
            .expect_err("NaN input must Err");
        assert!(matches!(err, CrosstalkError::NonFiniteInput));
    }

    #[test]
    fn z_dimension_mismatch_rejected() {
        let mut col = vec![0.0_f32; 5];
        let mut scratch = vec![0.0_f32; 6]; // mismatched
        let k = LightCrosstalkCalculator::build_separable_kernel(1.0).expect("σ = 1");
        let err = LightCrosstalkCalculator::apply_separable_1d_z(&mut col, &k, &mut scratch)
            .expect_err("dim mismatch must Err");
        assert!(matches!(err, CrosstalkError::DimensionMismatch { .. }));
    }
}
