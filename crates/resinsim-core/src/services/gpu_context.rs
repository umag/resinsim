//! ADR-0025 / t2f5: GPU context for compute-shader dispatch.
//!
//! Owns the wgpu `Device`, `Queue`, and adapter metadata. Constructed
//! via `GpuContext::try_new()` which returns `None` when no suitable
//! adapter is available (headless CI, no GPU). The caller (CLI or
//! test harness) decides whether to fall back to CPU.

#![cfg(feature = "gpu")]

use wgpu::{Device, Queue};

pub struct GpuContext {
    device: Device,
    queue: Queue,
    adapter_name: String,
    max_buffer_size: u64,
}

impl GpuContext {
    /// Attempt to create a GPU context. Returns `None` when no adapter
    /// is available (headless, no GPU, unsupported backend). Uses
    /// `pollster::block_on` to bridge wgpu's async API.
    pub fn try_new() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        ))?;
        let adapter_name = adapter.get_info().name.clone();
        let max_buffer_size = adapter.limits().max_buffer_size;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("resinsim-thermal"),
                required_limits: adapter.limits(),
                ..Default::default()
            },
            None,
        ))
        .ok()?;
        Some(Self {
            device,
            queue,
            adapter_name,
            max_buffer_size,
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn max_buffer_size(&self) -> u64 {
        self.max_buffer_size
    }
}
