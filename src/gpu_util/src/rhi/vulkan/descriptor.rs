// ディスクリプタセットレイアウト/プール/セットの生成。

use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;

use super::device::VulkanDevice;

#[derive(Clone, Copy)]
pub(super) struct DescriptorBindingDesc {
    pub binding: u32,
    pub descriptor_type: vk::DescriptorType,
    /// 固定バインディングでは実際の本数。可変長バインディングではレイアウト上の最大本数
    pub descriptor_count: u32,
    pub stage_flags: vk::ShaderStageFlags,
    pub variable_count: bool,
}

pub(super) struct DescriptorSetLayout {
    device: Arc<VulkanDevice>,
    handle: vk::DescriptorSetLayout,
    /// このレイアウトを構成したバインディング記述。Vulkanには一度作った
    ///　VkDescriptorSetLayoutからバインディング一覧を読み戻すAPIが無いため、
    /// ディスクリプタプールのサイズ算出用にここへ保持しておく。
    bindings: Vec<DescriptorBindingDesc>,
}

impl DescriptorSetLayout {
    pub(super) fn handle(&self) -> vk::DescriptorSetLayout {
        self.handle
    }

    pub(super) fn bindings(&self) -> &[DescriptorBindingDesc] {
        &self.bindings
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device
                .destroy_descriptor_set_layout(self.handle, None)
        };
    }
}

impl VulkanDevice {
    /// 単一のディスクリプタセットレイアウトを生成する(呼び出し側がbindingsを丸ごと1セット分として渡す想定)。
    pub(super) fn create_descriptor_set_layout(
        self: &Arc<Self>,
        bindings: &[DescriptorBindingDesc],
    ) -> Result<Arc<DescriptorSetLayout>> {
        debug_assert!(
            bindings
                .iter()
                .all(|v| { !v.variable_count || bindings.iter().all(|b| b.binding <= v.binding) }),
            "variable-count binding must have the highest binding number in the layout"
        );

        let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = bindings
            .iter()
            .map(|b| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b.binding)
                    .descriptor_type(b.descriptor_type)
                    .descriptor_count(b.descriptor_count)
                    .stage_flags(b.stage_flags)
            })
            .collect();

        let binding_flags: Vec<vk::DescriptorBindingFlags> = bindings
            .iter()
            .map(|b| {
                if b.variable_count {
                    vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                } else {
                    vk::DescriptorBindingFlags::empty()
                }
            })
            .collect();

        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&vk_bindings)
            .push_next(&mut binding_flags_info);

        let handle = unsafe { self.device.create_descriptor_set_layout(&create_info, None) }
            .context("Failed to create Vulkan descriptor set layout")?;

        Ok(Arc::new(DescriptorSetLayout {
            device: self.clone(),
            handle,
            bindings: bindings.to_vec(),
        }))
    }

    /// layoutsを1セットずつ確保するのにちょうど足りるサイズのディスクリプタプールを1つ作る。
    /// 可変長バインディングはレイアウト上の最大本数で確保すると無駄に巨大になるため、variable_count本で計上する
    pub(super) fn create_descriptor_pool_for(
        &self,
        layouts: &[Arc<DescriptorSetLayout>],
        variable_count: u32,
    ) -> Result<vk::DescriptorPool> {
        let pool_sizes: Vec<vk::DescriptorPoolSize> = layouts
            .iter()
            .flat_map(|l| l.bindings())
            .map(|b| {
                let count = if b.variable_count {
                    variable_count
                } else {
                    b.descriptor_count
                };
                // プールサイズに0は指定できない。
                vk::DescriptorPoolSize::default()
                    .ty(b.descriptor_type)
                    .descriptor_count(count.max(1))
            })
            .collect();

        let create_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(layouts.len() as u32)
            .pool_sizes(&pool_sizes);

        unsafe { self.device.create_descriptor_pool(&create_info, None) }
            .context("Failed to create Vulkan descriptor pool")
    }

    /// layoutに対して1つディスクリプタセットを確保する。variable_countが
    /// Someの場合、可変長バインディングの実際の本数を指定する
    pub(super) fn allocate_descriptor_set(
        &self,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        variable_count: Option<u32>,
    ) -> Result<vk::DescriptorSet> {
        let layouts = [layout];
        let counts = [variable_count.unwrap_or(0)];
        let mut variable_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
            .descriptor_counts(&counts);

        let mut alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        if variable_count.is_some() {
            alloc_info = alloc_info.push_next(&mut variable_info);
        }

        let sets = unsafe { self.device.allocate_descriptor_sets(&alloc_info) }
            .context("Failed to allocate Vulkan descriptor set")?;
        Ok(sets[0])
    }

    pub(super) fn write_buffer_descriptor(
        &self,
        set: vk::DescriptorSet,
        binding: u32,
        descriptor_type: vk::DescriptorType,
        buffer: vk::Buffer,
        offset: u64,
        range: u64,
    ) {
        let buffer_info = [vk::DescriptorBufferInfo {
            buffer,
            offset,
            range,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(descriptor_type)
            .buffer_info(&buffer_info);

        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
    }

    pub(super) fn write_image_descriptor(
        &self,
        set: vk::DescriptorSet,
        binding: u32,
        dst_array_element: u32,
        descriptor_type: vk::DescriptorType,
        image_view: vk::ImageView,
        image_layout: vk::ImageLayout,
        sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo {
            sampler,
            image_view,
            image_layout,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .dst_array_element(dst_array_element)
            .descriptor_type(descriptor_type)
            .image_info(&image_info);

        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
    }
}
