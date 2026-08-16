//! ADR-0025 / t2f5 Stage B: GPU-accelerated Beer-Lambert cure + PI depletion.
//!
//! Each (ix, iy) thread marches its Z column independently — no cross-thread
//! data dependency, no ping-pong needed. Cure dose is accumulated and PI
//! concentration depleted in-place on the GPU. The host downloads the
//! updated fields after each layer so downstream passes (shrinkage, stress)
//! can read them.
//!
//! GPU/CPU parity is tolerance-based (ADR-0025 §Decision v): max per-voxel
//! absolute difference < 1e-3 mJ/cm² (dose) and < 1e-5 (PI concentration).

#![cfg(feature = "gpu")]

use wgpu::util::DeviceExt;

use crate::services::gpu_context::GpuContext;
use crate::values::{CureField, PhotoinitiatorField};

const WGSL_SHADER: &str = r#"
struct Params {
    nx: u32,
    ny: u32,
    nz: u32,
    iz_top: u32,
    exposure_sec: f32,
    dp_base: f32,
    k_d: f32,
    layer_height_um: f32,
    c_threshold: f32,
    dp_max_factor: f32,
    negligible_dose_floor: f32,
    slab_iz_start: u32,
    slab_nz: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> cure: array<f32>;
@group(0) @binding(2) var<storage, read_write> pi: array<f32>;
@group(0) @binding(3) var<storage, read> intensity: array<f32>;

fn idx3(ix: u32, iy: u32, iz_local: u32) -> u32 {
    return ix * params.ny * params.slab_nz + iy * params.slab_nz + iz_local;
}

@compute @workgroup_size(8, 8)
fn cure_column_march(@builtin(global_invocation_id) gid: vec3<u32>) {
    let ix = gid.x;
    let iy = gid.y;
    if ix >= params.nx || iy >= params.ny {
        return;
    }

    let pixel_intensity = intensity[iy * params.nx + ix];
    let surface_dose = pixel_intensity * params.exposure_sec;
    if surface_dose <= 0.0 {
        return;
    }

    let slab_iz_end = params.slab_iz_start + params.slab_nz;
    let loop_start = max(params.iz_top, params.slab_iz_start);
    let loop_end = min(params.nz, slab_iz_end);
    if loop_start >= loop_end {
        return;
    }

    for (var iz_global = loop_start; iz_global < loop_end; iz_global++) {
        let iz_local = iz_global - params.slab_iz_start;
        let depth_um = f32(iz_global - params.iz_top) * params.layer_height_um
                       + params.layer_height_um * 0.5;
        let linear = idx3(ix, iy, iz_local);
        let c_local = pi[linear];
        let c_clamped = max(c_local, params.c_threshold);
        let dp_local = min(params.dp_base / c_clamped,
                           params.dp_base * params.dp_max_factor);
        let attenuation = exp(-depth_um / dp_local);
        let voxel_dose = surface_dose * attenuation;
        if voxel_dose <= params.negligible_dose_floor {
            break;
        }

        // Accumulate cure dose.
        cure[linear] += voxel_dose;

        // KB-160 PI depletion: C_new = C_old * clamp(exp(-k_d * dose), 0, 1).
        // Second clamp: C_after <= C_before (no recombination).
        let multiplier = clamp(exp(-params.k_d * voxel_dose), 0.0, 1.0);
        pi[linear] = clamp(c_local * multiplier, 0.0, c_local);
    }
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuCureParams {
    nx: u32,
    ny: u32,
    nz: u32,
    iz_top: u32,
    exposure_sec: f32,
    dp_base: f32,
    k_d: f32,
    layer_height_um: f32,
    c_threshold: f32,
    dp_max_factor: f32,
    negligible_dose_floor: f32,
    slab_iz_start: u32,
    slab_nz: u32,
    _pad: [u32; 3],
}

pub struct GpuCureBuffers {
    cure_buf: wgpu::Buffer,
    pi_buf: wgpu::Buffer,
    intensity_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    staging_cure: wgpu::Buffer,
    staging_pi: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    nx: u32,
    ny: u32,
    nz: u32,
    slab_nz: u32,
}

fn compute_slab_nz(nx: u32, ny: u32, nz: u32, max_buffer_size: u64) -> u32 {
    let xy_bytes = (nx as u64) * (ny as u64) * std::mem::size_of::<f32>() as u64;
    if xy_bytes == 0 || xy_bytes > max_buffer_size {
        return 0;
    }
    let max_nz = max_buffer_size / xy_bytes;
    (max_nz as u32).min(nz).max(1)
}

impl GpuCureBuffers {
    pub fn new(
        ctx: &GpuContext,
        cure_field: &CureField,
        pi_field: &PhotoinitiatorField,
    ) -> Option<Self> {
        let (nx, ny, nz) = cure_field.dimensions();
        let slab_nz = compute_slab_nz(nx, ny, nz, ctx.max_buffer_size());
        if slab_nz == 0 {
            return None;
        }

        let slab_voxels = (nx as usize) * (ny as usize) * (slab_nz as usize);
        let slab_bytes = (slab_voxels * std::mem::size_of::<f32>()) as u64;
        let xy_count = (nx as usize) * (ny as usize);

        let device = ctx.device();

        let cure_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cure"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pi_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pi"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let intensity_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("intensity"),
            size: (xy_count * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cure_params"),
            size: std::mem::size_of::<GpuCureParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_cure = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_cure"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_pi = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_pi"),
            size: slab_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cure_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cure_layout"),
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
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cure_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cure_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cure_column_march"),
            compilation_options: Default::default(),
            cache: None,
        });

        Some(Self {
            cure_buf,
            pi_buf,
            intensity_buf,
            params_buf,
            staging_cure,
            staging_pi,
            pipeline,
            bind_group_layout,
            nx,
            ny,
            nz,
            slab_nz,
        })
    }

