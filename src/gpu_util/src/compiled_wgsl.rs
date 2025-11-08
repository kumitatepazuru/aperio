// compiled_wgsl.rs

use anyhow::Result;
use std::sync::Arc;
use wgpu::Device;

pub struct SamplerOptions {
    pub address_mode: wgpu::AddressMode,
    pub filter: wgpu::FilterMode,
}

#[derive(Clone)]
pub struct CompiledWgsl {
    pub(crate) id: String,
    pub(crate) module: Arc<wgpu::ShaderModule>,
    pub(crate) sampler: Option<Arc<wgpu::Sampler>>,
    pub(crate) _source: Arc<str>,
}

impl CompiledWgsl {
    pub fn new(id: &str, wgsl_code: &str, device: &Device, sampler_options: Option<&SamplerOptions>) -> Result<Self> {
        let shader_module_descriptor = wgpu::ShaderModuleDescriptor {
            label: Some(id), // labelにもIDを使用
            source: wgpu::ShaderSource::Wgsl(wgsl_code.into()),
        };

        let module = device.create_shader_module(shader_module_descriptor);
        let sampler = sampler_options.map(|options| {
            Arc::new(device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: options.address_mode,
                address_mode_v: options.address_mode,
                address_mode_w: options.address_mode,
                mag_filter: options.filter,
                min_filter: options.filter,
                mipmap_filter: options.filter,
                ..Default::default()
            }))
        });

        Ok(Self {
            id: id.to_string(),
            module: Arc::new(module),
            sampler,
            _source: Arc::from(wgsl_code),
        })
    }
}