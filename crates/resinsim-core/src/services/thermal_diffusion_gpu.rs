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
    slab_nz: u32,
    r: f32,
    bi_top: f32,
    bi_side: f32,
    t_amb: f32,
    bottom_dirichlet_c: f32,
    slab_iz_start: u32,
    nz_global: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> t_old: array<f32>;
@group(0) @binding(2) var<storage, read_write> t_new: array<f32>;

fn idx(ix: u32, iy: u32, iz: u32) -> u32 {
    return ix * params.ny * params.slab_nz + iy * params.slab_nz + iz;
}

@compute @workgroup_size(64)
fn ftcs_step(@builtin(global_invocation_id) gid: vec3<u32>) {
    let wg_x = min(
        (params.nx * params.ny * params.slab_nz + 63u) / 64u,
        65535u
    );
    let linear = gid.y * wg_x * 64u + gid.x;
    let total = params.nx * params.ny * params.slab_nz;
    if linear >= total {
        return;
    }
    let iz = linear % params.slab_nz;
    let rem = linear / params.slab_nz;
    let iy = rem % params.ny;
    let ix = rem / params.ny;

    let global_iz = params.slab_iz_start + iz;

    // Dirichlet bottom (global domain boundary)
    if global_iz == 0u {
        t_new[linear] = params.bottom_dirichlet_c;
        return;
    }

    let t = t_old[linear];
    let nx = params.nx;
    let ny = params.ny;
    let slab_nz = params.slab_nz;

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

    // Z neighbours — boundary conditions use global position
    var t_zm: f32;
    if iz > 0u {
        t_zm = t_old[idx(ix, iy, iz - 1u)];
    } else {
        // iz==0 in slab but global_iz > 0 (Dirichlet handled above).
        // This cell is at the slab's bottom edge — halo data is in the buffer
        // if present, otherwise use Dirichlet as fallback.
        t_zm = params.bottom_dirichlet_c;
    }
    var t_zp: f32;
    if iz + 1u < slab_nz {
        t_zp = t_old[idx(ix, iy, iz + 1u)];
    } else if global_iz + 1u >= params.nz_global {
        // Domain top boundary — Robin condition
        let inner = t_old[idx(ix, iy, iz - 1u)];
        t_zp = inner - 2.0 * params.bi_top * (t - params.t_amb);
    } else {
        // Slab top edge but not domain top — halo data should be in buffer.
        // If no halo, use current value as approximation.
        t_zp = t;
    }

    t_new[linear] = t + params.r * (t_xm + t_xp + t_ym + t_yp + t_zm + t_zp - 6.0 * t);
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    nx: u32,
    ny: u32,
    slab_nz: u32,
    r: f32,
    bi_top: f32,
    bi_side: f32,
    t_amb: f32,
    bottom_dirichlet_c: f32,
    slab_iz_start: u32,
    nz_global: u32,
    _pad: [u32; 2],
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
    nx: u32,
    ny: u32,
    nz_global: u32,
    slab_nz: u32,
}

impl GpuThermalBuffers {
    pub fn new(ctx: &GpuContext, field: &ThermalField) -> Option<Self> {
        let (nx_u, ny_u, nz_u) = field.dimensions();
        let nx = nx_u as usize;
        let ny = ny_u as usize;
        let nz = nz_u as usize;

        let xy_bytes = (nx * ny * std::mem::size_of::<f32>()) as u64;
        if xy_bytes == 0 || xy_bytes > ctx.max_buffer_size() {
            return None;
        }
        let max_nz = (ctx.max_buffer_size() / xy_bytes) as usize;
        let needs_multi_slab = max_nz < nz;
        let slab_nz = if needs_multi_slab {
            (max_nz.saturating_sub(2)).max(1)
        } else {
            nz
        };
        let buf_nz = if needs_multi_slab { slab_nz + 2 } else { slab_nz };
        let slab_total = nx * ny * buf_nz;
        let slab_bytes = (slab_total * std::mem::size_of::<f32>()) as u64;

        let device = ctx.device();

        let buf_a = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal_a"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal_b"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: slab_bytes,
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

        Some(Self {
            buf_a,
            buf_b,
            params_buf,
            staging_buf,
            pipeline,
            bind_group_a_to_b,
            bind_group_b_to_a,
            current_is_a: true,
            nx: nx_u,
            ny: ny_u,
            nz_global: nz_u,
            slab_nz: slab_nz as u32,
        })
    }

    /// Upload the current ThermalField data to the GPU buffer for a
    /// specific slab. Gathers the slab's Z-range (including halo cells
    /// from adjacent slabs) into the slab buffer.
    fn upload_slab(
        &self,
        ctx: &GpuContext,
        field: &ThermalField,
        slab_iz_start: u32,
        this_slab_nz: u32,
    ) {
        let slab = Self::gather_thermal_slab(
            field, self.nx, self.ny, self.nz_global, slab_iz_start, this_slab_nz,
        );
        let buf = if self.current_is_a {
            &self.buf_a
        } else {
            &self.buf_b
        };
        ctx.queue()
            .write_buffer(buf, 0, bytemuck::cast_slice(&slab));
    }

