// 物理デバイスの選択とロジカルデバイスの生成。
//
// 下流のSlangシェーダーが前提とするVulkan 1.2 コア機能(descriptor indexing関連のフィーチャービット)を満たす物理デバイスのみを候補とする。

use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use ash::vk;

use super::{instance::VulkanInstance, memory::VulkanAllocator, Queue};

pub struct VulkanDevice {
    pub instance: VulkanInstance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    /// VK_KHR_dynamic_rendering(render pass / framebufferオブジェクトを使わずに描画する)。
    pub(crate) dynamic_rendering: ash::khr::dynamic_rendering::Device,
    pub queue_family: u32,
    /// このデバイスが実際にサポートする上限値
    pub limits: vk::PhysicalDeviceLimits,
    /// D3D12リソースのインポート(VK_KHR_external_memory_win32)。拡張が無いデバイスではNone。
    #[cfg(target_os = "windows")]
    pub(crate) external_memory_win32: Option<ash::khr::external_memory_win32::Device>,
    /// dmabufのインポート(VK_KHR_external_memory_fd + VK_EXT_external_memory_dma_buf)。
    #[cfg(target_os = "linux")]
    pub(crate) external_memory_fd: Option<ash::khr::external_memory_fd::Device>,
    /// 非LINEARなDRM format modifierのdmabufを取り込めるか(VK_EXT_image_drm_format_modifier)。
    #[cfg(target_os = "linux")]
    pub(crate) drm_format_modifier: bool,

    pub(crate) allocator: ManuallyDrop<VulkanAllocator>,
}

impl VulkanDevice {
    pub fn new() -> Result<Arc<Self>> {
        let instance = VulkanInstance::new()?;
        let (physical_device, queue_family) = pick_physical_device(&instance)?;
        let (device, external_memory) =
            create_logical_device(&instance, physical_device, queue_family)?;
        let allocator = VulkanAllocator::new(&instance, &device, physical_device)?;
        let dynamic_rendering =
            ash::khr::dynamic_rendering::Device::new(&instance.instance, &device);
        let limits = unsafe {
            instance
                .instance
                .get_physical_device_properties(physical_device)
        }
        .limits;
        #[cfg(target_os = "windows")]
        let external_memory_win32 = external_memory
            .import
            .then(|| ash::khr::external_memory_win32::Device::new(&instance.instance, &device));
        #[cfg(target_os = "linux")]
        let external_memory_fd = external_memory
            .import
            .then(|| ash::khr::external_memory_fd::Device::new(&instance.instance, &device));
        #[cfg(target_os = "linux")]
        let drm_format_modifier = external_memory.drm_format_modifier;

        Ok(Arc::new(Self {
            instance,
            physical_device,
            device,
            dynamic_rendering,
            queue_family,
            limits,
            #[cfg(target_os = "windows")]
            external_memory_win32,
            #[cfg(target_os = "linux")]
            external_memory_fd,
            #[cfg(target_os = "linux")]
            drm_format_modifier,
            allocator: ManuallyDrop::new(allocator),
        }))
    }

    /// このデバイスのアダプタのLUID(D3D12側で同じGPUを選ぶために使う)。取得できなければNone。
    pub fn adapter_luid(&self) -> Option<[u8; 8]> {
        let mut id = vk::PhysicalDeviceIDProperties::default();
        let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
        unsafe {
            self.instance
                .instance
                .get_physical_device_properties2(self.physical_device, &mut props)
        };
        (id.device_luid_valid == vk::TRUE).then_some(id.device_luid)
    }

    /// 可変長入力テクスチャ配列(InputTexturesVar)に使えるサンプル済みイメージ(Texture2D等)の上限値。
    pub(crate) fn max_variable_sampled_image_count(&self) -> u32 {
        /// 入力テクスチャ以外にcomputeステージで消費しうるリソース数(出力、サンプラー、params)。
        const NON_INPUT_RESOURCES: u32 = 3;
        let l = &self.limits;
        l.max_per_stage_descriptor_sampled_images
            .min(l.max_descriptor_set_sampled_images)
            .min(l.max_per_stage_resources.saturating_sub(NON_INPUT_RESOURCES))
    }

    /// キューファミリ内の0番目のキューを取得する。
    /// (現状はグラフィックス/コンピュート兼用の単一キューのみを使う設計。)
    pub(crate) fn queue(&self) -> Queue {
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
            // アロケータの解放(残存メモリブロックのvkFreeMemory)は、まだ生きているself.deviceを使って行う必要があるため、
            // destroy_deviceより先に明示的に行う。
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
        }
    }
}

