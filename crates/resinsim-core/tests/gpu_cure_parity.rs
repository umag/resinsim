//! GPU/CPU parity tests for the cure + PI column march shader.
//! ADR-0025 Stage B. Tolerance-based: max |dose_gpu - dose_cpu| < 1e-3,
//! max |pi_gpu - pi_cpu| < 1e-5.

#![cfg(feature = "gpu")]

use resinsim_core::services::{GpuContext, GpuCureBuffers, VoxelCureCalculator};
use resinsim_core::values::{CureField, PenetrationDepth, PhotoinitiatorField};

fn try_gpu() -> Option<GpuContext> {
    GpuContext::try_new()
}

#[test]
fn gpu_cure_parity_basic_4x4x8() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => {
            eprintln!("gpu_cure_parity: no GPU adapter, skipping");
            return;
        }
    };

    let nx = 4u32;
    let ny = 4;
    let nz = 8;
    let voxel_size_mm = 0.05;
    let layer_height_um = 50.0;
    let dp_um = 100.0;
    let k_d = 0.05;
    let led_power = 10.0;
    let exposure_sec = 2.5;
    let iz_top = 0u32;

    let mut cure_cpu =
        CureField::new(nx, ny, nz, voxel_size_mm, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).expect("valid");

    let mut cure_gpu =
        CureField::new(nx, ny, nz, voxel_size_mm, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).expect("valid");

    let dp = PenetrationDepth::new(dp_um).expect("valid");

    let mut intensity_grid = vec![0.0f32; (nx as usize) * (ny as usize)];
    for iy in 0..ny {
        for ix in 0..nx {
            let pixel_intensity = led_power;
            intensity_grid[(iy as usize) * (nx as usize) + (ix as usize)] = pixel_intensity;

            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu,
                &mut pi_cpu,
                ix,
                iy,
                iz_top,
                pixel_intensity,
                exposure_sec,
                dp,
                k_d,
                layer_height_um,
            )
            .expect("CPU cure must succeed");
        }
    }

    let gpu_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu);
    gpu_bufs.dispatch(
        &ctx,
        &intensity_grid,
        iz_top,
        nz,
        exposure_sec,
        dp_um,
        k_d,
        layer_height_um,
    );
    gpu_bufs.download(&ctx, &mut cure_gpu, &mut pi_gpu);

    let mut max_dose_diff: f32 = 0.0;
    let mut max_pi_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let d_cpu = cure_cpu.dose_at(ix, iy, iz).expect("in bounds");
                let d_gpu = cure_gpu.dose_at(ix, iy, iz).expect("in bounds");
                let diff = (d_cpu - d_gpu).abs();
                if diff > max_dose_diff {
                    max_dose_diff = diff;
                }

                let p_cpu = pi_cpu.concentration_at(ix, iy, iz).expect("in bounds");
                let p_gpu = pi_gpu.concentration_at(ix, iy, iz).expect("in bounds");
                let pdiff = (p_cpu - p_gpu).abs();
                if pdiff > max_pi_diff {
                    max_pi_diff = pdiff;
                }
            }
        }
    }

    eprintln!(
        "gpu_cure_parity: max_dose_diff={max_dose_diff:.6e}, max_pi_diff={max_pi_diff:.6e}"
    );
    assert!(
        max_dose_diff < 1e-3,
        "dose parity: max diff {max_dose_diff} exceeds 1e-3"
    );
    assert!(
        max_pi_diff < 1e-5,
        "PI parity: max diff {max_pi_diff} exceeds 1e-5"
    );
    assert!(
        cure_cpu.max_dose() > 0.0,
        "CPU must have produced non-zero dose"
    );
}

#[test]
fn gpu_cure_parity_zero_intensity_noop() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => return,
    };

    let nx = 2u32;
    let ny = 2;
    let nz = 4;

    let mut cure_gpu =
        CureField::new(nx, ny, nz, 0.05, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).expect("valid");

    let intensity_grid = vec![0.0f32; (nx as usize) * (ny as usize)];
    let gpu_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu);
    gpu_bufs.dispatch(&ctx, &intensity_grid, 0, nz, 2.5, 100.0, 0.05, 50.0);
    gpu_bufs.download(&ctx, &mut cure_gpu, &mut pi_gpu);

    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                assert_eq!(
                    cure_gpu.dose_at(ix, iy, iz).expect("in bounds"),
                    0.0,
                    "zero-intensity must produce zero dose"
                );
                assert_eq!(
                    pi_gpu.concentration_at(ix, iy, iz).expect("in bounds"),
                    1.0,
                    "zero-intensity must leave PI unchanged"
                );
            }
        }
    }
}

