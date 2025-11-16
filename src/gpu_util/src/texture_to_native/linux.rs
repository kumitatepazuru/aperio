use crate::image_generator::ImageGenerator;
use crate::texture_to_native::{OffscreenSharedTextureInfo, SharedTexturePlane};
use crate::texture_to_native::{SharedTextureHandle, SharedTextureHandleNativePixmap};
use anyhow::{anyhow, bail, Context, Result};
use ash::khr::external_memory_fd::Device as ExternalMemoryFdDevice;
use ash::vk;
use drm_fourcc::DrmFourcc;
use drm_fourcc::DrmModifier;
use std::{os::unix::io::RawFd, sync::Arc};
use wgpu_hal::{api::Vulkan as HalVulkan, MemoryFlags, TextureDescriptor};
use wgpu_types as wgt;

#[derive(Debug)]
struct LinearImage {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "linux")]
pub fn texture_to_offscreen_shared_info(
    generator: &ImageGenerator,
    texture: &wgpu::Texture,
) -> Result<OffscreenSharedTextureInfo> {
    // 1. Format チェック (RGBAf32 以外はエラー)
    if texture.format() != wgpu::TextureFormat::Rgba32Float {
        bail!("texture format must be Rgba32Float");
    }

    // 2. サイズ等
    let width = texture.width();
    let height = texture.height();
    let depth = texture.depth_or_array_layers();
    if depth != 1 {
        bail!("only 2D textures with depth_or_array_layers == 1 are supported");
    }

    // 3. HAL/Vulkan に降りて、LINEAR + exportable な別テクスチャを用意してコピー、
    //    そのテクスチャのメモリから FD を取る
    let plane = unsafe {
        export_vulkan_texture_as_dmabuf_linear(&generator, texture)
            .context("failed to export texture as dmabuf")?
    };

    let handle = SharedTextureHandle {
        native_pixmap: SharedTextureHandleNativePixmap {
            planes: vec![plane],
            modifier: DrmModifier::Linear, // LINEAR で固定
        },
    };

    Ok(OffscreenSharedTextureInfo {
        handle,
        pixel_format: DrmFourcc::Abgr16161616f, // RGBA16Fで固定
        width,
        height,
    })
}

