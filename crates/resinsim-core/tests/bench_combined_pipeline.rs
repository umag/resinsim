//! Benchmark: combined cure+strain encoder vs separate dispatches.
//! Run: cargo test --features gpu,field-sim -p resinsim-core --test bench_combined_pipeline --release -- --nocapture

#![cfg(feature = "gpu")]

use resinsim_core::services::{
    GpuContext, GpuCureBuffers, GpuStrainStressBuffers, VoxelCureCalculator,
};
use resinsim_core::values::{CureField, PenetrationDepth, PhotoinitiatorField};
use resinsim_core::wgpu;

fn try_gpu() -> Option<GpuContext> {
    GpuContext::try_new()
}

#[test]
fn bench_combined_vs_separate() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("no GPU — skipping benchmark");
            return;
        }
    };
    eprintln!("\nGPU adapter: {}", ctx.adapter_name());

    let nx = 64_u32;
    let ny = 64;
    let nz = 60;
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
    let n_layers = nz;

    let intensity_grid: Vec<f32> = (0..(nx * ny))
        .map(|i| led_power * (1.0 + 0.01 * (i % nx) as f32))
        .collect();

    // --- Separate dispatches (old path): cure.dispatch() + upload_dose + strain.dispatch() ---
    {
        let mut cure = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
        let mut pi = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
        let cure_bufs = GpuCureBuffers::new(&ctx, &cure, &pi)
            .expect("gpu cure buffers");
        let strain_bufs = GpuStrainStressBuffers::new(&ctx, &cure, nz);

        let start = std::time::Instant::now();
        for layer in 0..n_layers {
            cure_bufs.dispatch(
                &ctx, &intensity_grid, layer, nz, exposure_sec, dp_um, k_d,
                layer_height_um, &mut cure, &mut pi,
            );
            strain_bufs.upload_dose(&ctx, &cure, 0);
            strain_bufs.dispatch(
                &ctx, layer, ec_ref, dp_um, layer_height_um,
                linear_shrinkage_frac, z_anisotropy_ratio, youngs_modulus_mpa,
                poissons_ratio, nz, 0,
            );
        }
        let separate_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "\nSeparate dispatches ({n_layers} layers, {nx}x{ny}x{nz}):\n  \
             {separate_ms:.1} ms  ({:.2} ms/layer)",
            separate_ms / n_layers as f64,
        );
    }

    // --- Combined encoder (new path): write_intensity + encode_cure + copy_dose + encode_strain ---
    {
        let cure = CureField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0]).unwrap();
        let pi = PhotoinitiatorField::new(nx, ny, nz, 1.0).unwrap();
        let cure_bufs = GpuCureBuffers::new(&ctx, &cure, &pi)
            .expect("gpu cure buffers");
        let strain_bufs = GpuStrainStressBuffers::new(&ctx, &cure, nz);

        cure_bufs.upload_slab(&ctx, &cure, &pi, 0, nz);

        let start = std::time::Instant::now();
        for layer in 0..n_layers {
            cure_bufs.write_intensity(&ctx, &intensity_grid);
            let mut encoder = ctx.device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("combined") },
            );
            cure_bufs.encode_cure_pass(
                &ctx, &mut encoder, layer, nz, exposure_sec, dp_um, k_d,
                layer_height_um, 0, nz,
            );
            strain_bufs.encode_copy_dose_from(&mut encoder, cure_bufs.cure_buf());
            strain_bufs.encode_strain_stress_pass(
                &ctx, &mut encoder, layer, ec_ref, dp_um, layer_height_um,
                linear_shrinkage_frac, z_anisotropy_ratio, youngs_modulus_mpa,
                poissons_ratio, nz, 0,
            );
            ctx.queue().submit(Some(encoder.finish()));
        }
        let combined_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "\nCombined encoder ({n_layers} layers, {nx}x{ny}x{nz}):\n  \
             {combined_ms:.1} ms  ({:.2} ms/layer)",
            combined_ms / n_layers as f64,
        );
    }
}