#[test]
fn gpu_cure_parity_depleted_column() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => return,
    };

    let nx = 2u32;
    let ny = 2;
    let nz = 4;
    let dp_um = 100.0;
    let k_d = 0.05;
    let layer_height_um = 50.0;
    let exposure_sec = 2.5;
    let led_power = 10.0;

    let mut cure_cpu =
        CureField::new(nx, ny, nz, 0.05, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 0.02).expect("valid");

    let mut cure_gpu =
        CureField::new(nx, ny, nz, 0.05, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 0.02).expect("valid");

    let dp = PenetrationDepth::new(dp_um).expect("valid");
    let mut intensity_grid = vec![0.0f32; (nx as usize) * (ny as usize)];
    for iy in 0..ny {
        for ix in 0..nx {
            intensity_grid[(iy as usize) * (nx as usize) + (ix as usize)] = led_power;
            VoxelCureCalculator::apply_column_exposure(
                &mut cure_cpu, &mut pi_cpu, ix, iy, 0, led_power, exposure_sec,
                dp, k_d, layer_height_um,
            )
            .expect("CPU");
        }
    }

    let gpu_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu);
    gpu_bufs.dispatch(&ctx, &intensity_grid, 0, nz, exposure_sec, dp_um, k_d, layer_height_um);
    gpu_bufs.download(&ctx, &mut cure_gpu, &mut pi_gpu);

    let mut max_dose_diff: f32 = 0.0;
    let mut max_pi_diff: f32 = 0.0;
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let diff = (cure_cpu.dose_at(ix, iy, iz).expect("in bounds")
                    - cure_gpu.dose_at(ix, iy, iz).expect("in bounds"))
                .abs();
                max_dose_diff = max_dose_diff.max(diff);
                let pdiff = (pi_cpu.concentration_at(ix, iy, iz).expect("in bounds")
                    - pi_gpu.concentration_at(ix, iy, iz).expect("in bounds"))
                .abs();
                max_pi_diff = max_pi_diff.max(pdiff);
            }
        }
    }

    assert!(max_dose_diff < 1e-3, "depleted dose parity: {max_dose_diff}");
    assert!(max_pi_diff < 1e-5, "depleted PI parity: {max_pi_diff}");
}

#[test]
fn gpu_cure_parity_single_voxel_nz1() {
    let ctx = match try_gpu() {
        Some(c) => c,
        None => return,
    };

    let nx = 1u32;
    let ny = 1;
    let nz = 1;
    let dp_um = 1000.0;
    let k_d = 0.05;
    let layer_height_um = 1.0;
    let exposure_sec = 2.5;
    let led_power = 10.0;

    let mut cure_cpu =
        CureField::new(nx, ny, nz, 0.001, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_cpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).expect("valid");

    let mut cure_gpu =
        CureField::new(nx, ny, nz, 0.001, [0.0, 0.0, 0.0]).expect("valid");
    let mut pi_gpu = PhotoinitiatorField::new(nx, ny, nz, 1.0).expect("valid");

    let dp = PenetrationDepth::new(dp_um).expect("valid");
    let intensity_grid = vec![led_power; 1];

    VoxelCureCalculator::apply_column_exposure(
        &mut cure_cpu, &mut pi_cpu, 0, 0, 0, led_power, exposure_sec,
        dp, k_d, layer_height_um,
    )
    .expect("CPU");

    let gpu_bufs = GpuCureBuffers::new(&ctx, &cure_gpu, &pi_gpu);
    gpu_bufs.dispatch(&ctx, &intensity_grid, 0, nz, exposure_sec, dp_um, k_d, layer_height_um);
    gpu_bufs.download(&ctx, &mut cure_gpu, &mut pi_gpu);

    let dose_diff =
        (cure_cpu.dose_at(0, 0, 0).expect("in bounds") - cure_gpu.dose_at(0, 0, 0).expect("in bounds")).abs();
    let pi_diff = (pi_cpu.concentration_at(0, 0, 0).expect("in bounds")
        - pi_gpu.concentration_at(0, 0, 0).expect("in bounds"))
    .abs();

    eprintln!("nz=1 parity: dose_diff={dose_diff:.6e}, pi_diff={pi_diff:.6e}");
    assert!(dose_diff < 1e-3, "nz=1 dose parity: {dose_diff}");
    assert!(pi_diff < 1e-5, "nz=1 PI parity: {pi_diff}");
    assert!(cure_cpu.dose_at(0, 0, 0).expect("in bounds") > 0.0);
}
