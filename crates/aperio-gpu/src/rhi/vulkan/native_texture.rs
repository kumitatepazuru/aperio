// 外部(ネイティブ)テクスチャのインポート。
//
// Linux: dmabuf(DRM format modifier付き)をVulkanのイメージとして取り込み、Vulkan側が直接書き込む。
//
// Windows: 呼び出し側のD3D12が作ったリソース(D3D12_HEAP_FLAG_SHARED付きで作成し、
// CreateSharedHandleで得たNTハンドル)をVulkanのイメージとして取り込み、Vulkan側が直接書き込む。
// D3D12側は共有ハンドルを提供するだけで、描画コマンドの実行はすべてVulkanが行う。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ash::vk;

use super::device::VulkanDevice;
use super::resources::{to_vk_format, Texture};
use crate::rhi::{TextureFormat, TextureUsage};

#[cfg(target_os = "windows")]
impl VulkanDevice {
    /// D3D12リソースのNTハンドルをVulkanイメージとしてインポートする。
    ///
    /// ハンドルの所有権は呼び出し側に残る(Vulkanはインポート時に参照を取るだけで、
    /// ハンドル自体は閉じない)。`width` / `height` / `format`は元のD3D12リソースと
    /// 一致している必要がある。
    pub fn import_d3d12_texture(
        self: &Arc<Self>,
        nt_handle: usize,
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsage,
    ) -> Result<Arc<Texture>> {
        let external_memory = self.external_memory_win32.as_ref().context(
            "This device does not support VK_KHR_external_memory_win32, \
             so D3D12 textures cannot be imported",
        )?;
        let handle_type = vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE;
        let vk_format = to_vk_format(format);
        let vk_usage = super::dispatch::to_vk_texture_usage(usage);

        // このフォーマット・用途の組み合わせでD3D12リソースをインポートできるか確認する。
        let mut external_info =
            vk::PhysicalDeviceExternalImageFormatInfo::default().handle_type(handle_type);
        let format_info = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(vk_format)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk_usage)
            .push_next(&mut external_info);
        let mut external_props = vk::ExternalImageFormatProperties::default();
        let mut props = vk::ImageFormatProperties2::default().push_next(&mut external_props);
        unsafe {
            self.instance
                .instance
                .get_physical_device_image_format_properties2(
                    self.physical_device,
                    &format_info,
                    &mut props,
                )
        }
        .with_context(|| {
            format!("{format:?} with {usage:?} cannot be used with D3D12 resource import")
        })?;
        let features = external_props
            .external_memory_properties
            .external_memory_features;
        if !features.contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE) {
            bail!("{format:?} D3D12 resources cannot be imported on this device");
        }

        let mut external_image_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };
        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info);
        let image = unsafe { self.device.create_image(&image_create_info, None) }
            .context("Failed to create Vulkan image for the D3D12 resource")?;

        let result = (|| -> Result<vk::DeviceMemory> {
            let raw_handle = nt_handle as vk::HANDLE;
            let mut handle_props = vk::MemoryWin32HandlePropertiesKHR::default();
            unsafe {
                external_memory.get_memory_win32_handle_properties(
                    handle_type,
                    raw_handle,
                    &mut handle_props,
                )
            }
            .context("vkGetMemoryWin32HandlePropertiesKHR failed (is the handle a shared D3D12 resource?)")?;

            let requirements = unsafe { self.device.get_image_memory_requirements(image) };
            let memory_properties = unsafe {
                self.instance
                    .instance
                    .get_physical_device_memory_properties(self.physical_device)
            };
            let allowed = requirements.memory_type_bits & handle_props.memory_type_bits;
            let memory_type_index = (0..memory_properties.memory_type_count)
                .filter(|&i| allowed & (1 << i) != 0)
                .max_by_key(|&i| {
                    memory_properties.memory_types[i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .context("No memory type is compatible with the imported D3D12 resource")?;

            let mut import_info = vk::ImportMemoryWin32HandleInfoKHR::default()
                .handle_type(handle_type)
                .handle(raw_handle);
            let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type_index)
                .push_next(&mut dedicated_info)
                .push_next(&mut import_info);
            let memory = unsafe { self.device.allocate_memory(&allocate_info, None) }
                .context("Failed to import the D3D12 resource memory")?;
            if let Err(e) = unsafe { self.device.bind_image_memory(image, memory, 0) } {
                unsafe { self.device.free_memory(memory, None) };
                return Err(e).context("Failed to bind the imported D3D12 memory");
            }
            Ok(memory)
        })();

        match result {
            Ok(memory) => Ok(Arc::new(Texture::from_imported(
                self.clone(),
                image,
                memory,
                vk_format,
                extent,
            ))),
            Err(e) => {
                unsafe { self.device.destroy_image(image, None) };
                Err(e)
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl VulkanDevice {
    /// dmabufをVulkanイメージとしてインポートする(単一プレーンのみ対応)。
    ///
    /// fdは複製(dup)してからVulkanへ渡すので、呼び出し側のfdは開いたまま残る。
    /// `modifier`がLINEAR / INVALIDのときは通常のLINEARタイリング、それ以外のときは
    /// DRM format modifierを使う(後者はVK_EXT_image_drm_format_modifierが必要)。
    pub fn import_dmabuf_texture(
        self: &Arc<Self>,
        planes: &[crate::rhi::DmaBufPlane],
        modifier: u64,
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsage,
    ) -> Result<Arc<Texture>> {
        const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
        const DRM_FORMAT_MOD_LINEAR: u64 = 0;

        let external_memory = self.external_memory_fd.as_ref().context(
            "This device does not support VK_KHR_external_memory_fd / \
             VK_EXT_external_memory_dma_buf, so dmabufs cannot be imported",
        )?;
        let plane = match planes {
            [plane] => plane,
            [] => bail!("No planes in the dmabuf handle"),
            _ => bail!(
                "Multi-plane dmabufs are not supported (got {} planes)",
                planes.len()
            ),
        };
        let use_linear_tiling =
            modifier == DRM_FORMAT_MOD_INVALID || modifier == DRM_FORMAT_MOD_LINEAR;
        if !use_linear_tiling && !self.drm_format_modifier {
            bail!(
                "dmabuf uses DRM modifier {modifier:#x}, but this device does not support \
                 VK_EXT_image_drm_format_modifier"
            );
        }

        let handle_type = vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
        let vk_format = to_vk_format(format);
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let mut external_image_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let plane_layouts = [vk::SubresourceLayout {
            offset: plane.offset as u64,
            size: plane.size as u64,
            row_pitch: plane.stride as u64,
            array_pitch: 0,
            depth_pitch: 0,
        }];
        let mut drm_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier)
            .plane_layouts(&plane_layouts);
        let mut image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(if use_linear_tiling {
                vk::ImageTiling::LINEAR
            } else {
                vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT
            })
            .usage(super::dispatch::to_vk_texture_usage(usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info);
        if !use_linear_tiling {
            image_create_info = image_create_info.push_next(&mut drm_info);
        }
        let image = unsafe { self.device.create_image(&image_create_info, None) }
            .context("Failed to create Vulkan image for the dmabuf")?;

        // vkAllocateMemoryは成功時にfdの所有権を引き取る。失敗時は自分で閉じる。
        let fd = unsafe { libc::dup(plane.fd) };
        if fd < 0 {
            unsafe { self.device.destroy_image(image, None) };
            return Err(std::io::Error::last_os_error()).context("dup(fd) failed");
        }

        let result = (|| -> Result<vk::DeviceMemory> {
            let mut fd_props = vk::MemoryFdPropertiesKHR::default();
            unsafe { external_memory.get_memory_fd_properties(handle_type, fd, &mut fd_props) }
                .context("vkGetMemoryFdPropertiesKHR failed (is the fd a dmabuf?)")?;

            let requirements = unsafe { self.device.get_image_memory_requirements(image) };
            let memory_properties = unsafe {
                self.instance
                    .instance
                    .get_physical_device_memory_properties(self.physical_device)
            };
            let allowed = requirements.memory_type_bits & fd_props.memory_type_bits;
            let memory_type_index = (0..memory_properties.memory_type_count)
                .filter(|&i| allowed & (1 << i) != 0)
                .max_by_key(|&i| {
                    memory_properties.memory_types[i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .context("No memory type is compatible with the dmabuf")?;

            let mut import_info = vk::ImportMemoryFdInfoKHR::default()
                .handle_type(handle_type)
                .fd(fd);
            let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type_index)
                .push_next(&mut dedicated_info)
                .push_next(&mut import_info);
            let memory = unsafe { self.device.allocate_memory(&allocate_info, None) }
                .context("Failed to import the dmabuf memory")?;
            if let Err(e) = unsafe { self.device.bind_image_memory(image, memory, 0) } {
                unsafe { self.device.free_memory(memory, None) };
                return Err(e).context("Failed to bind the imported dmabuf memory");
            }
            Ok(memory)
        })();

        match result {
            Ok(memory) => Ok(Arc::new(Texture::from_imported(
                self.clone(),
                image,
                memory,
                vk_format,
                extent,
            ))),
            Err(e) => {
                // インポートに失敗した場合、fdの所有権はまだこちらにある。
                unsafe {
                    libc::close(fd);
                    self.device.destroy_image(image, None);
                }
                Err(e)
            }
        }
    }
}
