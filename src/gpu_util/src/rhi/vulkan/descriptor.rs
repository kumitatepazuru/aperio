// ディスクリプタセットレイアウト/プール/セットの生成。
//
// bindlessの実装を行い、バインディングは
// 固定長(variable_count: false)と実行時可変長(variable_count: true、
// VK_DESCRIPTOR_BINDING_VARIABLE_DESCRIPTOR_COUNT_BIT)の両方を宣言できる。
// 可変長バインディングはVulkanの仕様上レイアウト内の最後のバインディングでなければならない。

use anyhow::{Context, Result};
use ash::vk;

use super::device::VulkanDevice;

#[derive(Clone, Copy)]
pub struct DescriptorBindingDesc {
    pub binding: u32,
    pub descriptor_type: vk::DescriptorType,
    /// 固定バインディングでは実際の本数。可変長バインディングではディスクリプタ
    /// プール確保に使う上限(実際に使う本数はallocate_descriptor_set側で
    /// vk::DescriptorSetVariableDescriptorCountAllocateInfoにより指定する)。
    pub descriptor_count: u32,
    pub stage_flags: vk::ShaderStageFlags,
    pub variable_count: bool,
}

impl VulkanDevice {
    /// 単一のディスクリプタセットレイアウトを生成する(呼び出し側がbindingsを丸ごと1セット分として渡す想定)。
    pub fn create_descriptor_set_layout(
        &self,
        bindings: &[DescriptorBindingDesc],
    ) -> Result<vk::DescriptorSetLayout> {
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
                        | vk::DescriptorBindingFlags::PARTIALLY_BOUND
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

        unsafe { self.device.create_descriptor_set_layout(&create_info, None) }
            .context("Failed to create Vulkan descriptor set layout")
    }

    /// bindingsの内容に見合うサイズのディスクリプタプールを1つ作る
    pub fn create_descriptor_pool(
        &self,
        bindings: &[DescriptorBindingDesc],
        max_sets: u32,
    ) -> Result<vk::DescriptorPool> {
        let pool_sizes: Vec<vk::DescriptorPoolSize> = bindings
            .iter()
            .map(|b| {
                vk::DescriptorPoolSize::default()
                    .ty(b.descriptor_type)
                    .descriptor_count(b.descriptor_count.max(1) * max_sets)
            })
            .collect();

        let create_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(max_sets)
            .pool_sizes(&pool_sizes);

        unsafe { self.device.create_descriptor_pool(&create_info, None) }
            .context("Failed to create Vulkan descriptor pool")
    }

    /// layoutに対して1つディスクリプタセットを確保する。variable_countが
    /// Someの場合、可変長バインディングの実際の本数を指定する
    /// (レイアウト側のdescriptor_countは上限でしかないため)。
    pub fn allocate_descriptor_set(
        &self,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        variable_count: Option<u32>,
    ) -> Result<vk::DescriptorSet> {
        let layouts = [layout];
        let counts = [variable_count.unwrap_or(0)];
        let mut variable_info =
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::default().descriptor_counts(&counts);

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

    pub fn write_buffer_descriptor(
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

    pub fn write_image_descriptor(
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
