//! ADR-0025 Stage E: combined cure+strain GPU pipeline parity tests.
//!
//! Verifies the batched single-encoder path (encode_cure_pass +
//! encode_copy_dose_from + encode_strain_stress_pass -> one submit)
//! produces results within tolerance of the existing separate-dispatch
//! CPU reference. Also exercises double-buffer intensity swap and
//! boundary conditions (1-layer, 8+ layers).

#![cfg(feature = "gpu")]

use resinsim_core::services::{
    GpuContext, GpuCureBuffers, GpuStrainStressBuffers, ShrinkageCalculator,
    VoxelCureCalculator,
};
use resinsim_core::values::{CureField, PenetrationDepth, PhotoinitiatorField, StrainTensor, StressTensor};
use resinsim_core::wgpu;

fn try_gpu() -> Option<GpuContext> {
    GpuContext::try_new()
}

const DOSE_TOLERANCE: f32 = 1e-3;
const PI_TOLERANCE: f32 = 1e-5;
const STRAIN_TOLERANCE: f32 = 1e-3;
const STRESS_TOLERANCE: f32 = 1e-1;

#[test]
fn combined_cure_strain_single_encoder_parity() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 8_u32;
    let ny = 8;
    let nz = 4;
    let voxel_mm = 0.05;
    let layer_height_um = 50.0;
    let dp_um = 100.0;
    let k_d = 0.05;
    let led_power = 10.0;
    let exposure_sec = 2.5;
    let iz_top = 0_u32;
    let ec_ref = 5.0_f32;
    let linear_shrinkage_frac = 0.015_f32;
    let z_anisotropy_ratio = 1.5_f32;
    let youngs_modulus_mpa = 2000.0_f32;
    let poissons_ratio = 0.35_f32;

    // --- CPU reference ---
    let mut cure_cpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
    let dp = PenetrationDepth::new(dp_um).unwrap();

    let mut intensity_grid = vec![0.0_f32; (nx * ny) as usize];
    for iy in 0..ny {
        for ix in 0..nx {
            let intensity = led_power * (1.0 + 0.1 * ix as f32);
            intensity_grid[(iy * nx + ix) as usize] = intensity;
            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu, &mut pi_cpu, ix, iy, iz_top, intensity,
                exposure_sec, dp, k_d, layer_height_um,
            ).unwrap();
        }
    }

    let mut cpu_strain = vec![StrainTensor::zero(); (nx * ny) as usize];
    let mut cpu_stress = vec![[0.0_f32; 6]; (nx * ny) as usize];
    for ix in 0..nx {
        for iy in 0..ny {
            let dose = cure_cpu.dose_at(ix, iy, iz_top).unwrap();
            let extent = ShrinkageCalculator::cure_extent_at_voxel(
                dose, ec_ref, dp_um, layer_height_um,
            );
            let strain = ShrinkageCalculator::free_shrinkage_strain_at_voxel(
                extent, linear_shrinkage_frac, z_anisotropy_ratio,
            );
            let idx = (ix * ny + iy) as usize;
            cpu_strain[idx] = strain;
            if strain != StrainTensor::zero() {
                let sigma = StressTensor::from_strain_linear_elastic(
                    &strain, youngs_modulus_mpa, poissons_ratio,
                ).unwrap();
                cpu_stress[idx] = sigma.components();
            }
        }
    }

    // --- GPU combined pipeline: single encoder ---
    let mut cure_gpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();

    let cure_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu)
        .expect("gpu cure buffers");
    let strain_bufs = GpuStrainStressBuffers::new(&ctx, &cure_gpu, nz);

    cure_bufs.upload_slab(&ctx, &cure_gpu, &pi_gpu, 0, nz);
    cure_bufs.write_intensity(&ctx, &intensity_grid);

    let mut encoder = ctx.device().create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("combined_test") },
    );
    cure_bufs.encode_cure_pass(
        &ctx, &mut encoder, iz_top, nz, exposure_sec, dp_um, k_d, layer_height_um,
        0, nz,
    );
    strain_bufs.encode_copy_dose_from(&mut encoder, cure_bufs.cure_buf());
    strain_bufs.encode_strain_stress_pass(
        &ctx, &mut encoder, iz_top, ec_ref, dp_um, layer_height_um,
        linear_shrinkage_frac, z_anisotropy_ratio, youngs_modulus_mpa,
        poissons_ratio, nz, 0,
    );
    ctx.queue().submit(Some(encoder.finish()));

    cure_bufs.download_slab(&ctx, &mut cure_gpu, &mut pi_gpu, 0, nz);
    let gpu_strain = strain_bufs.download_strain(&ctx);
    let gpu_stress = strain_bufs.download_stress(&ctx);

    let mut max_dose_diff: f32 = 0.0;
    let mut max_pi_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let diff = (cure_cpu.dose_at(ix, iy, iz).unwrap()
                    - cure_gpu.dose_at(ix, iy, iz).unwrap()).abs();
                max_dose_diff = max_dose_diff.max(diff);
                let pdiff = (pi_cpu.concentration_at(ix, iy, iz).unwrap()
                    - pi_gpu.concentration_at(ix, iy, iz).unwrap()).abs();
                max_pi_diff = max_pi_diff.max(pdiff);
            }
        }
    }

    let mut max_strain_diff: f32 = 0.0;
    let mut max_stress_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            let idx = (ix * ny + iy) as usize;
            let cpu_s = cpu_strain[idx].components();
            let gpu_s = gpu_strain[idx].components();
            for c in 0..6 {
                max_strain_diff = max_strain_diff.max((cpu_s[c] - gpu_s[c]).abs());
            }
            let cpu_sig = cpu_stress[idx];
            let gpu_sig = gpu_stress[idx];
            for c in 0..6 {
                max_stress_diff = max_stress_diff.max((cpu_sig[c] - gpu_sig[c]).abs());
            }
        }
    }

    eprintln!(
        "combined parity: dose={max_dose_diff:.6e} pi={max_pi_diff:.6e} \
         strain={max_strain_diff:.6e} stress={max_stress_diff:.6e}"
    );
    assert!(max_dose_diff < DOSE_TOLERANCE, "dose: {max_dose_diff}");
    assert!(max_pi_diff < PI_TOLERANCE, "pi: {max_pi_diff}");
    assert!(max_strain_diff < STRAIN_TOLERANCE, "strain: {max_strain_diff}");
    assert!(max_stress_diff < STRESS_TOLERANCE, "stress: {max_stress_diff}");
}

