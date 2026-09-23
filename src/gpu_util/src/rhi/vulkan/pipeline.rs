// SPIR-Vワード列から直接コンピュートパイプラインを作る

use anyhow::{Context, Result};
use ash::vk;

use super::device::VulkanDevice;

impl VulkanDevice {
    /// spirv(コンピュートシェーダー1本分)からvk::Pipelineを作る。
    /// set_layoutsはディスクリプタセットレイアウトの並び(セット番号順)。
    pub fn create_compute_pipeline_from_spirv(
        &self,
        spirv: &[u32],
        entry_point: &str,
        set_layouts: &[vk::DescriptorSetLayout],
    ) -> Result<(vk::Pipeline, vk::PipelineLayout)> {
        let shader_module = self.create_shader_module(spirv)?;

        let entry_point_c = std::ffi::CString::new(entry_point)?;

        let layout_create_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(set_layouts);
        let layout = unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        }
        .context("Failed to create Vulkan pipeline layout");

        let layout = match layout {
            Ok(l) => l,
            Err(e) => {
                unsafe { self.device.destroy_shader_module(shader_module, None) };
                return Err(e);
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

        // シェーダーモジュールはパイプライン作成後は不要(Vulkan仕様上、作成に使ったモジュールをすぐ破棄してよい)。
        unsafe { self.device.destroy_shader_module(shader_module, None) };

        match result {
            Ok(pipelines) => Ok((pipelines[0], layout)),
            Err((_, e)) => {
                unsafe { self.device.destroy_pipeline_layout(layout, None) };
                Err(e).context("Failed to create Vulkan compute pipeline")
            }
        }
    }

    fn create_shader_module(&self, spirv: &[u32]) -> Result<vk::ShaderModule> {
        let create_info = vk::ShaderModuleCreateInfo::default().code(spirv);
        unsafe { self.device.create_shader_module(&create_info, None) }
            .context("Failed to create Vulkan shader module from SPIR-V")
    }

    pub fn destroy_pipeline(&self, pipeline: vk::Pipeline, layout: vk::PipelineLayout) {
        unsafe {
            self.device.destroy_pipeline(pipeline, None);
            self.device.destroy_pipeline_layout(layout, None);
        }
    }
}