// TODO: GPUの選択をできるようにしたい。
/// グラフィックス・コンピュート両方に対応するキューファミリを持ち、かつ
/// 必要なdescriptor-indexing機能をサポートする物理デバイスを1つ選ぶ(専用GPUを優先)。
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
             descriptorBindingVariableDescriptorCount, \
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
    let mut features11 = vk::PhysicalDeviceVulkan11Features::default();
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut dynamic_rendering = vk::PhysicalDeviceDynamicRenderingFeatures::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut features11)
        .push_next(&mut features12)
        .push_next(&mut dynamic_rendering);
    unsafe {
        instance
            .instance
            .get_physical_device_features2(pd, &mut features2)
    };

    let has_dynamic_rendering_extension = unsafe {
        instance
            .instance
            .enumerate_device_extension_properties(pd)
    }
    .is_ok_and(|exts| {
        exts.iter().any(|e| {
            e.extension_name_as_c_str()
                .is_ok_and(|n| n == ash::khr::dynamic_rendering::NAME)
        })
    });

    // SlangはSV_VertexID / SV_InstanceIDをDrawParameters capability付きで出力する。
    features11.shader_draw_parameters == vk::TRUE
        && features12.shader_sampled_image_array_non_uniform_indexing == vk::TRUE
        && features12.descriptor_binding_variable_descriptor_count == vk::TRUE
        && features12.runtime_descriptor_array == vk::TRUE
        && has_dynamic_rendering_extension
        && dynamic_rendering.dynamic_rendering == vk::TRUE
}

/// 外部メモリのインポートに必要な任意拡張のうち、有効にできたもの。
#[derive(Clone, Copy, Default)]
struct ExternalMemorySupport {
    /// プラットフォームの外部メモリ拡張一式
    /// (Windows: VK_KHR_external_memory_win32、Linux: external_memory_fd + dma_buf)。
    import: bool,
    /// VK_EXT_image_drm_format_modifier(Linuxのみ)。
    #[allow(dead_code)]
    drm_format_modifier: bool,
}

fn create_logical_device(
    instance: &VulkanInstance,
    physical_device: vk::PhysicalDevice,
    queue_family: u32,
) -> Result<(ash::Device, ExternalMemorySupport)> {
    let queue_priorities = [1.0f32];
    let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family)
        .queue_priorities(&queue_priorities)];

    let mut features11 =
        vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
        .shader_sampled_image_array_non_uniform_indexing(true)
        .descriptor_binding_variable_descriptor_count(true)
        .runtime_descriptor_array(true)
        .descriptor_indexing(true);

    let mut dynamic_rendering =
        vk::PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true);
    let mut extension_names = vec![ash::khr::dynamic_rendering::NAME.as_ptr()];

    // 任意拡張: 外部メモリ(D3D12 / dmabuf)のインポート。無いデバイスでも他の機能は使える。
    let available = unsafe {
        instance
            .instance
            .enumerate_device_extension_properties(physical_device)
    }
    .unwrap_or_default();
    let has_extension = |name: &std::ffi::CStr| {
        available
            .iter()
            .any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == name))
    };
    #[cfg(target_os = "windows")]
    let external_memory_extensions = [
        ash::khr::external_memory::NAME,
        ash::khr::external_memory_win32::NAME,
    ];
    #[cfg(target_os = "linux")]
    let external_memory_extensions = [
        ash::khr::external_memory::NAME,
        ash::khr::external_memory_fd::NAME,
        vk::EXT_EXTERNAL_MEMORY_DMA_BUF_NAME,
    ];
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let external_memory_extensions: [&std::ffi::CStr; 0] = [];
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut external_memory = ExternalMemorySupport {
        import: !external_memory_extensions.is_empty()
            && external_memory_extensions.iter().all(|n| has_extension(n)),
        drm_format_modifier: false,
    };
    if external_memory.import {
        extension_names.extend(external_memory_extensions.iter().map(|n| n.as_ptr()));
        #[cfg(target_os = "linux")]
        if has_extension(ash::ext::image_drm_format_modifier::NAME) {
            extension_names.push(ash::ext::image_drm_format_modifier::NAME.as_ptr());
            external_memory.drm_format_modifier = true;
        }
    }

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_names)
        .push_next(&mut features11)
        .push_next(&mut features12)
        .push_next(&mut dynamic_rendering);

    let device = unsafe {
        instance
            .instance
            .create_device(physical_device, &device_create_info, None)
    }
    .context("Failed to create Vulkan logical device")?;

    Ok((device, external_memory))
}
