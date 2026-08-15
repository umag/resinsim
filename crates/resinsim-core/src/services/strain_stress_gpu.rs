//! ADR-0025 / t2f5 Stage D: GPU-accelerated shrinkage strain + stress.
//!
//! Single WGSL compute shader computes per-voxel: cure dose → Beer-Lambert
//! cure_extent → KB-164 anisotropic free_shrinkage strain → 6x6 isotropic
//! Voigt stiffness → stress tensor. One-shot dispatch (not iterative like
//! thermal FTCS).
//!
//! GPU/CPU parity is tolerance-based per ADR-0025 §Decision v.

#![cfg(feature = "gpu")]

use wgpu::util::DeviceExt;

use crate::services::gpu_context::GpuContext;
use crate::values::{CureField, StrainTensor};

const WGSL_SHADER: &str = r#"
struct Params {
    nx: u32,
    ny: u32,
    nz: u32,
    layer_z: u32,
    ec_at_temp: f32,
    dp_um: f32,
    layer_height_um: f32,
    linear_shrinkage_frac: f32,
    z_anisotropy_ratio: f32,
    youngs_modulus_mpa: f32,
    poissons_ratio: f32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> dose: array<f32>;
@group(0) @binding(2) var<storage, read_write> strain_out: array<f32>;
@group(0) @binding(3) var<storage, read_write> stress_out: array<f32>;

fn dose_idx(ix: u32, iy: u32, iz: u32) -> u32 {
    return ix * params.ny * params.nz + iy * params.nz + iz;
}

fn out_idx(ix: u32, iy: u32) -> u32 {
    return ix * params.ny + iy;
}

@compute @workgroup_size(64)
fn strain_stress_step(@builtin(global_invocation_id) gid: vec3<u32>) {
    let total_xy = params.nx * params.ny;
    let wg_x = min((total_xy + 63u) / 64u, 65535u);
    let linear = gid.y * wg_x * 64u + gid.x;
    if linear >= total_xy {
        return;
    }
    let iy = linear % params.ny;
    let ix = linear / params.ny;

    let absorbed_dose = dose[dose_idx(ix, iy, params.layer_z)];

    let base = out_idx(ix, iy) * 6u;

    // Undercured voxel: dose <= Ec(T) → zero strain and stress
    if absorbed_dose <= params.ec_at_temp || params.ec_at_temp <= 0.0 || params.dp_um <= 0.0 || params.layer_height_um <= 0.0 {
        for (var c = 0u; c < 6u; c = c + 1u) {
            strain_out[base + c] = 0.0;
            stress_out[base + c] = 0.0;
        }
        return;
    }

    // Beer-Lambert: cure_depth = Dp × ln(E / Ec)
    let cure_depth_um = params.dp_um * log(absorbed_dose / params.ec_at_temp);
    var cure_extent = cure_depth_um / params.layer_height_um;
    cure_extent = clamp(cure_extent, 0.0, 1.0);

    // KB-164 anisotropic free shrinkage strain
    let eps_iso = -params.linear_shrinkage_frac * cure_extent;
    let r = params.z_anisotropy_ratio;
    let factor_xy = 3.0 / (2.0 + r);
    let factor_z = r * factor_xy;
    let eps_xy = factor_xy * eps_iso;
    let eps_z = factor_z * eps_iso;

    // Strain output: [eps_xx, eps_yy, eps_zz, eps_yz, eps_xz, eps_xy]
    strain_out[base + 0u] = eps_xy;
    strain_out[base + 1u] = eps_xy;
    strain_out[base + 2u] = eps_z;
    strain_out[base + 3u] = 0.0;
    strain_out[base + 4u] = 0.0;
    strain_out[base + 5u] = 0.0;

    // Voigt isotropic stiffness: σ = D : ε
    let nu = params.poissons_ratio;
    let e = params.youngs_modulus_mpa;
    let denom = (1.0 + nu) * (1.0 - 2.0 * nu);
    let d_diag = e * (1.0 - nu) / denom;
    let d_off = e * nu / denom;

    let s_xx = d_diag * eps_xy + d_off * (eps_xy + eps_z);
    let s_yy = d_diag * eps_xy + d_off * (eps_xy + eps_z);
    let s_zz = d_diag * eps_z + d_off * (eps_xy + eps_xy);

    stress_out[base + 0u] = s_xx;
    stress_out[base + 1u] = s_yy;
    stress_out[base + 2u] = s_zz;
    stress_out[base + 3u] = 0.0;
    stress_out[base + 4u] = 0.0;
    stress_out[base + 5u] = 0.0;
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuStrainStressParams {
    nx: u32,
    ny: u32,
    nz: u32,
    layer_z: u32,
    ec_at_temp: f32,
    dp_um: f32,
    layer_height_um: f32,
    linear_shrinkage_frac: f32,
    z_anisotropy_ratio: f32,
    youngs_modulus_mpa: f32,
    poissons_ratio: f32,
    _pad: u32,
}

pub struct GpuStrainStressBuffers {
    dose_buf: wgpu::Buffer,
    strain_buf: wgpu::Buffer,
    stress_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    strain_staging: wgpu::Buffer,
    stress_staging: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    nx: u32,
    ny: u32,
}

impl GpuStrainStressBuffers {
    pub fn new(ctx: &GpuContext, cure_field: &CureField) -> Self {
        let (nx, ny, _nz) = cure_field.dimensions();
        let total_layer = (nx as usize) * (ny as usize);
        let dose_data: Vec<f32> = cure_field.data().iter().copied().collect();

        let device = ctx.device();

        let dose_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("strain_dose"),
            contents: bytemuck::cast_slice(&dose_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let strain_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strain_out"),
            size: (total_layer * 6 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let stress_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stress_out"),
            size: (total_layer * 6 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strain_stress_params"),
            size: std::mem::size_of::<GpuStrainStressParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let strain_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strain_staging"),
            size: (total_layer * 6 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stress_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stress_staging"),
            size: (total_layer * 6 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strain_stress_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strain_stress_layout"),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
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
            label: Some("strain_stress_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("strain_stress_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("strain_stress_step"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strain_stress_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dose_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: strain_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: stress_buf.as_entire_binding(),
                },
            ],
        });

        Self {
            dose_buf,
            strain_buf,
            stress_buf,
            params_buf,
            strain_staging,
            stress_staging,
            pipeline,
            bind_group,
            nx,
            ny,
        }
    }

    pub fn upload_dose(&self, ctx: &GpuContext, cure_field: &CureField) {
        let dose_data: Vec<f32> = cure_field.data().iter().copied().collect();
        ctx.queue()
            .write_buffer(&self.dose_buf, 0, bytemuck::cast_slice(&dose_data));
    }

    /// Record a GPU-side copy from an external cure buffer into this
    /// struct's `dose_buf`. Replaces the CPU round-trip of
    /// `upload_dose` when cure and strain run in the same encoder.
    pub fn encode_copy_dose_from(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cure_buf: &wgpu::Buffer,
    ) {
        debug_assert_eq!(
            cure_buf.size(),
            self.dose_buf.size(),
            "cure_buf and dose_buf must have matching byte sizes"
        );
        let size = cure_buf.size().min(self.dose_buf.size());
        encoder.copy_buffer_to_buffer(cure_buf, 0, &self.dose_buf, 0, size);
    }

    /// Record the strain/stress compute pass onto an external
    /// `CommandEncoder` without submitting. The dose data must already
    /// be in `dose_buf` (via `upload_dose` or `encode_copy_dose_from`).
    #[allow(clippy::too_many_arguments)]
    pub fn encode_strain_stress_pass(
        &self,
        ctx: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        layer_z: u32,
        ec_at_temp: f32,
        dp_um: f32,
        layer_height_um: f32,
        linear_shrinkage_frac: f32,
        z_anisotropy_ratio: f32,
        youngs_modulus_mpa: f32,
        poissons_ratio: f32,
        nz: u32,
    ) {
        let params = GpuStrainStressParams {
            nx: self.nx,
            ny: self.ny,
            nz,
            layer_z,
            ec_at_temp,
            dp_um,
            layer_height_um,
            linear_shrinkage_frac,
            z_anisotropy_ratio,
            youngs_modulus_mpa,
            poissons_ratio,
            _pad: 0,
        };
        ctx.queue()
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));

        let total_xy = self.nx * self.ny;
        let total_workgroups = total_xy.div_ceil(64);
        let wg_x = total_workgroups.min(65535);
        let wg_y = total_workgroups.div_ceil(wg_x);

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("strain_stress_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        ctx: &GpuContext,
        layer_z: u32,
        ec_at_temp: f32,
        dp_um: f32,
        layer_height_um: f32,
        linear_shrinkage_frac: f32,
        z_anisotropy_ratio: f32,
        youngs_modulus_mpa: f32,
        poissons_ratio: f32,
        nz: u32,
    ) {
        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strain_stress_encoder"),
            });
        self.encode_strain_stress_pass(
            ctx, &mut encoder, layer_z, ec_at_temp, dp_um, layer_height_um,
            linear_shrinkage_frac, z_anisotropy_ratio, youngs_modulus_mpa,
            poissons_ratio, nz,
        );
        ctx.queue().submit(Some(encoder.finish()));
    }

    pub fn download_strain(&self, ctx: &GpuContext) -> Vec<StrainTensor> {
        let total = (self.nx as usize) * (self.ny as usize);
        let byte_size = (total * 6 * std::mem::size_of::<f32>()) as u64;

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strain_download_encoder"),
            });
        encoder.copy_buffer_to_buffer(&self.strain_buf, 0, &self.strain_staging, 0, byte_size);
        ctx.queue().submit(Some(encoder.finish()));

        let slice = self.strain_staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        ctx.device().poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("strain map_async channel closed")
            .expect("strain map_async failed");

        let mapped = slice.get_mapped_range();
        let gpu_data: &[f32] = bytemuck::cast_slice(&mapped);
        let mut tensors = Vec::with_capacity(total);
        for i in 0..total {
            let base = i * 6;
            let t = StrainTensor::new(
                gpu_data[base],
                gpu_data[base + 1],
                gpu_data[base + 2],
                gpu_data[base + 3],
                gpu_data[base + 4],
                gpu_data[base + 5],
            )
            .unwrap_or_else(|_| StrainTensor::zero());
            tensors.push(t);
        }
        drop(mapped);
        self.strain_staging.unmap();
        tensors
    }

    pub fn download_stress(&self, ctx: &GpuContext) -> Vec<[f32; 6]> {
        let total = (self.nx as usize) * (self.ny as usize);
        let byte_size = (total * 6 * std::mem::size_of::<f32>()) as u64;

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stress_download_encoder"),
            });
        encoder.copy_buffer_to_buffer(&self.stress_buf, 0, &self.stress_staging, 0, byte_size);
        ctx.queue().submit(Some(encoder.finish()));

        let slice = self.stress_staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        ctx.device().poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("stress map_async channel closed")
            .expect("stress map_async failed");

        let mapped = slice.get_mapped_range();
        let gpu_data: &[f32] = bytemuck::cast_slice(&mapped);
        let mut components = Vec::with_capacity(total);
        for i in 0..total {
            let base = i * 6;
            components.push([
                gpu_data[base],
                gpu_data[base + 1],
                gpu_data[base + 2],
                gpu_data[base + 3],
                gpu_data[base + 4],
                gpu_data[base + 5],
            ]);
        }
        drop(mapped);
        self.stress_staging.unmap();
        components
    }
}