    /// Upload the full thermal field for single-slab case (backward compat).
    pub fn upload(&mut self, ctx: &GpuContext, field: &ThermalField) {
        if self.slab_nz >= self.nz_global {
            let data: Vec<f32> = field.as_array_view().iter().copied().collect();
            let buf = if self.current_is_a {
                &self.buf_a
            } else {
                &self.buf_b
            };
            ctx.queue()
                .write_buffer(buf, 0, bytemuck::cast_slice(&data));
        } else {
            self.upload_slab(ctx, field, 0, self.slab_nz.min(self.nz_global));
        }
    }

    /// Run `n_substeps` FTCS substeps across all Z-slabs. For single-slab
    /// volumes, this batches all substeps in one encoder (same perf as before).
    /// For multi-slab, dispatches 1 substep at a time with boundary exchange
    /// between slabs.
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

        if self.slab_nz >= nz {
            // Single slab — batch all substeps (original fast path).
            let params = GpuParams {
                nx, ny,
                slab_nz: nz,
                r, bi_top, bi_side,
                t_amb: bcs.ambient_c,
                bottom_dirichlet_c: bcs.bottom_dirichlet_c,
                slab_iz_start: 0,
                nz_global: nz,
                _pad: [0; 2],
            };
            ctx.queue()
                .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

            let slab_voxels = (nx * ny * nz) as u32;
            let total_workgroups = slab_voxels.div_ceil(64);
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
            return;
        }

