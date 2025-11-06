// compiled_wgsl.rs

use anyhow::Result;
use std::sync::Arc;
use wgpu::Device;

#[derive(Clone)]
pub struct CompiledWgsl {
    pub(crate) id: String, // IDを追加
    pub(crate) module: Arc<wgpu::ShaderModule>,
    pub(crate) _source: Arc<str>,
}

impl CompiledWgsl {
    pub fn new(id: &str, wgsl_code: &str, device: &Device) -> Result<Self> {
        let shader_module_descriptor = wgpu::ShaderModuleDescriptor {
            label: Some(id), // labelにもIDを使用
            source: wgpu::ShaderSource::Wgsl(wgsl_code.into()),
        };

        let module = device.create_shader_module(shader_module_descriptor);

        Ok(Self {
            id: id.to_string(), // IDを保存
            module: Arc::new(module),
            _source: Arc::from(wgsl_code),
        })
    }
}