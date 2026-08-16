//! ADR-0025 / t2f5 Stage C: GPU ↔ CPU light crosstalk XY convolution parity.
//!
//! Tolerance: max per-pixel absolute difference < 1e-3 after separable
//! 2D Gaussian convolution (ADR-0025 §Decision v).
//!
//! Tests skip gracefully when no GPU adapter is available via
//! `try_gpu_context()`.

#![cfg(feature = "gpu")]

use ndarray::Array2;
use resinsim_core::services::{
    GpuContext, LightCrosstalkCalculator,
};
use resinsim_core::services::light_crosstalk_gpu::GpuCrosstalkBuffers;

fn try_gpu_context() -> Option<GpuContext> {
    GpuContext::try_new()
}

const TOLERANCE: f32 = 1e-3;

#[test]
fn gpu_cpu_xy_parity_impulse_response() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 7_u32;
    let ny = 7_u32;
    let sigma = 1.0_f32;

    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1 kernel");

    // CPU path
    let mut cpu_grid = Array2::<f32>::zeros((nx as usize, ny as usize));
    cpu_grid[(3, 3)] = 1.0;
    let mut cpu_scratch = Array2::<f32>::zeros((nx as usize, ny as usize));
    LightCrosstalkCalculator::apply_separable_2d(&mut cpu_grid, &kernel, &mut cpu_scratch)
        .expect("CPU conv");

    // GPU path
    let mut gpu_grid = Array2::<f32>::zeros((nx as usize, ny as usize));
    gpu_grid[(3, 3)] = 1.0;
    let mut bufs = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs.apply_separable_2d(&ctx, &mut gpu_grid, &kernel);

    // Compare
    let mut max_diff: f32 = 0.0;
    for ix in 0..nx as usize {
        for iy in 0..ny as usize {
            let diff = (cpu_grid[(ix, iy)] - gpu_grid[(ix, iy)]).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
    }
    eprintln!("impulse max CPU-GPU diff: {max_diff:.6e} (tolerance: {TOLERANCE:.1e})");
    assert!(
        max_diff < TOLERANCE,
        "GPU/CPU parity violation: max diff {max_diff} >= {TOLERANCE}"
    );
}

#[test]
fn gpu_cpu_xy_parity_energy_conservation() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 8_u32;
    let ny = 8_u32;
    let sigma = 1.5_f32;

    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1.5 kernel");

    let mut gpu_grid = Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
        ((ix * 13 + iy * 7) % 11) as f32 + 0.5
    });
    let sum_before: f32 = gpu_grid.iter().sum();

    let mut bufs = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs.apply_separable_2d(&ctx, &mut gpu_grid, &kernel);

    let sum_after: f32 = gpu_grid.iter().sum();
    eprintln!("energy: before={sum_before:.4}, after={sum_after:.4}");
    assert!(
        sum_after <= sum_before * 1.001 + 1e-4,
        "GPU energy conservation violated: {sum_after} > {sum_before} * 1.001"
    );
}

#[test]
fn gpu_identity_kernel_is_noop() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 5_u32;
    let ny = 5_u32;

    let kernel = LightCrosstalkCalculator::build_separable_kernel(0.0)
        .expect("sigma=0 identity kernel");
    assert_eq!(kernel, vec![1.0]);

    let mut grid = Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
        (ix * 5 + iy) as f32 + 1.0
    });
    let before = grid.clone();

    let mut bufs = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs.apply_separable_2d(&ctx, &mut grid, &kernel);

    for ix in 0..nx as usize {
        for iy in 0..ny as usize {
            assert_eq!(
                grid[(ix, iy)],
                before[(ix, iy)],
                "identity kernel must be no-op at ({ix},{iy})"
            );
        }
    }
}

#[test]
fn begin_dispatch_finish_download_matches_apply() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 8_u32;
    let ny = 8_u32;
    let sigma = 1.0_f32;
    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1 kernel");

    let make_grid = || {
        Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
            ((ix * 13 + iy * 7) % 11) as f32 + 0.5
        })
    };

    // One-shot path
    let mut oneshot_grid = make_grid();
    let mut bufs_a = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs_a.apply_separable_2d(&ctx, &mut oneshot_grid, &kernel);

    // Split path: begin_dispatch then finish_download
    let input_grid = make_grid();
    let mut bufs_b = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs_b.begin_dispatch(&ctx, &input_grid, &kernel);
    let split_grid = bufs_b.finish_download(&ctx);

    for ix in 0..nx as usize {
        for iy in 0..ny as usize {
            assert_eq!(
                oneshot_grid[(ix, iy)],
                split_grid[(ix, iy)],
                "begin/finish must match apply_separable_2d at ({ix},{iy})"
            );
        }
    }
}

