// 物理デバイスの選択とロジカルデバイスの生成。
//
// 下流のSlangシェーダーが前提とするbindlessパターンに必要な Vulkan 1.2 コア機能
// (descriptor indexing関連のフィーチャービット)を満たす物理デバイスのみを候補とする。

use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use ash::vk;

use super::{instance::VulkanInstance, Queue};

pub struct VulkanDevice {
    pub instance: VulkanInstance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue_family: u32,
}

impl VulkanDevice {
    pub fn new() -> Result<Arc<Self>> {
        let instance = VulkanInstance::new()?;
        let (physical_device, queue_family) = pick_physical_device(&instance)?;
        let device = create_logical_device(&instance, physical_device, queue_family)?;

        Ok(Arc::new(Self {
            instance,
            physical_device,
            device,
            queue_family,
        }))
    }

    /// キューファミリ内の0番目のキューを取得する。
    /// (現状はグラフィックス/コンピュート兼用の単一キューのみを使う設計。)
    pub fn queue(&self) -> Queue {
        let raw = unsafe { self.device.get_device_queue(self.queue_family, 0) };
        Queue {
            raw: Mutex::new(raw),
            family_index: self.queue_family,
        }
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        unsafe {
            // 保留中の作業が残っていればここでブロックして待つ。
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}

// TODO: GPUの選択をできるようにしたい。
/// グラフィックス・コンピュート両方に対応するキューファミリを持ち、かつ
/// 必要なdescriptor-indexing機能をサポートする物理デバイスを1つ選ぶ
/// (専用GPUを優先)。
fn pick_physical_device(instance: &VulkanInstance) -> Result<(vk::PhysicalDevice, u32)> {
    let physical_devices = unsafe { instance.instance.enumerate_physical_devices() }
        .context("Failed to enumerate physical devices")?;

    if physical_devices.is_empty() {
        bail!("No Vulkan-capable physical devices found");
    }

    let mut candidates: Vec<(vk::PhysicalDevice, u32, vk::PhysicalDeviceType)> = Vec::new();

    for pd in physical_devices {
        let queue_families =
            unsafe { instance.instance.get_physical_device_queue_family_properties(pd) };
        let Some(family_index) = queue_families.iter().position(|f| {
            f.queue_flags
                .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        }) else {
            continue;
        };

        if !supports_required_features(instance, pd) {
            continue;
        }

        let props = unsafe { instance.instance.get_physical_device_properties(pd) };
        candidates.push((pd, family_index as u32, props.device_type));
    }

    if candidates.is_empty() {
        bail!(
            "No physical device satisfies the RHI's minimum requirements: Vulkan 1.2 with \
             descriptor indexing (shaderSampledImageArrayNonUniformIndexing, \
             descriptorBindingPartiallyBound, descriptorBindingVariableDescriptorCount, \
             runtimeDescriptorArray) and a combined graphics+compute queue family."
        );
    }

    // 専用GPUを優先し、無ければ内蔵GPU、それも無ければ見つかった最初のもの。
    candidates.sort_by_key(|(_, _, ty)| match *ty {
        vk::PhysicalDeviceType::DISCRETE_GPU => 0,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
        _ => 2,
    });

    let (pd, family_index, _) = candidates[0];
    Ok((pd, family_index))
}

fn supports_required_features(instance: &VulkanInstance, pd: vk::PhysicalDevice) -> bool {
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut features12);
    unsafe {
        instance
            .instance
            .get_physical_device_features2(pd, &mut features2)
    };

    features12.shader_sampled_image_array_non_uniform_indexing == vk::TRUE
        && features12.descriptor_binding_partially_bound == vk::TRUE
        && features12.descriptor_binding_variable_descriptor_count == vk::TRUE
        && features12.runtime_descriptor_array == vk::TRUE
}

fn create_logical_device(
    instance: &VulkanInstance,
    physical_device: vk::PhysicalDevice,
    queue_family: u32,
) -> Result<ash::Device> {
    let queue_priorities = [1.0f32];
    let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family)
        .queue_priorities(&queue_priorities)];

    let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
        .shader_sampled_image_array_non_uniform_indexing(true)
        .descriptor_binding_partially_bound(true)
        .descriptor_binding_variable_descriptor_count(true)
        .runtime_descriptor_array(true)
        .descriptor_indexing(true);

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .push_next(&mut features12);

    let device = unsafe {
        instance
            .instance
            .create_device(physical_device, &device_create_info, None)
    }
    .context("Failed to create Vulkan logical device")?;

    Ok(device)
}
