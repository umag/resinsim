//! ADR-0025 / t2f5: GPU ↔ CPU thermal solver parity tests.
//!
//! Tolerance: max per-voxel absolute difference < 1e-3 °C after N
//! substeps on a reference field (ADR-0025 §Decision v).
//!
//! Tests skip gracefully when no GPU adapter is available via
//! `try_gpu_context()`.

#![cfg(feature = "gpu")]

use resinsim_core::services::{
    BoundaryConditions, GpuContext, GpuThermalBuffers, ThermalDiffusionSolver,
};
use resinsim_core::values::ThermalField;

fn try_gpu_context() -> Option<GpuContext> {
    GpuContext::try_new()
}

fn default_bcs() -> BoundaryConditions {
    BoundaryConditions {
        bottom_dirichlet_c: 40.0,
        top_h_w_m2k: 10.0,
        side_h_w_m2k: 8.0,
        ambient_c: 22.0,
        k_resin_w_mk: 0.20,
    }
}

const ALPHA: f32 = 1.07e-7;
const VOXEL_MM: f32 = 0.5;
const TOLERANCE: f32 = 1e-3;

#[test]
fn gpu_context_creation_succeeds_or_skips() {
    match try_gpu_context() {
        Some(ctx) => {
            assert!(!ctx.adapter_name().is_empty());
            eprintln!("GPU adapter: {}", ctx.adapter_name());
        }
        None => {
            eprintln!("no GPU adapter — skipping GPU tests");
        }
    }
}

#[test]
fn gpu_cpu_parity_uniform_field_20_substeps() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4_u32;
    let nz = 4_u32;
    let initial_c = 25.0_f32;
    let bcs = default_bcs();
    let dt = ThermalDiffusionSolver::cfl_max_dt(ALPHA, VOXEL_MM);
    let n_substeps = 20_u32;

    // CPU path
    let mut cpu_field =
        ThermalField::new(nx, ny, nz, VOXEL_MM, [0.0, 0.0, 0.0], initial_c)
            .expect("CPU field");
    let mut scratch =
        ndarray::Array3::<f32>::zeros((nx as usize, ny as usize, nz as usize));
    for _ in 0..n_substeps {
        ThermalDiffusionSolver::step(&mut cpu_field, &mut scratch, dt, ALPHA, &bcs)
            .expect("CPU step");
    }

    // GPU path
    let mut gpu_field =
        ThermalField::new(nx, ny, nz, VOXEL_MM, [0.0, 0.0, 0.0], initial_c)
            .expect("GPU field");
    let mut bufs = GpuThermalBuffers::new(&ctx, &gpu_field);
    bufs.upload(&ctx, &gpu_field);
    bufs.dispatch_substeps(&ctx, n_substeps, dt, ALPHA, VOXEL_MM, &bcs, nx, ny, nz);
    bufs.download(&ctx, &mut gpu_field);

    // Compare
    let cpu_view = cpu_field.as_array_view();
    let gpu_view = gpu_field.as_array_view();
    let mut max_diff: f32 = 0.0;
    for (cpu_val, gpu_val) in cpu_view.iter().zip(gpu_view.iter()) {
        let diff = (cpu_val - gpu_val).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    eprintln!("max CPU-GPU difference: {max_diff:.6e} °C (tolerance: {TOLERANCE:.1e})");
    assert!(
        max_diff < TOLERANCE,
        "GPU/CPU parity violation: max diff {max_diff} >= {TOLERANCE}"
    );
}

#[test]
fn gpu_cpu_parity_odd_substep_count() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4_u32;
    let nz = 4_u32;
    let initial_c = 30.0_f32;
    let bcs = default_bcs();
    let dt = ThermalDiffusionSolver::cfl_max_dt(ALPHA, VOXEL_MM);

    // CPU: 7 substeps (odd)
    let mut cpu_field =
        ThermalField::new(nx, ny, nz, VOXEL_MM, [0.0, 0.0, 0.0], initial_c)
            .expect("CPU field");
    let mut scratch =
        ndarray::Array3::<f32>::zeros((nx as usize, ny as usize, nz as usize));
    for _ in 0..7 {
        ThermalDiffusionSolver::step(&mut cpu_field, &mut scratch, dt, ALPHA, &bcs)
            .expect("CPU step");
    }

    // GPU: 7 substeps (odd — tests ping-pong buffer tracking)
    let mut gpu_field =
        ThermalField::new(nx, ny, nz, VOXEL_MM, [0.0, 0.0, 0.0], initial_c)
            .expect("GPU field");
    let mut bufs = GpuThermalBuffers::new(&ctx, &gpu_field);
    bufs.upload(&ctx, &gpu_field);
    bufs.dispatch_substeps(&ctx, 7, dt, ALPHA, VOXEL_MM, &bcs, nx, ny, nz);
    bufs.download(&ctx, &mut gpu_field);

    let cpu_view = cpu_field.as_array_view();
    let gpu_view = gpu_field.as_array_view();
    let mut max_diff: f32 = 0.0;
    for (cpu_val, gpu_val) in cpu_view.iter().zip(gpu_view.iter()) {
        let diff = (cpu_val - gpu_val).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    eprintln!("odd-count max diff: {max_diff:.6e} °C");
    assert!(
        max_diff < TOLERANCE,
        "GPU/CPU parity violation (odd substeps): max diff {max_diff} >= {TOLERANCE}"
    );
}

#[test]
fn gpu_dirichlet_bottom_pinned() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 4_u32;
    let ny = 4_u32;
    let nz = 4_u32;
    let bcs = default_bcs();
    let dt = ThermalDiffusionSolver::cfl_max_dt(ALPHA, VOXEL_MM);

    let mut field =
        ThermalField::new(nx, ny, nz, VOXEL_MM, [0.0, 0.0, 0.0], 25.0)
            .expect("field");
    let mut bufs = GpuThermalBuffers::new(&ctx, &field);
    bufs.upload(&ctx, &field);
    bufs.dispatch_substeps(&ctx, 10, dt, ALPHA, VOXEL_MM, &bcs, nx, ny, nz);
    bufs.download(&ctx, &mut field);

    for ix in 0..nx {
        for iy in 0..ny {
            let t = field
                .temperature_at(ix, iy, 0)
                .expect("in-bounds");
            assert!(
                (t - 40.0).abs() < 0.01,
                "z=0 voxel ({ix}, {iy}, 0) must equal Dirichlet 40.0, got {t}"
            );
        }
    }
}
