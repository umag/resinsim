//! ADR-0025 / t2f5 Stage C: GPU-accelerated XY light crosstalk convolution.
//!
//! Implements the same separable 2D Gaussian convolution as
//! [`LightCrosstalkCalculator::apply_separable_2d`] (CPU, ADR-0018) but
//! dispatches via two WGSL compute shaders on the GPU: an X-pass (convolve
//! along axis 0) and a Y-pass (convolve along axis 1). Each pass reads
//! from one storage buffer and writes to another; kernel weights live in
//! a uniform buffer.
//!
//! Z convolution stays on CPU — dose columns are computed per-pixel after
//! the sequential Beer-Lambert column march, and batching them for GPU
//! dispatch is memory-prohibitive (nx × ny × nz floats).
//!
//! GPU/CPU parity is tolerance-based (ADR-0025 §Decision v): max
//! per-pixel absolute difference < 1e-3. Results are NOT byte-identical.

#![cfg(feature = "gpu")]

use ndarray::Array2;
use wgpu::util::DeviceExt;

use crate::services::gpu_context::GpuContext;

const WGSL_SHADER: &str = r#"
struct Params {
    nx: u32,
    ny: u32,
    kernel_len: u32,
    kernel_radius: i32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> kernel: array<f32>;

// ndarray row-major (C-order): strides = (ny, 1)
fn idx(ix: u32, iy: u32) -> u32 {
    return ix * params.ny + iy;
}

@compute @workgroup_size(64)
fn conv_x(@builtin(global_invocation_id) gid: vec3<u32>) {
    let wg_x = min((params.nx * params.ny + 63u) / 64u, 65535u);
    let linear = gid.y * wg_x * 64u + gid.x;
    let total = params.nx * params.ny;
    if linear >= total {
        return;
    }
    let iy = linear % params.ny;
    let ix = linear / params.ny;

    var acc: f32 = 0.0;
    for (var k: u32 = 0u; k < params.kernel_len; k = k + 1u) {
        let src_ix = i32(ix) + i32(k) - params.kernel_radius;
        if src_ix >= 0 && src_ix < i32(params.nx) {
            acc = acc + kernel[k] * src[idx(u32(src_ix), iy)];
        }
    }
    dst[linear] = acc;
}

@compute @workgroup_size(64)
fn conv_y(@builtin(global_invocation_id) gid: vec3<u32>) {
    let wg_x = min((params.nx * params.ny + 63u) / 64u, 65535u);
    let linear = gid.y * wg_x * 64u + gid.x;
    let total = params.nx * params.ny;
    if linear >= total {
        return;
    }
    let iy = linear % params.ny;
    let ix = linear / params.ny;

    var acc: f32 = 0.0;
    for (var k: u32 = 0u; k < params.kernel_len; k = k + 1u) {
        let src_iy = i32(iy) + i32(k) - params.kernel_radius;
        if src_iy >= 0 && src_iy < i32(params.ny) {
            acc = acc + kernel[k] * src[idx(ix, u32(src_iy))];
        }
    }
    dst[linear] = acc;
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    nx: u32,
    ny: u32,
    kernel_len: u32,
    kernel_radius: i32,
}

pub struct GpuCrosstalkBuffers {
    buf_a: wgpu::Buffer,
    #[allow(dead_code)] // holds ownership; bind groups reference the underlying GPU resource
    buf_b: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    kernel_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    pipeline_x: wgpu::ComputePipeline,
    pipeline_y: wgpu::ComputePipeline,
    bind_group_a_to_b: wgpu::BindGroup,
    bind_group_b_to_a: wgpu::BindGroup,
    total_pixels: u32,
    nx: u32,
    ny: u32,
}

impl GpuCrosstalkBuffers {
    pub fn new(ctx: &GpuContext, nx: u32, ny: u32) -> Self {
        let total = (nx * ny) as usize;
        let buf_size = (total * std::mem::size_of::<f32>()) as u64;
        let device = ctx.device();

        let buf_a = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosstalk_a"),
            size: buf_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosstalk_b"),
            size: buf_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosstalk_params"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Kernel buffer: sized to the maximum plausible kernel length.
        // Actual kernel data is written per-dispatch via queue.write_buffer.
        const MAX_KERNEL_LEN: usize = 64;
        let kernel_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosstalk_kernel"),
            size: (MAX_KERNEL_LEN * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosstalk_staging"),
            size: buf_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crosstalk_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("crosstalk_layout"),
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
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crosstalk_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline_x = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("crosstalk_pipeline_x"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("conv_x"),
            compilation_options: Default::default(),
            cache: None,
        });
        let pipeline_y = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("crosstalk_pipeline_y"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("conv_y"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_a_to_b = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crosstalk_a_to_b"),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: kernel_buf.as_entire_binding(),
                },
            ],
        });

