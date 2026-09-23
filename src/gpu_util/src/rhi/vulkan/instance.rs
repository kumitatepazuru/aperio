// Vulkan Instance の生成。オフスクリーン専用の画像処理エンジンのため、
// サーフェス/スワップチェーン関連の拡張は要求しない。
//
// バリデーションレイヤーは debug ビルドでのみ、かつ実際に列挙できた場合のみ有効化する。

use std::ffi::{CStr, CString};

use anyhow::{Context, Result};
use ash::vk;

pub struct VulkanInstance {
    pub instance: ash::Instance,
    pub entry: ash::Entry,
}

impl VulkanInstance {
    pub fn new() -> Result<Self> {
        let entry = unsafe { ash::Entry::load() }.context(
            "Failed to load the Vulkan loader (vulkan-1.dll / libvulkan.so.1). \
             Install a Vulkan-capable GPU driver (and, for local development, \
             the Vulkan SDK) on this machine.",
        )?;

        let app_name = CString::new("aperio-gpu_util").unwrap();
        let engine_name = CString::new("aperio-rhi").unwrap();

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&engine_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_2);

        let validation_layer = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
        let enabled_layers = if cfg!(debug_assertions) && layer_is_available(&entry, &validation_layer)? {
            vec![validation_layer.as_ptr()]
        } else {
            Vec::new()
        };

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&enabled_layers);

        let instance = unsafe { entry.create_instance(&create_info, None) }
            .context("Failed to create Vulkan instance")?;

        Ok(Self { entry, instance })
    }
}

fn layer_is_available(entry: &ash::Entry, wanted: &CStr) -> Result<bool> {
    let available = unsafe { entry.enumerate_instance_layer_properties() }
        .context("Failed to enumerate instance layer properties")?;

    Ok(available.iter().any(|l| {
        let name = unsafe { CStr::from_ptr(l.layer_name.as_ptr()) };
        name == wanted
    }))
}

impl Drop for VulkanInstance {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
    }
}