unsafe fn export_vulkan_texture_as_dmabuf_linear(
    generator: &ImageGenerator,
    texture: &wgpu::Texture,
) -> Result<SharedTexturePlane> {
    let instance = &generator.instance;
    let device = &generator.device;
    let queue = &generator.queue;

    // ---- 1. HAL に降りる ----

    // Device -> hal::vulkan::Device
    let hal_device_guard = device
        .as_hal::<HalVulkan>()
        .ok_or_else(|| anyhow!("wgpu device is not backed by Vulkan/hal"))?;
    let hal_device: &wgpu_hal::vulkan::Device = &*hal_device_guard;

    // ---- 2. LINEAR + exportable な vk::Image を作成 ----
    let width = texture.width();
    let height = texture.height();

    // RGBA32F → VK_FORMAT_R32G32B32A32_SFLOAT を前提
    let format = vk::Format::R16G16B16A16_SFLOAT;

    let LinearImage {
        image,
        memory,
        width,
        height,
    } = create_linear_exportable_image(hal_device, width, height, format)?;

    // ---- 3. その vk::Image を HAL の Texture にラップして wgpu::Texture を作る ----
    //
    // ※ 実際には wgpu_hal::vulkan::Texture に対して
    //    `Texture::from_raw(image, memory, …)` のような関数がある想定。
    //    （Buffer には from_raw/from_raw_managed があるので、それと同じノリです。
    //
    //    ここのシグネチャは wgpu-hal 27 の docs に合わせて調整してください。

    let hal_linear_tex = wrap_vk_image_as_hal_texture(hal_device, image, width, height);

    // wgpu 側に戻すための TextureDescriptor
    let desc = wgpu::TextureDescriptor {
        label: Some("linear export texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };

    let linear_texture = device.create_texture_from_hal::<HalVulkan>(hal_linear_tex, &desc);
    let f16_texture = convert_texture_f32_to_f16(&generator, texture);

    // ---- 4. 元のテクスチャから LINEAR テクスチャへコピー ----
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("copy to linear"),
    });

    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &f16_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &linear_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;

    // ---- 5. LINEAR テクスチャの HAL 側を取り直して external_memory() を取得 ----

    // let linear_hal_tex_guard = linear_texture
    //     .as_hal::<HalVulkan>()
    //     .ok_or_else(|| anyhow!("linear export texture is not Vulkan-backed"))?;
    // let linear_hal_tex: &wgpu_hal::vulkan::Texture = &*linear_hal_tex_guard;

    // let vk_memory = linear_hal_tex.external_memory().ok_or_else(|| {
    //     anyhow!(
    //         "linear texture does not have external_memory; \
    //          it must be created with exportable external memory"
    //     )
    // })?;

    // ---- 6. vkGetMemoryFdKHR で FD を取得 ----
    let hal_instance = instance
        .as_hal::<HalVulkan>()
        .ok_or_else(|| anyhow!("wgpu instance is not Vulkan-backed"))?;
    let raw_instance = hal_instance.shared_instance().raw_instance();
    let raw_device = hal_device.raw_device();

    let external_mem_fd = ExternalMemoryFdDevice::new(raw_instance, raw_device);

    let get_fd_info = vk::MemoryGetFdInfoKHR::default()
        .memory(memory)
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    let fd: RawFd = external_mem_fd
        .get_memory_fd(&get_fd_info)
        .map_err(|e| anyhow!("vkGetMemoryFdKHR failed: {:?}", e))?;

    // ---- 7. RGBA32F の stride / size を計算 ----
    //
    // RGBA32F: 4ch * 32bit = 16 bytes / pixel
    // 線形イメージのサブリソースレイアウトから stride / size を決定する
    let subresource = vk::ImageSubresource::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .array_layer(0);

    let layout = unsafe { raw_device.get_image_subresource_layout(image, subresource) };

    // layout.row_pitch / layout.size はバイト数
    let stride = layout.row_pitch as u32;
    let size = layout.size as u32;

    Ok(SharedTexturePlane {
        fd,
        stride,
        offset: 0,
        size,
    })
}

/// LINEAR タイリング & VK_KHR_external_memory_fd でエクスポート可能な VkImage を作る。
///
/// - 戻り値の LinearImage は「まだ中身が何も入っていない」状態。
/// - レイアウト遷移やコピーは呼び出し側で行う。
/// - `image` / `memory` の破棄と `fd` の close も呼び出し側の責任。
unsafe fn create_linear_exportable_image(
    device: &wgpu_hal::vulkan::Device,
    width: u32,
    height: u32,
    format: vk::Format,
) -> Result<LinearImage> {
    let raw_device = device.raw_device();

    // 1) 外部メモリ(ここでは dma-buf)にバインドできる LINEAR イメージを作成
    let mut external_image_info = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    let image_ci = vk::ImageCreateInfo::default()
        .push_next(&mut external_image_info)
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::LINEAR)
        // 元の wgpu::Texture からコピーして読み出すことを想定して
        // TRANSFER_DST/TRANSFER_SRC + SAMPLED をつけておく。
        .usage(
            vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::SAMPLED,
        )
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let image = raw_device
        .create_image(&image_ci, None)
        .context("vkCreateImage (linear exportable)")?;

    // 2) 外部エクスポート可能なメモリを確保してバインド
    let mem_reqs = raw_device.get_image_memory_requirements(image);

    let instance_shared = device.shared_instance();
    let instance = instance_shared.raw_instance();
    let physical = device.raw_physical_device();

    let memory_type_index = find_memory_type_index_for_export(
        instance,
        physical,
        mem_reqs.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .ok_or_else(|| anyhow!("No suitable memory type for linear export image"))?;

    let mut export_alloc_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

    let alloc_info = vk::MemoryAllocateInfo::default()
        .push_next(&mut export_alloc_info)
        .allocation_size(mem_reqs.size)
        .memory_type_index(memory_type_index);

    let memory = raw_device
        .allocate_memory(&alloc_info, None)
        .context("vkAllocateMemory (exportable)")?;

    raw_device
        .bind_image_memory(image, memory, 0)
        .context("vkBindImageMemory (linear exportable)")?;

    Ok(LinearImage {
        image,
        memory,
        width,
        height,
    })
}