        let bind_group_b_to_a = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crosstalk_b_to_a"),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: kernel_buf.as_entire_binding(),
                },
            ],
        });

        Self {
            buf_a,
            buf_b,
            params_buf,
            kernel_buf,
            staging_buf,
            pipeline_x,
            pipeline_y,
            bind_group_a_to_b,
            bind_group_b_to_a,
            total_pixels: total as u32,
            nx,
            ny,
        }
    }

    /// Upload intensity grid, encode X-pass and Y-pass, copy result to
    /// staging, and submit — non-blocking. Call `finish_download` to
    /// retrieve the result after the GPU finishes.
    pub fn begin_dispatch(
        &mut self,
        ctx: &GpuContext,
        intensity: &Array2<f32>,
        kernel: &[f32],
    ) {
        let (nx, ny) = intensity.dim();
        debug_assert_eq!(nx as u32, self.nx);
        debug_assert_eq!(ny as u32, self.ny);

        let radius = (kernel.len() as i32 - 1) / 2;
        let params = GpuParams {
            nx: nx as u32,
            ny: ny as u32,
            kernel_len: kernel.len() as u32,
            kernel_radius: radius,
        };

        let data: Vec<f32> = intensity.iter().copied().collect();
        ctx.queue()
            .write_buffer(&self.buf_a, 0, bytemuck::cast_slice(&data));
        ctx.queue()
            .write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        ctx.queue()
            .write_buffer(&self.kernel_buf, 0, bytemuck::cast_slice(kernel));

        let total_workgroups = self.total_pixels.div_ceil(64);
        let wg_x = total_workgroups.min(65535);
        let wg_y = total_workgroups.div_ceil(wg_x);

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("crosstalk_encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("conv_x_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline_x);
            pass.set_bind_group(0, &self.bind_group_a_to_b, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("conv_y_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline_y);
            pass.set_bind_group(0, &self.bind_group_b_to_a, &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        encoder.copy_buffer_to_buffer(
            &self.buf_a,
            0,
            &self.staging_buf,
            0,
            (self.total_pixels as usize * std::mem::size_of::<f32>()) as u64,
        );
        ctx.queue().submit(Some(encoder.finish()));
    }

    /// Block until the previously dispatched GPU work completes, then
    /// download the convolved intensity grid from the staging buffer.
    pub fn finish_download(&mut self, ctx: &GpuContext) -> Array2<f32> {
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
        let mut result = Array2::<f32>::zeros((self.nx as usize, self.ny as usize));
        for (dst, &src_val) in result.iter_mut().zip(gpu_data.iter()) {
            *dst = src_val;
        }
        drop(mapped);
        self.staging_buf.unmap();
        result
    }

    /// Upload intensity grid, run GPU X-pass then Y-pass, download result
    /// back into the grid. The grid is modified in-place to match the CPU
    /// `apply_separable_2d` contract.
    pub fn apply_separable_2d(
        &mut self,
        ctx: &GpuContext,
        intensity: &mut Array2<f32>,
        kernel: &[f32],
    ) {
        self.begin_dispatch(ctx, intensity, kernel);
        let result = self.finish_download(ctx);
        for (dst, &src_val) in intensity.iter_mut().zip(result.iter()) {
            *dst = src_val;
        }
    }
}