        panic!(
            "multi-slab thermal dispatch requires dispatch_substeps_with_field; \
             dispatch_substeps only supports single-slab (nz <= slab_nz)"
        );
    }

    /// Download the current GPU buffer contents back into a ThermalField.
    /// For single-slab, downloads directly. For multi-slab, the data was
    /// already committed to the host in dispatch_substeps.
    pub fn download(&self, ctx: &GpuContext, field: &mut ThermalField) {
        let src = if self.current_is_a {
            &self.buf_a
        } else {
            &self.buf_b
        };

        if self.slab_nz >= self.nz_global {
            // Single slab — download directly
            let size = (self.nx as u64) * (self.ny as u64) * (self.nz_global as u64)
                * std::mem::size_of::<f32>() as u64;
            let mut encoder = ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("download_encoder"),
                });
            encoder.copy_buffer_to_buffer(src, 0, &self.staging_buf, 0, size);
            ctx.queue().submit(Some(encoder.finish()));

            let slice = self.staging_buf.slice(..size);
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
        } else {
            // Multi-slab: dispatch_substeps already wrote results to host.
            // Download each slab from the host data that was stored in
            // dispatch_substeps. Since we uploaded the first slab back to
            // GPU, we can download it, but for simplicity the caller should
            // use the field that dispatch_substeps_with_field populated.
            let slab_bytes = (self.nx as u64) * (self.ny as u64)
                * (self.slab_nz.min(self.nz_global) as u64) * std::mem::size_of::<f32>() as u64;
            let mut encoder = ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("download_encoder"),
                });
            encoder.copy_buffer_to_buffer(src, 0, &self.staging_buf, 0, slab_bytes);
            ctx.queue().submit(Some(encoder.finish()));

            let slab_data = Self::read_staging_flat(ctx, &self.staging_buf, slab_bytes);
            let mut flat = field.as_array_mut();
            let flat_slice = flat.as_slice_mut().expect("contiguous");
            Self::scatter_to_flat(
                &slab_data, flat_slice,
                self.nx, self.ny, self.nz_global, 0, self.slab_nz.min(self.nz_global),
            );
        }
    }

    /// Run substeps with direct access to the thermal field for multi-slab.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_substeps_with_field(
        &mut self,
        ctx: &GpuContext,
        n_substeps: u32,
        dt_sec: f32,
        alpha_m2_s: f32,
        voxel_size_mm: f32,
        bcs: &BoundaryConditions,
        field: &mut ThermalField,
    ) {
        let (nx, ny, nz) = field.dimensions();
        if self.slab_nz >= nz {
            self.upload(ctx, field);
            self.dispatch_substeps(ctx, n_substeps, dt_sec, alpha_m2_s, voxel_size_mm, bcs, nx, ny, nz);
            self.download(ctx, field);
            return;
        }

        let h_m = voxel_size_mm * 1e-3;
        let h2 = h_m * h_m;
        let r = dt_sec * alpha_m2_s / h2;
        let bi_top = bcs.top_h_w_m2k * h_m / bcs.k_resin_w_mk;
        let bi_side = bcs.side_h_w_m2k * h_m / bcs.k_resin_w_mk;

        let slab_nz = self.slab_nz;
        let mut host_arr = field.as_array_mut();
        let host_flat = host_arr.as_slice_mut().expect("contiguous");

        for _substep in 0..n_substeps {
            for slab_start in (0..nz).step_by(slab_nz as usize) {
                let interior_nz = slab_nz.min(nz - slab_start);
                let buf_iz_start = if slab_start > 0 { slab_start - 1 } else { 0 };
                let buf_iz_end = (slab_start + interior_nz + 1).min(nz);
                let buf_nz = buf_iz_end - buf_iz_start;
                let halo_bottom = slab_start - buf_iz_start;

                let slab = Self::gather_from_flat(
                    host_flat, nx, ny, nz, buf_iz_start, buf_nz,
                );
                let buf = if self.current_is_a { &self.buf_a } else { &self.buf_b };
                ctx.queue().write_buffer(buf, 0, bytemuck::cast_slice(&slab));

                let params = GpuParams {
                    nx, ny,
                    slab_nz: buf_nz,
                    r, bi_top, bi_side,
                    t_amb: bcs.ambient_c,
                    bottom_dirichlet_c: bcs.bottom_dirichlet_c,
                    slab_iz_start: buf_iz_start,
                    nz_global: nz,
                    _pad: [0; 2],
                };
                ctx.queue()
                    .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

                let this_wg_total = (nx * ny * buf_nz).div_ceil(64);
                let this_wg_x = this_wg_total.min(65535);
                let this_wg_y = this_wg_total.div_ceil(this_wg_x);

                let mut encoder = ctx
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("ftcs_slab_encoder"),
                    });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("ftcs_slab_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    let bg = if self.current_is_a {
                        &self.bind_group_a_to_b
                    } else {
                        &self.bind_group_b_to_a
                    };
                    pass.set_bind_group(0, bg, &[]);
                    pass.dispatch_workgroups(this_wg_x, this_wg_y, 1);
                }
                self.current_is_a = !self.current_is_a;
                ctx.queue().submit(Some(encoder.finish()));

                let result_buf = if self.current_is_a { &self.buf_a } else { &self.buf_b };
                let buf_bytes = (nx as u64) * (ny as u64)
                    * (buf_nz as u64) * std::mem::size_of::<f32>() as u64;
                let mut enc = ctx.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("ftcs_slab_download"),
                });
                enc.copy_buffer_to_buffer(result_buf, 0, &self.staging_buf, 0, buf_bytes);
                ctx.queue().submit(Some(enc.finish()));
                let slab_result = Self::read_staging_flat(ctx, &self.staging_buf, buf_bytes);
                Self::scatter_interior(
                    &slab_result, host_flat, nx, ny, nz,
                    buf_nz, halo_bottom, slab_start, interior_nz,
                );
            }
        }
    }

    fn scatter_interior(
        slab: &[f32],
        target: &mut [f32],
        nx: u32, ny: u32, nz: u32,
        buf_nz: u32, halo_bottom: u32,
        dst_iz_start: u32, interior_nz: u32,
    ) {
        let nx = nx as usize;
        let ny = ny as usize;
        let nz = nz as usize;
        let buf_nz = buf_nz as usize;
        let halo = halo_bottom as usize;
        let dst_start = dst_iz_start as usize;
        let inz = interior_nz as usize;
        for ix in 0..nx {
            for iy in 0..ny {
                let src = ix * ny * buf_nz + iy * buf_nz + halo;
                let dst = ix * ny * nz + iy * nz + dst_start;
                target[dst..dst + inz].copy_from_slice(&slab[src..src + inz]);
            }
        }
    }

    fn gather_thermal_slab(
        field: &ThermalField,
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) -> Vec<f32> {
        let flat: Vec<f32> = field.as_array_view().iter().copied().collect();
        Self::gather_from_flat(&flat, nx, ny, nz, slab_iz_start, this_slab_nz)
    }

    fn gather_from_flat(
        flat: &[f32],
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) -> Vec<f32> {
        let nx = nx as usize;
        let ny = ny as usize;
        let nz = nz as usize;
        let start = slab_iz_start as usize;
        let snz = this_slab_nz as usize;
        let mut slab = vec![0.0f32; nx * ny * snz];
        for ix in 0..nx {
            for iy in 0..ny {
                let src = ix * ny * nz + iy * nz + start;
                let dst = ix * ny * snz + iy * snz;
                slab[dst..dst + snz].copy_from_slice(&flat[src..src + snz]);
            }
        }
        slab
    }

    fn scatter_thermal_slab(
        slab: &[f32],
        target: &mut [f32],
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) {
        Self::scatter_to_flat(slab, target, nx, ny, nz, slab_iz_start, this_slab_nz);
    }

    fn scatter_to_flat(
        slab: &[f32],
        target: &mut [f32],
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) {
        let nx = nx as usize;
        let ny = ny as usize;
        let nz = nz as usize;
        let start = slab_iz_start as usize;
        let snz = this_slab_nz as usize;
        for ix in 0..nx {
            for iy in 0..ny {
                let dst = ix * ny * nz + iy * nz + start;
                let src = ix * ny * snz + iy * snz;
                target[dst..dst + snz].copy_from_slice(&slab[src..src + snz]);
            }
        }
    }

    fn read_staging_flat(
        ctx: &GpuContext,
        staging: &wgpu::Buffer,
        byte_size: u64,
    ) -> Vec<f32> {
        let slice = staging.slice(..byte_size);
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
        let result = gpu_data.to_vec();
        drop(mapped);
        staging.unmap();
        result
    }
}