#[test]
fn combined_pipeline_multi_layer_8_layers() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4;
    let nz = 8;
    let voxel_mm = 0.05;
    let layer_height_um = 50.0;
    let dp_um = 100.0;
    let k_d = 0.05;
    let led_power = 10.0;
    let exposure_sec = 2.0;
    let ec_ref = 5.0_f32;
    let linear_shrinkage_frac = 0.015_f32;
    let z_anisotropy_ratio = 1.5_f32;
    let youngs_modulus_mpa = 2000.0_f32;
    let poissons_ratio = 0.35_f32;

    let mut cure_cpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
    let dp = PenetrationDepth::new(dp_um).unwrap();

    let mut cure_gpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();

    let cure_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu)
        .expect("gpu cure buffers");
    let strain_bufs = GpuStrainStressBuffers::new(&ctx, &cure_gpu, nz);

    cure_bufs.upload_slab(&ctx, &cure_gpu, &pi_gpu, 0, nz);

    for layer in 0..nz {
        let layer_intensity = led_power * (1.0 + 0.05 * layer as f32);
        let mut intensity_grid = vec![0.0_f32; (nx * ny) as usize];
        for iy in 0..ny {
            for ix in 0..nx {
                intensity_grid[(iy * nx + ix) as usize] = layer_intensity;
                VoxelCureCalculator::apply_column_exposure(
                    &mut cure_cpu, &mut pi_cpu, ix, iy, layer, layer_intensity,
                    exposure_sec, dp, k_d, layer_height_um,
                ).unwrap();
            }
        }

        cure_bufs.write_intensity(&ctx, &intensity_grid);
        let mut encoder = ctx.device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("multi_layer") },
        );
        cure_bufs.encode_cure_pass(
            &ctx, &mut encoder, layer, nz, exposure_sec, dp_um, k_d, layer_height_um,
            0, nz,
        );
        strain_bufs.encode_copy_dose_from(&mut encoder, cure_bufs.cure_buf());
        strain_bufs.encode_strain_stress_pass(
            &ctx, &mut encoder, layer, ec_ref, dp_um, layer_height_um,
            linear_shrinkage_frac, z_anisotropy_ratio, youngs_modulus_mpa,
            poissons_ratio, nz, 0,
        );
        ctx.queue().submit(Some(encoder.finish()));
    }

    cure_bufs.download_slab(&ctx, &mut cure_gpu, &mut pi_gpu, 0, nz);

    let mut max_dose_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let diff = (cure_cpu.dose_at(ix, iy, iz).unwrap()
                    - cure_gpu.dose_at(ix, iy, iz).unwrap()).abs();
                max_dose_diff = max_dose_diff.max(diff);
            }
        }
    }

    eprintln!("multi-layer 8: max_dose_diff={max_dose_diff:.6e}");
    assert!(max_dose_diff < DOSE_TOLERANCE, "8-layer dose: {max_dose_diff}");
    assert!(cure_gpu.max_dose() > 0.0, "must produce non-zero dose");
}