#[test]
fn pipelined_multi_layer_matches_sequential() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 8_u32;
    let ny = 8_u32;
    let sigma = 1.0_f32;
    let n_layers = 6_u32;
    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1 kernel");

    let make_grid = |layer: u32| {
        Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
            ((ix * 13 + iy * 7 + layer as usize * 3) % 11) as f32 + 0.5
        })
    };

    // Sequential: one-shot per layer
    let mut sequential_results = Vec::new();
    let mut bufs_seq = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    for layer in 0..n_layers {
        let mut grid = make_grid(layer);
        bufs_seq.apply_separable_2d(&ctx, &mut grid, &kernel);
        sequential_results.push(grid);
    }

    // Pipelined: download(K) → dispatch(K+1) → process(K)
    let mut pipelined_results = Vec::new();
    let mut bufs_pipe = GpuCrosstalkBuffers::new(&ctx, nx, ny);

    // Prologue: dispatch layer 0
    let grid_0 = make_grid(0);
    bufs_pipe.begin_dispatch(&ctx, &grid_0, &kernel);

    for k in 0..n_layers {
        // Download K
        let convolved = bufs_pipe.finish_download(&ctx);

        // Dispatch K+1 (if exists)
        if k + 1 < n_layers {
            let grid_next = make_grid(k + 1);
            bufs_pipe.begin_dispatch(&ctx, &grid_next, &kernel);
        }

        // "Process" K — just save result
        pipelined_results.push(convolved);
    }

    // Compare
    for layer in 0..n_layers as usize {
        for ix in 0..nx as usize {
            for iy in 0..ny as usize {
                assert_eq!(
                    sequential_results[layer][(ix, iy)],
                    pipelined_results[layer][(ix, iy)],
                    "pipelined must match sequential at layer={layer} ({ix},{iy})"
                );
            }
        }
    }
}

#[test]
fn pipelined_single_layer_no_dispatch_next() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 5_u32;
    let ny = 5_u32;
    let sigma = 1.0_f32;
    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1 kernel");

    let grid = Array2::<f32>::from_shape_fn((nx as usize, ny as usize), |(ix, iy)| {
        (ix * 5 + iy) as f32 + 1.0
    });

    // One-shot
    let mut oneshot = grid.clone();
    let mut bufs_a = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs_a.apply_separable_2d(&ctx, &mut oneshot, &kernel);

    // Pipelined single layer: prologue dispatch, download, no dispatch-next
    let mut bufs_b = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs_b.begin_dispatch(&ctx, &grid, &kernel);
    let pipelined = bufs_b.finish_download(&ctx);

    for ix in 0..nx as usize {
        for iy in 0..ny as usize {
            assert_eq!(
                oneshot[(ix, iy)],
                pipelined[(ix, iy)],
                "single-layer pipelined must match at ({ix},{iy})"
            );
        }
    }
}

#[test]
fn gpu_cpu_xy_parity_edge_clamp_to_zero() {
    let ctx = match try_gpu_context() {
        Some(c) => c,
        None => {
            eprintln!("no GPU adapter — skipping");
            return;
        }
    };

    let nx = 7_u32;
    let ny = 7_u32;
    let sigma = 1.0_f32;

    let kernel = LightCrosstalkCalculator::build_separable_kernel(sigma)
        .expect("sigma=1 kernel");

    // Impulse at corner (0,0) — exercises clamp-to-zero SKIP edges
    let mut cpu_grid = Array2::<f32>::zeros((nx as usize, ny as usize));
    cpu_grid[(0, 0)] = 1.0;
    let mut cpu_scratch = Array2::<f32>::zeros((nx as usize, ny as usize));
    LightCrosstalkCalculator::apply_separable_2d(&mut cpu_grid, &kernel, &mut cpu_scratch)
        .expect("CPU conv");

    let mut gpu_grid = Array2::<f32>::zeros((nx as usize, ny as usize));
    gpu_grid[(0, 0)] = 1.0;
    let mut bufs = GpuCrosstalkBuffers::new(&ctx, nx, ny);
    bufs.apply_separable_2d(&ctx, &mut gpu_grid, &kernel);

    let mut max_diff: f32 = 0.0;
    for ix in 0..nx as usize {
        for iy in 0..ny as usize {
            let diff = (cpu_grid[(ix, iy)] - gpu_grid[(ix, iy)]).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
    }
    eprintln!("edge max CPU-GPU diff: {max_diff:.6e} (tolerance: {TOLERANCE:.1e})");
    assert!(
        max_diff < TOLERANCE,
        "GPU/CPU edge parity violation: max diff {max_diff} >= {TOLERANCE}"
    );
}