fn find_memory_type_index_for_export(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    required: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };

    for i in 0..mem_props.memory_type_count {
        let supported = (type_bits & (1 << i)) != 0;
        if !supported {
            continue;
        }

        let flags = mem_props.memory_types[i as usize].property_flags;
        if flags.contains(required) {
            return Some(i);
        }
    }

    None
}

// 例: LinearImage を使わず、生の vk::Image / DeviceMemory / サイズを受け取る場合
unsafe fn wrap_vk_image_as_hal_texture(
    hal_device: &wgpu_hal::vulkan::Device,
    vk_image: vk::Image,
    width: u32,
    height: u32,
) -> wgpu_hal::vulkan::Texture {
    // hal 側の TextureDescriptor を組み立てる
    // wgpu-hal の TextureDescriptor は wgpu-types の型をそのまま使います。:contentReference[oaicite:1]{index=1}
    let desc = TextureDescriptor {
        label: Some("offscreen-linear-export"),
        size: wgt::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        // RGBAF16 のみを想定
        format: wgt::TextureFormat::Rgba16Float,
        // 必要な用途に合わせて Usage を足す:
        // - COPY_DST: 元の OPTIMAL texture からコピーしてくる
        // - COPY_SRC: 必要ならさらに別のバッファ/テクスチャにコピーする
        // - TEXTURE_BINDING: シェーダから参照したい場合
        usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC | wgt::TextureUses::RESOURCE,
        // 特にヒントは不要なので空で OK。TRANSIENT / PREFER_COHERENT が必要ならここで足す。:contentReference[oaicite:2]{index=2}
        memory_flags: MemoryFlags::empty(),
        view_formats: Vec::new(),
    };

    // Vulkan backend の Device::texture_from_raw を使って hal::vulkan::Texture を作る。:contentReference[oaicite:3]{index=3}
    //
    // - drop_callback = None:
    //     → wgpu-hal 側が vk_image を破棄してくれる
    // - external_memory = Some(vk_memory):
    //     → wgpu-hal が DeviceMemory も所有し、Texture drop 時に解放してくれる
    //
    // 以後、自前で vkDestroyImage / vkFreeMemory 等を呼ばないこと！
    hal_device.texture_from_raw(
        vk_image, &desc, None, // drop_callback: 画像の破棄も wgpu-hal に任せる
    )
}

fn convert_texture_f32_to_f16(
    generator: &ImageGenerator,
    texture: &wgpu::Texture,
) -> Arc<wgpu::Texture> {
    let output_texture = generator.get_or_create_texture(
        0,
        texture.width(),
        texture.height(),
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        Some("Convert f32 to f16 Texture"),
    );

    // Bind Group の作成
    let input_texture_view = texture.create_view(&Default::default());
    let output_texture_view = output_texture.create_view(&Default::default());
    let bind_group = generator
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("F32 to F16 Bind Group"),
            layout: &generator.f32_to_f16_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&output_texture_view),
                },
            ],
        });

    let mut encoder = generator
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("F32 to F16 Convert Encoder"),
        });

    // コンピュートパスを実行して、f32 -> f16 変換を行う
    {
        let mut cpass = encoder.begin_compute_pass(&Default::default());
        cpass.set_pipeline(&generator.f32_to_f16_pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        // ディスパッチサイズは画像の解像度に基づく
        cpass.dispatch_workgroups((texture.width() + 15) / 16, (texture.height() + 15) / 16, 1);
    }

    generator.queue.submit(Some(encoder.finish()));

    output_texture
}
