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
    _pad: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> cure: array<f32>;
@group(0) @binding(2) var<storage, read_write> pi: array<f32>;
@group(0) @binding(3) var<storage, read> intensity: array<f32>;

fn idx3(ix: u32, iy: u32, iz: u32) -> u32 {
    return ix * params.ny * params.nz + iy * params.nz + iz;
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

    for (var iz = params.iz_top; iz < params.nz; iz++) {
        let depth_um = f32(iz - params.iz_top) * params.layer_height_um
                       + params.layer_height_um * 0.5;
        let linear = idx3(ix, iy, iz);
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
    _pad: u32,
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
    total_voxels: u32,
    nx: u32,
    ny: u32,
}

impl GpuCureBuffers {
    pub fn new(
        ctx: &GpuContext,
        cure_field: &CureField,
        pi_field: &PhotoinitiatorField,
    ) -> Self {
        let (nx, ny, nz) = cure_field.dimensions();
        let total = (nx as usize) * (ny as usize) * (nz as usize);
        let total_bytes = (total * std::mem::size_of::<f32>()) as u64;
        let xy_count = (nx as usize) * (ny as usize);

        let cure_data: Vec<f32> = cure_field.data().iter().copied().collect();
        let pi_data: Vec<f32> = pi_field.data().iter().copied().collect();

        let device = ctx.device();

        let cure_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cure"),
            contents: bytemuck::cast_slice(&cure_data),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
        let pi_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pi"),
            contents: bytemuck::cast_slice(&pi_data),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
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
            size: total_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_pi = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_pi"),
            size: total_bytes,
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

        Self {
            cure_buf,
            pi_buf,
            intensity_buf,
            params_buf,
            staging_cure,
            staging_pi,
            pipeline,
            bind_group_layout,
            total_voxels: total as u32,
            nx,
            ny,
        }
    }

    /// Upload per-pixel intensity grid and dispatch the cure column march.
    /// `intensity_grid` is row-major (iy * nx + ix), length nx * ny.
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
            _pad: 0,
        };
        ctx.queue()
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        ctx.queue().write_buffer(
            &self.intensity_buf,
            0,
            bytemuck::cast_slice(intensity_grid),
        );

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

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cure_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cure_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        ctx.queue().submit(Some(encoder.finish()));
    }

    /// Download cure and PI fields from GPU back to host.
    pub fn download(
        &self,
        ctx: &GpuContext,
        cure_field: &mut CureField,
        pi_field: &mut PhotoinitiatorField,
    ) {
        let size = (self.total_voxels as usize * std::mem::size_of::<f32>()) as u64;

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cure_download_encoder"),
            });
        encoder.copy_buffer_to_buffer(&self.cure_buf, 0, &self.staging_cure, 0, size);
        encoder.copy_buffer_to_buffer(&self.pi_buf, 0, &self.staging_pi, 0, size);
        ctx.queue().submit(Some(encoder.finish()));

        Self::read_staging(ctx, &self.staging_cure, cure_field.data_mut());
        Self::read_staging(ctx, &self.staging_pi, pi_field.data_mut());
    }

    fn read_staging(
        ctx: &GpuContext,
        staging: &wgpu::Buffer,
        target: &mut ndarray::Array3<f32>,
    ) {
        let slice = staging.slice(..);
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
        for (dst, &src_val) in target.iter_mut().zip(gpu_data.iter()) {
            *dst = src_val;
        }
        drop(mapped);
        staging.unmap();
    }
}
