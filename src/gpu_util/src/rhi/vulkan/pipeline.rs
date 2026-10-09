// SPIR-Vからコンピュートパイプラインを作る。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ash::vk;

use super::descriptor::{DescriptorBindingDesc, DescriptorSetLayout};
use super::device::VulkanDevice;
use crate::rhi::{BindingDesc, BindingKind, PipelineLayoutDesc, ShaderStages};

pub struct ComputePipeline {
    device: Arc<VulkanDevice>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    /// layout_descのセット順に並んだディスクリプタセットレイアウト。
    pub(super) set_layouts: Vec<Arc<DescriptorSetLayout>>,
    layout_desc: PipelineLayoutDesc,
}

impl ComputePipeline {
    pub(crate) fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub(crate) fn pipeline_layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    pub fn layout_desc(&self) -> &PipelineLayoutDesc {
        &self.layout_desc
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

fn to_vk_descriptor_type(kind: BindingKind) -> vk::DescriptorType {
    match kind {
        BindingKind::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        BindingKind::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        BindingKind::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        BindingKind::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        BindingKind::Sampler => vk::DescriptorType::SAMPLER,
    }
}

fn to_vk_stages(stages: ShaderStages) -> vk::ShaderStageFlags {
    let mut flags = vk::ShaderStageFlags::empty();
    if stages.contains(ShaderStages::COMPUTE) {
        flags |= vk::ShaderStageFlags::COMPUTE;
    }
    if stages.contains(ShaderStages::VERTEX) {
        flags |= vk::ShaderStageFlags::VERTEX;
    }
    if stages.contains(ShaderStages::FRAGMENT) {
        flags |= vk::ShaderStageFlags::FRAGMENT;
    }
    flags
}

impl VulkanDevice {
    fn to_descriptor_binding_desc(&self, b: &BindingDesc) -> Result<DescriptorBindingDesc> {
        if b.variable_count && b.kind != BindingKind::SampledImage {
            bail!(
                "binding {}: variable_count is only supported for SampledImage bindings",
                b.binding
            );
        }
        Ok(DescriptorBindingDesc {
            binding: b.binding,
            descriptor_type: to_vk_descriptor_type(b.kind),
            // 可変長はデバイスの上限を最大本数とする。
            descriptor_count: if b.variable_count {
                self.max_variable_sampled_image_count()
            } else {
                b.count
            },
            stage_flags: to_vk_stages(b.stages),
            variable_count: b.variable_count,
        })
    }

    /// レイアウト記述からディスクリプタセットレイアウト群とVkPipelineLayoutを作る
    /// (compute / graphics共通)。
    pub(super) fn create_pipeline_layout_from_desc(
        self: &Arc<Self>,
        layout_desc: &PipelineLayoutDesc,
    ) -> Result<(Vec<Arc<DescriptorSetLayout>>, vk::PipelineLayout)> {
        let set_layouts = layout_desc
            .sets
            .iter()
            .map(|set| {
                let bindings = set
                    .bindings
                    .iter()
                    .map(|b| self.to_descriptor_binding_desc(b))
                    .collect::<Result<Vec<_>>>()?;
                self.create_descriptor_set_layout(&bindings)
            })
            .collect::<Result<Vec<_>>>()?;
        let handles: Vec<vk::DescriptorSetLayout> =
            set_layouts.iter().map(|l| l.handle()).collect();

        let create_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&handles);
        let layout = unsafe { self.device.create_pipeline_layout(&create_info, None) }
            .context("Failed to create Vulkan pipeline layout")?;
        Ok((set_layouts, layout))
    }

    pub(super) fn create_shader_module(&self, spirv: &[u32]) -> Result<vk::ShaderModule> {
        let create_info = vk::ShaderModuleCreateInfo::default().code(spirv);
        unsafe { self.device.create_shader_module(&create_info, None) }
            .context("Failed to create Vulkan shader module from SPIR-V")
    }

    pub fn create_compute_pipeline(
        self: &Arc<Self>,
        spirv: &[u32],
        entry_point: &str,
        layout_desc: &PipelineLayoutDesc,
    ) -> Result<Arc<ComputePipeline>> {
        let (set_layouts, layout) = self.create_pipeline_layout_from_desc(layout_desc)?;

        let shader_module = match self.create_shader_module(spirv) {
            Ok(m) => m,
            Err(e) => {
                unsafe { self.device.destroy_pipeline_layout(layout, None) };
                return Err(e);
            }
        };
        let entry_point_c = std::ffi::CString::new(entry_point)?;

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
            layout_desc: layout_desc.clone(),
        }))
    }
}