    pub fn slab_nz(&self) -> u32 {
        self.slab_nz
    }

    pub fn nz(&self) -> u32 {
        self.nz
    }

    /// Upload per-pixel intensity grid and dispatch the cure column march
    /// across all Z-slabs. Each slab's cure+PI data is uploaded from the
    /// host arrays, dispatched, and downloaded back.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        ctx: &GpuContext,
        intensity_grid: &[f32],
        iz_top: u32,
        nz: u32,
        exposure_sec: f32,
        dp_base: f32,
        k_d: f32,
        layer_height_um: f32,
        cure_field: &mut CureField,
        pi_field: &mut PhotoinitiatorField,
    ) {
        self.write_intensity(ctx, intensity_grid);
        for slab_iz_start in (0..nz).step_by(self.slab_nz as usize) {
            let this_slab_nz = self.slab_nz.min(nz - slab_iz_start);
            self.upload_slab(ctx, cure_field, pi_field, slab_iz_start, this_slab_nz);
            let mut encoder = ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cure_encoder"),
                });
            self.encode_cure_pass(
                ctx, &mut encoder, iz_top, nz, exposure_sec, dp_base, k_d,
                layer_height_um, slab_iz_start, this_slab_nz,
            );
            ctx.queue().submit(Some(encoder.finish()));
            self.download_slab(ctx, cure_field, pi_field, slab_iz_start, this_slab_nz);
        }
    }

    /// Write per-pixel intensity grid to the GPU buffer without
    /// creating or submitting a command encoder.
    pub fn write_intensity(&self, ctx: &GpuContext, intensity_grid: &[f32]) {
        ctx.queue().write_buffer(
            &self.intensity_buf,
            0,
            bytemuck::cast_slice(intensity_grid),
        );
    }

    /// Upload a Z-slab of cure and PI data from host arrays to GPU buffers.
    pub fn upload_slab(
        &self,
        ctx: &GpuContext,
        cure_field: &CureField,
        pi_field: &PhotoinitiatorField,
        slab_iz_start: u32,
        this_slab_nz: u32,
    ) {
        let cure_slab = Self::gather_slab(
            cure_field.data(), self.nx, self.ny, self.nz, slab_iz_start, this_slab_nz,
        );
        let pi_slab = Self::gather_slab(
            pi_field.data(), self.nx, self.ny, self.nz, slab_iz_start, this_slab_nz,
        );
        ctx.queue()
            .write_buffer(&self.cure_buf, 0, bytemuck::cast_slice(&cure_slab));
        ctx.queue()
            .write_buffer(&self.pi_buf, 0, bytemuck::cast_slice(&pi_slab));
    }

    /// Download a Z-slab of cure and PI data from GPU buffers back to host.
    pub fn download_slab(
        &self,
        ctx: &GpuContext,
        cure_field: &mut CureField,
        pi_field: &mut PhotoinitiatorField,
        slab_iz_start: u32,
        this_slab_nz: u32,
    ) {
        let slab_bytes = (self.nx as u64) * (self.ny as u64)
            * (this_slab_nz as u64) * std::mem::size_of::<f32>() as u64;

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cure_download_encoder"),
            });
        encoder.copy_buffer_to_buffer(&self.cure_buf, 0, &self.staging_cure, 0, slab_bytes);
        encoder.copy_buffer_to_buffer(&self.pi_buf, 0, &self.staging_pi, 0, slab_bytes);
        ctx.queue().submit(Some(encoder.finish()));

        let cure_slab = Self::read_staging_flat(ctx, &self.staging_cure, slab_bytes);
        Self::scatter_slab(
            &cure_slab, cure_field.data_mut(),
            self.nx, self.ny, self.nz, slab_iz_start, this_slab_nz,
        );
        let pi_slab = Self::read_staging_flat(ctx, &self.staging_pi, slab_bytes);
        Self::scatter_slab(
            &pi_slab, pi_field.data_mut(),
            self.nx, self.ny, self.nz, slab_iz_start, this_slab_nz,
        );
    }

    /// Record the cure column-march compute pass onto an external
    /// `CommandEncoder` without submitting. Call `write_intensity`
    /// first to stage the intensity data. The caller submits the
    /// encoder after recording all passes (e.g. cure + strain).
    #[allow(clippy::too_many_arguments)]
    pub fn encode_cure_pass(
        &self,
        ctx: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        iz_top: u32,
        nz: u32,
        exposure_sec: f32,
        dp_base: f32,
        k_d: f32,
        layer_height_um: f32,
        slab_iz_start: u32,
        slab_nz: u32,
    ) {
        let params = GpuCureParams {
            nx: self.nx,
            ny: self.ny,
            nz,
            iz_top,
            exposure_sec,
            dp_base,
            k_d,
            layer_height_um,
            c_threshold: 0.01,
            dp_max_factor: 10.0,
            negligible_dose_floor: 1e-6,
            slab_iz_start,
            slab_nz,
            _pad: [0; 3],
        };
        ctx.queue()
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cure_bind"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.cure_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.pi_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.intensity_buf.as_entire_binding(),
                },
            ],
        });

        let wg_x = self.nx.div_ceil(8);
        let wg_y = self.ny.div_ceil(8);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cure_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }
    }

    /// Expose the cure storage buffer for on-GPU copies (e.g.
    /// `GpuStrainStressBuffers::encode_copy_dose_from`).
    pub fn cure_buf(&self) -> &wgpu::Buffer {
        &self.cure_buf
    }

    /// Download the current slab's cure and PI data from GPU to host.
    /// With slab chunking, `dispatch()` handles per-slab upload/download
    /// internally. This method downloads whatever is currently in the GPU
    /// buffer (the last-dispatched slab).
    pub fn download_current_slab(
        &self,
        ctx: &GpuContext,
        cure_field: &mut CureField,
        pi_field: &mut PhotoinitiatorField,
        slab_iz_start: u32,
        this_slab_nz: u32,
    ) {
        self.download_slab(ctx, cure_field, pi_field, slab_iz_start, this_slab_nz);
    }

    fn gather_slab(
        data: &ndarray::Array3<f32>,
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) -> Vec<f32> {
        let nx = nx as usize;
        let ny = ny as usize;
        let nz = nz as usize;
        let slab_iz_start = slab_iz_start as usize;
        let this_slab_nz = this_slab_nz as usize;
        let mut slab = vec![0.0f32; nx * ny * this_slab_nz];
        let flat = data.as_slice().expect("contiguous ndarray");
        for ix in 0..nx {
            for iy in 0..ny {
                let src_base = ix * ny * nz + iy * nz + slab_iz_start;
                let dst_base = ix * ny * this_slab_nz + iy * this_slab_nz;
                slab[dst_base..dst_base + this_slab_nz]
                    .copy_from_slice(&flat[src_base..src_base + this_slab_nz]);
            }
        }
        slab
    }

    fn scatter_slab(
        slab: &[f32],
        target: &mut ndarray::Array3<f32>,
        nx: u32, ny: u32, nz: u32,
        slab_iz_start: u32, this_slab_nz: u32,
    ) {
        let nx = nx as usize;
        let ny = ny as usize;
        let nz = nz as usize;
        let slab_iz_start = slab_iz_start as usize;
        let this_slab_nz = this_slab_nz as usize;
        let flat = target.as_slice_mut().expect("contiguous ndarray");
        for ix in 0..nx {
            for iy in 0..ny {
                let dst_base = ix * ny * nz + iy * nz + slab_iz_start;
                let src_base = ix * ny * this_slab_nz + iy * this_slab_nz;
                flat[dst_base..dst_base + this_slab_nz]
                    .copy_from_slice(&slab[src_base..src_base + this_slab_nz]);
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
