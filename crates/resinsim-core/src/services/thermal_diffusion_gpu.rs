//! ADR-0025 / t2f5: GPU-accelerated FTCS thermal diffusion solver.
//!
//! Implements the same 7-point stencil as `ThermalDiffusionSolver` (CPU,
//! ADR-0020) but dispatches via a WGSL compute shader on the GPU. Uses
//! a ping-pong double-buffer pattern: two storage buffers swap
//! read/write roles each substep.
//!
//! GPU/CPU parity is tolerance-based (ADR-0025 §Decision v): max
//! per-voxel absolute difference < 1e-3 °C. Results are NOT
//! byte-identical across device classes.

#![cfg(feature = "gpu")]

use wgpu::util::DeviceExt;

use crate::services::gpu_context::GpuContext;
use crate::services::thermal_diffusion_solver::BoundaryConditions;
use crate::values::ThermalField;

const WGSL_SHADER: &str = r#"
struct Params {
    nx: u32,
    ny: u32,
    nz: u32,
    r: f32,
    bi_top: f32,
    bi_side: f32,
    t_amb: f32,
    bottom_dirichlet_c: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> t_old: array<f32>;
@group(0) @binding(2) var<storage, read_write> t_new: array<f32>;

// ndarray row-major (C-order): last axis (iz) varies fastest.
// strides = (ny*nz, nz, 1)
fn idx(ix: u32, iy: u32, iz: u32) -> u32 {
    return ix * params.ny * params.nz + iy * params.nz + iz;
}

@compute @workgroup_size(64)
fn ftcs_step(@builtin(global_invocation_id) gid: vec3<u32>) {
    // 2D dispatch: linear = gid.y * (ceil(total/64) clamped to 65535) * 64 + gid.x
    // Each workgroup row is 64 threads wide; gid.y indexes the row of
    // workgroups when total_workgroups > 65535.
    let wg_x = min(
        (params.nx * params.ny * params.nz + 63u) / 64u,
        65535u
    );
    let linear = gid.y * wg_x * 64u + gid.x;
    let total = params.nx * params.ny * params.nz;
    if linear >= total {
        return;
    }
    // Decompose linear index matching row-major layout
    let iz = linear % params.nz;
    let rem = linear / params.nz;
    let iy = rem % params.ny;
    let ix = rem / params.ny;

    // Dirichlet bottom
    if iz == 0u {
        t_new[linear] = params.bottom_dirichlet_c;
        return;
    }

    let t = t_old[linear];
    let nx = params.nx;
    let ny = params.ny;
    let nz = params.nz;

    // X neighbours with Robin ghost
    var t_xm: f32;
    if ix > 0u {
        t_xm = t_old[idx(ix - 1u, iy, iz)];
    } else {
        let inner = t_old[idx(1u, iy, iz)];
        t_xm = inner - 2.0 * params.bi_side * (t - params.t_amb);
    }
    var t_xp: f32;
    if ix + 1u < nx {
        t_xp = t_old[idx(ix + 1u, iy, iz)];
    } else {
        let inner = t_old[idx(nx - 2u, iy, iz)];
        t_xp = inner - 2.0 * params.bi_side * (t - params.t_amb);
    }

    // Y neighbours with Robin ghost
    var t_ym: f32;
    if iy > 0u {
        t_ym = t_old[idx(ix, iy - 1u, iz)];
    } else {
        let inner = t_old[idx(ix, 1u, iz)];
        t_ym = inner - 2.0 * params.bi_side * (t - params.t_amb);
    }
    var t_yp: f32;
    if iy + 1u < ny {
        t_yp = t_old[idx(ix, iy + 1u, iz)];
    } else {
        let inner = t_old[idx(ix, ny - 2u, iz)];
        t_yp = inner - 2.0 * params.bi_side * (t - params.t_amb);
    }

    // Z neighbours
    let t_zm = t_old[idx(ix, iy, iz - 1u)];
    var t_zp: f32;
    if iz + 1u < nz {
        t_zp = t_old[idx(ix, iy, iz + 1u)];
    } else {
        let inner = t_old[idx(ix, iy, nz - 2u)];
        t_zp = inner - 2.0 * params.bi_top * (t - params.t_amb);
    }

    t_new[linear] = t + params.r * (t_xm + t_xp + t_ym + t_yp + t_zm + t_zp - 6.0 * t);
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    nx: u32,
    ny: u32,
    nz: u32,
    r: f32,
    bi_top: f32,
    bi_side: f32,
    t_amb: f32,
    bottom_dirichlet_c: f32,
}

pub struct GpuThermalBuffers {
    buf_a: wgpu::Buffer,
    buf_b: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group_a_to_b: wgpu::BindGroup,
    bind_group_b_to_a: wgpu::BindGroup,
    current_is_a: bool,
    total_voxels: u32,
}

impl GpuThermalBuffers {
    pub fn new(ctx: &GpuContext, field: &ThermalField) -> Self {
        let (nx_u, ny_u, nz_u) = field.dimensions();
        let nx = nx_u as usize;
        let ny = ny_u as usize;
        let nz = nz_u as usize;
        let total = nx * ny * nz;
        let data: Vec<f32> = field
            .as_array_view()
            .iter()
            .copied()
            .collect();

        let device = ctx.device();

        let buf_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("thermal_a"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
        let buf_b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("thermal_b"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: (total * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ftcs_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ftcs_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ftcs_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ftcs_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("ftcs_step"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_a_to_b = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ftcs_a_to_b"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_a.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_b.as_entire_binding(),
                },
            ],
        });

        let bind_group_b_to_a = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ftcs_b_to_a"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_b.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_a.as_entire_binding(),
                },
            ],
        });

        Self {
            buf_a,
            buf_b,
            params_buf,
            staging_buf,
            pipeline,
            bind_group_a_to_b,
            bind_group_b_to_a,
            current_is_a: true,
            total_voxels: total as u32,
        }
    }

    /// Upload the current ThermalField data to the GPU buffer that
    /// holds the "current" state.
    pub fn upload(&mut self, ctx: &GpuContext, field: &ThermalField) {
        let data: Vec<f32> = field.as_array_view().iter().copied().collect();
        let buf = if self.current_is_a {
            &self.buf_a
        } else {
            &self.buf_b
        };
        ctx.queue()
            .write_buffer(buf, 0, bytemuck::cast_slice(&data));
    }

    /// Run `n_substeps` FTCS substeps on the GPU with the given
    /// parameters. All substeps are batched into a single command
    /// buffer with alternating compute passes (ping-pong), submitted
    /// once. Does NOT download results — call `download` after.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_substeps(
        &mut self,
        ctx: &GpuContext,
        n_substeps: u32,
        dt_sec: f32,
        alpha_m2_s: f32,
        voxel_size_mm: f32,
        bcs: &BoundaryConditions,
        nx: u32,
        ny: u32,
        nz: u32,
    ) {
        let h_m = voxel_size_mm * 1e-3;
        let h2 = h_m * h_m;
        let r = dt_sec * alpha_m2_s / h2;
        let bi_top = bcs.top_h_w_m2k * h_m / bcs.k_resin_w_mk;
        let bi_side = bcs.side_h_w_m2k * h_m / bcs.k_resin_w_mk;

        let params = GpuParams {
            nx,
            ny,
            nz,
            r,
            bi_top,
            bi_side,
            t_amb: bcs.ambient_c,
            bottom_dirichlet_c: bcs.bottom_dirichlet_c,
        };
        ctx.queue()
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        let total_workgroups = self.total_voxels.div_ceil(64);
        // wgpu limits each dispatch dimension to 65535. Split into
        // (wg_x, wg_y, 1) where wg_x × wg_y >= total_workgroups.
        let wg_x = total_workgroups.min(65535);
        let wg_y = total_workgroups.div_ceil(wg_x);

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ftcs_encoder"),
            });
        for _ in 0..n_substeps {
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("ftcs_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                let bg = if self.current_is_a {
                    &self.bind_group_a_to_b
                } else {
                    &self.bind_group_b_to_a
                };
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(wg_x, wg_y, 1);
            }
            self.current_is_a = !self.current_is_a;
        }
        ctx.queue().submit(Some(encoder.finish()));
    }

    /// Download the current GPU buffer contents back into a
    /// `ThermalField`. Blocks until the GPU→CPU transfer completes.
    pub fn download(&self, ctx: &GpuContext, field: &mut ThermalField) {
        let src = if self.current_is_a {
            &self.buf_a
        } else {
            &self.buf_b
        };
        let size = (self.total_voxels as usize * std::mem::size_of::<f32>()) as u64;

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("download_encoder"),
            });
        encoder.copy_buffer_to_buffer(src, 0, &self.staging_buf, 0, size);
        ctx.queue().submit(Some(encoder.finish()));

        let slice = self.staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        ctx.device().poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("GPU map_async channel closed")
            .expect("GPU map_async failed");

        let mapped = slice.get_mapped_range();
        let gpu_data: &[f32] = bytemuck::cast_slice(&mapped);
        let mut data = field.as_array_mut();
        for (dst, &src_val) in data.iter_mut().zip(gpu_data.iter()) {
            *dst = src_val;
        }
        drop(mapped);
        self.staging_buf.unmap();
    }
}
