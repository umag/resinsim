//! ADR-0025 / t2f5 Stage D: GPU ↔ CPU strain + stress parity tests.
//!
//! Tolerance: max per-component strain < 1e-3, stress < 1e-1 MPa.
//! Tests skip gracefully when no GPU adapter is available.

#![cfg(feature = "gpu")]

use resinsim_core::services::{GpuContext, GpuStrainStressBuffers, ShrinkageCalculator};
use resinsim_core::values::{CureField, StressTensor, StrainTensor};

fn try_gpu_context() -> Option<GpuContext> {
    GpuContext::try_new()
}

const STRAIN_TOLERANCE: f32 = 1e-3;
const STRESS_TOLERANCE: f32 = 1e-1;

#[test]
fn gpu_cpu_strain_stress_parity_mixed_cure() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 8_u32;
    let ny = 8_u32;
    let nz = 4_u32;
    let voxel_mm = 0.5;

    let mut cure = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0])
        .expect("cure field");

    let ec_ref = 5.0_f32;
    let dp_um = 170.0_f32;
    let layer_height_um = 50.0_f32;
    let linear_shrinkage_frac = 0.015_f32;
    let z_anisotropy_ratio = 1.5_f32;
    let youngs_modulus_mpa = 2000.0_f32;
    let poissons_ratio = 0.35_f32;

    let layer_z = 1_u32;

    // Seed dose: some voxels above Ec (cured), some below (uncured)
    for ix in 0..nx {
        for iy in 0..ny {
            let dose = if (ix + iy) % 3 == 0 {
                2.0 // below Ec → uncured
            } else {
                20.0 + (ix as f32) * 5.0 // above Ec → varying cure
            };
            cure.add_dose(ix, iy, layer_z, dose)
                .expect("add_dose");
        }
    }

    // --- CPU path ---
    let mut cpu_strain = vec![StrainTensor::zero(); (nx * ny) as usize];
    let mut cpu_stress = vec![[0.0_f32; 6]; (nx * ny) as usize];

    for ix in 0..nx {
        for iy in 0..ny {
            let dose = cure.dose_at(ix, iy, layer_z).expect("dose_at");
            let cure_extent = ShrinkageCalculator::cure_extent_at_voxel(
                dose,
                ec_ref,
                dp_um,
                layer_height_um,
            );
            let strain = ShrinkageCalculator::free_shrinkage_strain_at_voxel(
                cure_extent,
                linear_shrinkage_frac,
                z_anisotropy_ratio,
            );
            let idx = (ix * ny + iy) as usize;
            cpu_strain[idx] = strain;

            if strain != StrainTensor::zero() {
                let sigma = StressTensor::from_strain_linear_elastic(
                    &strain,
                    youngs_modulus_mpa,
                    poissons_ratio,
                )
                .expect("stress");
                cpu_stress[idx] = sigma.components();
            }
        }
    }

    // --- GPU path ---
    let bufs = GpuStrainStressBuffers::new(&ctx, &cure);
    bufs.dispatch(
        &ctx,
        layer_z,
        ec_ref,
        dp_um,
        layer_height_um,
        linear_shrinkage_frac,
        z_anisotropy_ratio,
        youngs_modulus_mpa,
        poissons_ratio,
        nz,
    );
    let gpu_strain = bufs.download_strain(&ctx);
    let gpu_stress = bufs.download_stress(&ctx);

    // --- Compare ---
    let mut max_strain_diff: f32 = 0.0;
    let mut max_stress_diff: f32 = 0.0;
    let mut cured_count = 0_u32;
    let mut uncured_count = 0_u32;

    for ix in 0..nx {
        for iy in 0..ny {
            let idx = (ix * ny + iy) as usize;
            let cpu_s = cpu_strain[idx].components();
            let gpu_s = gpu_strain[idx].components();

            for c in 0..6 {
                let diff = (cpu_s[c] - gpu_s[c]).abs();
                if diff > max_strain_diff {
                    max_strain_diff = diff;
                }
            }

            let cpu_sig = cpu_stress[idx];
            let gpu_sig = gpu_stress[idx];
            for c in 0..6 {
                let diff = (cpu_sig[c] - gpu_sig[c]).abs();
                if diff > max_stress_diff {
                    max_stress_diff = diff;
                }
            }

            if cpu_strain[idx] == StrainTensor::zero() {
                uncured_count += 1;
            } else {
                cured_count += 1;
            }
        }
    }

    eprintln!(
        "max strain diff: {max_strain_diff:.6e} (tol: {STRAIN_TOLERANCE:.1e}), \
         max stress diff: {max_stress_diff:.6e} (tol: {STRESS_TOLERANCE:.1e}), \
         cured: {cured_count}, uncured: {uncured_count}"
    );

    assert!(
        max_strain_diff < STRAIN_TOLERANCE,
        "GPU/CPU strain parity violation: max diff {max_strain_diff} >= {STRAIN_TOLERANCE}"
    );
    assert!(
        max_stress_diff < STRESS_TOLERANCE,
        "GPU/CPU stress parity violation: max diff {max_stress_diff} >= {STRESS_TOLERANCE}"
    );
    assert!(cured_count > 0, "test must have at least one cured voxel");
    assert!(uncured_count > 0, "test must have at least one uncured voxel");
}

#[test]
fn gpu_uncured_voxels_produce_zero() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4_u32;
    let nz = 2_u32;
    let voxel_mm = 0.5;

    let cure = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0])
        .expect("cure field");
    // No dose added → all voxels have dose=0 → all uncured

    let bufs = GpuStrainStressBuffers::new(&ctx, &cure);
    bufs.dispatch(
        &ctx,
        0,       // layer_z
        5.0,     // ec_at_temp
        170.0,   // dp_um
        50.0,    // layer_height_um
        0.015,   // linear_shrinkage_frac
        1.5,     // z_anisotropy_ratio
        2000.0,  // youngs_modulus_mpa
        0.35,    // poissons_ratio
        nz,
    );
    let gpu_strain = bufs.download_strain(&ctx);
    let gpu_stress = bufs.download_stress(&ctx);

    for (i, tensor) in gpu_strain.iter().enumerate() {
        assert_eq!(
            *tensor,
            StrainTensor::zero(),
            "uncured voxel {i} must produce zero strain"
        );
    }
    for (i, comps) in gpu_stress.iter().enumerate() {
        for c in 0..6 {
            assert_eq!(
                comps[c], 0.0,
                "uncured voxel {i} component {c} must produce zero stress"
            );
        }
    }
}