#[test]
fn combined_pipeline_single_layer_boundary() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4;
    let nz = 1;
    let voxel_mm = 0.05;
    let layer_height_um = 50.0;
    let dp_um = 100.0;
    let k_d = 0.05;
    let led_power = 10.0;
    let exposure_sec = 2.5;

    let mut cure_cpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
    let dp = PenetrationDepth::new(dp_um).unwrap();

    for iy in 0..ny {
        for ix in 0..nx {
            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu, &mut pi_cpu, ix, iy, 0, led_power,
                exposure_sec, dp, k_d, layer_height_um,
            ).unwrap();
        }
    }

    let mut cure_gpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();

    let cure_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu)
        .expect("gpu cure buffers");

    cure_bufs.upload_slab(&ctx, &cure_gpu, &pi_gpu, 0, nz);
    let intensity_grid = vec![led_power; (nx * ny) as usize];
    cure_bufs.write_intensity(&ctx, &intensity_grid);
    let mut encoder = ctx.device().create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("single_layer") },
    );
    cure_bufs.encode_cure_pass(
        &ctx, &mut encoder, 0, nz, exposure_sec, dp_um, k_d, layer_height_um,
        0, nz,
    );
    ctx.queue().submit(Some(encoder.finish()));
    cure_bufs.download_slab(&ctx, &mut cure_gpu, &mut pi_gpu, 0, nz);

    let mut max_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            let diff = (cure_cpu.dose_at(ix, iy, 0).unwrap()
                - cure_gpu.dose_at(ix, iy, 0).unwrap()).abs();
            max_diff = max_diff.max(diff);
        }
    }

    eprintln!("1-layer boundary: max_diff={max_diff:.6e}");
    assert!(max_diff < DOSE_TOLERANCE, "1-layer: {max_diff}");
    assert!(cure_gpu.max_dose() > 0.0);
}

#[test]
fn double_buffer_swap_isolation() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4;
    let nz = 4;
    let voxel_mm = 0.05;
    let layer_height_um = 50.0;
    let dp_um = 100.0;
    let k_d = 0.05;
    let exposure_sec = 2.5;
    let dp = PenetrationDepth::new(dp_um).unwrap();

    let intensity_low = vec![1.0_f32; (nx * ny) as usize];
    let intensity_high = vec![50.0_f32; (nx * ny) as usize];

    let mut cure_cpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
    for iy in 0..ny {
        for ix in 0..nx {
            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu, &mut pi_cpu, ix, iy, 0, 1.0,
                exposure_sec, dp, k_d, layer_height_um,
            ).unwrap();
            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu, &mut pi_cpu, ix, iy, 1, 50.0,
                exposure_sec, dp, k_d, layer_height_um,
            ).unwrap();
        }
    }

    let mut cure_gpu = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
    let cure_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu)
        .expect("gpu cure buffers");

    cure_bufs.upload_slab(&ctx, &cure_gpu, &pi_gpu, 0, nz);

    cure_bufs.write_intensity(&ctx, &intensity_low);
    let mut enc0 = ctx.device().create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("layer0") },
    );
    cure_bufs.encode_cure_pass(
        &ctx, &mut enc0, 0, nz, exposure_sec, dp_um, k_d, layer_height_um,
        0, nz,
    );
    ctx.queue().submit(Some(enc0.finish()));

    cure_bufs.write_intensity(&ctx, &intensity_high);
    let mut enc1 = ctx.device().create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("layer1") },
    );
    cure_bufs.encode_cure_pass(
        &ctx, &mut enc1, 1, nz, exposure_sec, dp_um, k_d, layer_height_um,
        0, nz,
    );
    ctx.queue().submit(Some(enc1.finish()));

    cure_bufs.download_slab(&ctx, &mut cure_gpu, &mut pi_gpu, 0, nz);

    let dose_layer0 = cure_gpu.dose_at(0, 0, 0).unwrap();
    let dose_layer1 = cure_gpu.dose_at(0, 0, 1).unwrap();
    assert!(
        dose_layer1 > dose_layer0 * 5.0,
        "layer1 dose ({dose_layer1}) must be >> layer0 dose ({dose_layer0}); \
         if they're similar the double-buffer swap is stale"
    );

    let mut max_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..2 {
                let diff = (cure_cpu.dose_at(ix, iy, iz).unwrap()
                    - cure_gpu.dose_at(ix, iy, iz).unwrap()).abs();
                max_diff = max_diff.max(diff);
            }
        }
    }
    eprintln!("double-buffer swap: max_diff={max_diff:.6e}");
    assert!(max_diff < DOSE_TOLERANCE, "swap parity: {max_diff}");
}
