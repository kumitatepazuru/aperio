// SPIR-Vからコンピュートパイプラインを作る。

use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;

use super::descriptor::{DescriptorBindingDesc, DescriptorSetLayout};
use super::device::VulkanDevice;
use crate::rhi::{InputArity, OutputKind, PipelineDesc};

const INPUTS_SET: usize = 0;
const RES_SET: usize = 1;

pub struct ComputePipeline {
    device: Arc<VulkanDevice>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    /// [inputs(set 0), res(set 1)]
    pub(super) set_layouts: [Arc<DescriptorSetLayout>; 2],
    desc: PipelineDesc,
}

impl ComputePipeline {
    pub(crate) fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub(crate) fn pipeline_layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    pub fn desc(&self) -> &PipelineDesc {
        &self.desc
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.device.destroy_pipeline(self.pipeline, None);
            self.device
                .device
                .destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn compute_binding(binding: u32, descriptor_type: vk::DescriptorType) -> DescriptorBindingDesc {
    DescriptorBindingDesc {
        binding,
        descriptor_type,
        descriptor_count: 1,
        stage_flags: vk::ShaderStageFlags::COMPUTE,
        variable_count: false,
    }
}

impl VulkanDevice {
    pub fn create_compute_pipeline(
        self: &Arc<Self>,
        spirv: &[u32],
        desc: &PipelineDesc,
    ) -> Result<Arc<ComputePipeline>> {
        let inputs_bindings = match desc.input_arity {
            InputArity::Fixed => (0..desc.input_count)
                .map(|i| compute_binding(i, vk::DescriptorType::SAMPLED_IMAGE))
                .collect::<Vec<_>>(),
            InputArity::Variable => vec![DescriptorBindingDesc {
                // 実機の上限をそのまま使う。実際の使用本数はセット確保時に別途指定する。
                descriptor_count: self.max_variable_sampled_image_count(),
                variable_count: true,
                ..compute_binding(0, vk::DescriptorType::SAMPLED_IMAGE)
            }],
        };

        let output_type = match desc.output {
            OutputKind::Texture => vk::DescriptorType::STORAGE_IMAGE,
            OutputKind::Buffer => vk::DescriptorType::STORAGE_BUFFER,
        };
        let mut res_bindings = vec![compute_binding(0, output_type)];
        if desc.has_sampler {
            res_bindings.push(compute_binding(
                res_bindings.len() as u32,
                vk::DescriptorType::SAMPLER,
            ));
        }
        if desc.has_params {
            res_bindings.push(compute_binding(
                res_bindings.len() as u32,
                vk::DescriptorType::STORAGE_BUFFER,
            ));
        }

        let set_layouts = [
            self.create_descriptor_set_layout(&inputs_bindings)?,
            self.create_descriptor_set_layout(&res_bindings)?,
        ];
        let set_layout_handles = [
            set_layouts[INPUTS_SET].handle(),
            set_layouts[RES_SET].handle(),
        ];

        let shader_module = {
            let create_info = vk::ShaderModuleCreateInfo::default().code(spirv);
            unsafe { self.device.create_shader_module(&create_info, None) }
                .context("Failed to create Vulkan shader module from SPIR-V")?
        };
        let entry_point_c = std::ffi::CString::new(desc.entry_point.as_str())?;

        let layout_create_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layout_handles);
        let layout = match unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        } {
            Ok(l) => l,
            Err(e) => {
                unsafe { self.device.destroy_shader_module(shader_module, None) };
                return Err(e).context("Failed to create Vulkan pipeline layout");
            }
        };

        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(&entry_point_c);
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(layout);

        let result = unsafe {
            self.device
                .create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
        };
        // モジュールはパイプライン作成後すぐ破棄してよい。
        unsafe { self.device.destroy_shader_module(shader_module, None) };

        let pipeline = match result {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                unsafe { self.device.destroy_pipeline_layout(layout, None) };
                return Err(e).context("Failed to create Vulkan compute pipeline");
            }
        };

        Ok(Arc::new(ComputePipeline {
            device: self.clone(),
            pipeline,
            layout,
            set_layouts,
            desc: desc.clone(),
        }))
    }
}
