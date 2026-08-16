//! GPU vs CPU thermal solver benchmark using lilith torso envelope dimensions.
//! Run: cargo test --features gpu -p resinsim-core --test bench_gpu --release -- --nocapture

#![cfg(feature = "gpu")]

use resinsim_core::services::{
    BoundaryConditions, GpuContext, GpuThermalBuffers, ThermalDiffusionSolver,
};
use resinsim_core::values::ThermalField;

fn bcs() -> BoundaryConditions {
    BoundaryConditions {
        bottom_dirichlet_c: 40.0,
        top_h_w_m2k: 10.0,
        side_h_w_m2k: 8.0,
        ambient_c: 22.0,
        k_resin_w_mk: 0.20,
    }
}

const ALPHA: f32 = 1.07e-7;

#[allow(clippy::too_many_arguments)]
fn bench_one(
    ctx: &GpuContext,
    nx: u32, ny: u32, nz: u32,
    voxel_mm: f32, n_substeps: u32, n_layers: u32,
    label: &str,
) {
    let total = nx as u64 * ny as u64 * nz as u64;
    let dt = ThermalDiffusionSolver::cfl_max_dt(ALPHA, voxel_mm);
    let bcs = bcs();

    // --- CPU: n_layers × n_substeps ---
    let mut cpu_field =
        ThermalField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0], 25.0).expect("field");
    let mut scratch =
        ndarray::Array3::<f32>::zeros((nx as usize, ny as usize, nz as usize));
    let cpu_start = std::time::Instant::now();
    for _ in 0..n_layers {
        for _ in 0..n_substeps {
            ThermalDiffusionSolver::step(&mut cpu_field, &mut scratch, dt, ALPHA, &bcs)
                .expect("cpu step");
        }
    }
    let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

    // --- GPU: n_layers × n_substeps (batched per layer) ---
    let mut gpu_field =
        ThermalField::new(nx, ny, nz, voxel_mm, [0.0, 0.0, 0.0], 25.0).expect("field");
    let mut bufs = GpuThermalBuffers::new(ctx, &gpu_field).expect("thermal buffers");
    bufs.upload(ctx, &gpu_field);
    let gpu_start = std::time::Instant::now();
    for _ in 0..n_layers {
        bufs.dispatch_substeps(ctx, n_substeps, dt, ALPHA, voxel_mm, &bcs, nx, ny, nz);
    }
    bufs.download(ctx, &mut gpu_field);
    let gpu_ms = gpu_start.elapsed().as_secs_f64() * 1000.0;

    let speedup = cpu_ms / gpu_ms;
    let total_substeps = n_layers as u64 * n_substeps as u64;
    eprintln!(
        "\n{label}\n  {total} voxels, {n_layers} layers × {n_substeps} substeps = {total_substeps} total\n  \
         CPU: {cpu_ms:.1} ms  GPU: {gpu_ms:.1} ms  speedup: {speedup:.1}×"
    );
}

#[test]
fn bench_lilith_torso_gpu_vs_cpu() {
    let ctx = match GpuContext::try_new() {
        Some(c) => c,
        None => {
            eprintln!("no GPU — skipping benchmark");
            return;
        }
    };
    eprintln!("\nGPU adapter: {}", ctx.adapter_name());

    // generic_msla_4k envelope: 192×120×200 mm
    // lilith torso: 4492 layers @ 50µm

    // At 2mm thermal voxel (current floor): 96×60×100
    bench_one(&ctx, 96, 60, 100, 2.0, 30, 10,
        "lilith torso @ 2mm voxel, 10 layers");

    bench_one(&ctx, 96, 60, 100, 2.0, 30, 100,
        "lilith torso @ 2mm voxel, 100 layers");

    // At 1mm thermal voxel (future): 192×120×200
    bench_one(&ctx, 192, 120, 200, 1.0, 30, 10,
        "lilith torso @ 1mm voxel, 10 layers");

    // Full 4492-layer run at 2mm (the production config)
    bench_one(&ctx, 96, 60, 100, 2.0, 30, 4492,
        "lilith torso FULL 4492 layers @ 2mm voxel");
}
