//! GPU vs CPU light crosstalk XY convolution benchmark.
//! Run: cargo test --features gpu -p resinsim-core --test bench_gpu_crosstalk --release -- --nocapture

#![cfg(feature = "gpu")]

use ndarray::Array2;
use resinsim_core::services::{GpuContext, LightCrosstalkCalculator};
use resinsim_core::services::light_crosstalk_gpu::GpuCrosstalkBuffers;

fn bench_xy(
    ctx: &GpuContext,
    nx: u32,
    ny: u32,
    sigma_voxels: f32,
    n_layers: u32,
    label: &str,
) {
    let total_pixels = nx as u64 * ny as u64;
    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma_voxels)
        .expect("kernel");

    // Build a representative intensity grid (non-uniform, some zeros)
    let make_grid = || {
        Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
            if (ix + iy) % 3 == 0 { 0.0 } else { ((ix * 13 + iy * 7) % 11) as f32 + 0.5 }
        })
    };

    // --- CPU ---
    let cpu_start = std::time::Instant::now();
    for _ in 0..n_layers {
        let mut grid = make_grid();
        let mut scratch = Array2::<f32>::zeros((nx as usize, ny as usize));
        LightCrosstalkCalculator::apply_separable_2d(&mut grid, &kernel, &mut scratch)
            .expect("cpu conv");
    }
    let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

    // --- GPU ---
    let mut bufs = GpuCrosstalkBuffers::new(ctx, nx, ny);
    let gpu_start = std::time::Instant::now();
    for _ in 0..n_layers {
        let mut grid = make_grid();
        bufs.apply_separable_2d(ctx, &mut grid, &kernel);
    }
    let gpu_ms = gpu_start.elapsed().as_secs_f64() * 1000.0;

    let speedup = cpu_ms / gpu_ms;
    eprintln!(
        "\n{label}\n  {total_pixels} pixels ({nx}x{ny}), kernel_len={}, {n_layers} layers\n  \
         CPU: {cpu_ms:.1} ms  GPU: {gpu_ms:.1} ms  speedup: {speedup:.1}×",
        kernel.len(),
    );
}

#[test]
fn bench_lilith_torso_xy_conv_gpu_vs_cpu() {
    let ctx = match GpuContext::try_new() {
        Some(c) => c,
        None => {
            eprintln!("no GPU — skipping benchmark");
            return;
        }
    };
    eprintln!("\nGPU adapter: {}", ctx.adapter_name());

    // Mars 5 Ultra: 3840x2400 pixels @ 0.05mm voxel
    // σ_xy = 8µm → σ_voxels = 8 / (0.05 * 1000) = 0.16 → kernel radius ⌈0.48⌉ = 1, len 3
    let sigma_xy_um = 8.0_f32;
    let voxel_size_mm = 0.05_f32;
    let sigma_voxels = sigma_xy_um / (voxel_size_mm * 1000.0);

    // Small grid warmup
    bench_xy(&ctx, 64, 64, sigma_voxels, 10, "warmup 64x64");

    // Lilith torso scale, 10 layers
    bench_xy(&ctx, 3840, 2400, sigma_voxels, 10,
        "lilith torso 3840x2400, 10 layers, σ=0.16");

    // Lilith torso, 100 layers
    bench_xy(&ctx, 3840, 2400, sigma_voxels, 100,
        "lilith torso 3840x2400, 100 layers, σ=0.16");

    // Larger sigma (wider kernel) — σ=1.0
    bench_xy(&ctx, 3840, 2400, 1.0, 10,
        "lilith torso 3840x2400, 10 layers, σ=1.0 (7-tap kernel)");

    // Full 4492-layer run
    bench_xy(&ctx, 3840, 2400, sigma_voxels, 4492,
        "lilith torso FULL 4492 layers, σ=0.16");
}
