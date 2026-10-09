// VulkanDeviceの公開メソッド群(rhi::DeviceのVulkan側の実体)のうち、
// リソース生成・コンピュートディスパッチ・テクスチャ転送。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ash::vk;
use gpu_allocator::MemoryLocation;

use super::bindings::{ResourceBinding, TouchedTextures};
use super::device::VulkanDevice;
use super::pipeline::ComputePipeline;
use super::resources::{to_vk_format, Buffer, Sampler, Texture, TextureView};
use crate::rhi::{
    AddressMode, BufferUsage, FilterMode, SamplerOptions, TextureFormat, TextureUsage,
};

fn to_vk_buffer_usage(usage: BufferUsage) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    if usage.contains(BufferUsage::STORAGE) {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if usage.contains(BufferUsage::UNIFORM) {
        flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
    }
    if usage.contains(BufferUsage::VERTEX) {
        flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
    }
    if usage.contains(BufferUsage::TRANSFER_SRC) {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(BufferUsage::TRANSFER_DST) {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    flags
}

pub(super) fn to_vk_texture_usage(usage: TextureUsage) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(TextureUsage::SAMPLED) {
        flags |= vk::ImageUsageFlags::SAMPLED;
    }
    if usage.contains(TextureUsage::STORAGE) {
        flags |= vk::ImageUsageFlags::STORAGE;
    }
    if usage.contains(TextureUsage::COLOR_TARGET) {
        flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
    }
    if usage.contains(TextureUsage::TRANSFER_SRC) {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(TextureUsage::TRANSFER_DST) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    flags
}

fn to_vk_address_mode(mode: AddressMode) -> vk::SamplerAddressMode {
    match mode {
        AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        AddressMode::MirrorRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        AddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    }
}

fn to_vk_filter(mode: FilterMode) -> vk::Filter {
    match mode {
        FilterMode::Nearest => vk::Filter::NEAREST,
        FilterMode::Linear => vk::Filter::LINEAR,
    }
}

const COLOR_SUBRESOURCE: vk::ImageSubresourceLayers = vk::ImageSubresourceLayers {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    mip_level: 0,
    base_array_layer: 0,
    layer_count: 1,
};

/// copy_texturesの1件分。
pub struct TextureCopy<'a> {
    pub src: &'a Texture,
    pub src_origin: (u32, u32),
    pub dst: &'a Texture,
    pub dst_origin: (u32, u32),
    pub size: (u32, u32),
}

impl VulkanDevice {
    pub fn create_buffer(
        self: &Arc<Self>,
        size: u64,
        usage: BufferUsage,
        cpu_visible: bool,
        label: &str,
    ) -> Result<Arc<Buffer>> {
        let location = if cpu_visible {
            MemoryLocation::CpuToGpu
        } else {
            MemoryLocation::GpuOnly
        };
        self.create_vk_buffer(size, to_vk_buffer_usage(usage), location, label)
    }

    pub fn create_texture(
        self: &Arc<Self>,
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsage,
        label: &str,
    ) -> Result<Arc<Texture>> {
        self.create_vk_texture(
            width,
            height,
            to_vk_format(format),
            to_vk_texture_usage(usage),
            label,
        )
    }

    pub fn create_texture_view(self: &Arc<Self>, texture: &Arc<Texture>) -> Result<Arc<TextureView>> {
        self.create_vk_texture_view(texture, vk::ImageAspectFlags::COLOR)
    }

    pub fn create_sampler(self: &Arc<Self>, options: &SamplerOptions) -> Result<Arc<Sampler>> {
        self.create_vk_sampler(
            to_vk_address_mode(options.address_mode),
            to_vk_filter(options.filter),
        )
    }

    /// 同期実行する(完了までブロックする)。
    ///
    /// `sets[i]`はパイプラインレイアウトのセットiに対応する。レイアウトの各バインディングに
    /// 過不足なくリソースが与えられ、種別・本数が一致していることを検証してから実行する。
    pub fn dispatch_compute(
        &self,
        pipeline: &ComputePipeline,
        sets: &[Vec<ResourceBinding<'_>>],
        workgroups: (u32, u32, u32),
    ) -> Result<()> {
        let layout_desc = pipeline.layout_desc();
        let variable_counts = self
            .validate_sets(layout_desc, sets)
            .context("dispatch_compute")?;

        // 呼び出しごとに専用プールを作って使い捨てる。
        // TODO: プールをパイプライン側で使い回す。
        let pool_layouts: Vec<_> = pipeline.set_layouts.iter().zip(&variable_counts).collect();
        let pool = self.create_descriptor_pool_for(&pool_layouts)?;

        let mut touched = TouchedTextures::default();
        let vk_sets = self.allocate_and_write_sets(
            pool,
            &pipeline.set_layouts,
            layout_desc,
            sets,
            &variable_counts,
            &mut touched,
        )?;

        let command_pool = self.create_command_pool()?;
        let command_buffer = self.allocate_command_buffer(command_pool)?;

        self.submit_and_wait(command_buffer, |cb| unsafe {
            self.transition_touched(cb, &touched, vk::PipelineStageFlags::COMPUTE_SHADER);

            self.device
                .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline.handle());
            if !vk_sets.is_empty() {
                self.device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.pipeline_layout(),
                    0,
                    &vk_sets,
                    &[],
                );
            }
            let (x, y, z) = workgroups;
            self.device.cmd_dispatch(cb, x, y, z);
        })?;

        unsafe {
            self.device.destroy_command_pool(command_pool, None);
            self.device.destroy_descriptor_pool(pool, None);
        }

        Ok(())
    }

    /// テクスチャ全体へCPU側のバイト列をアップロードする。
    pub fn upload_texture_data(self: &Arc<Self>, texture: &Texture, data: &[u8]) -> Result<()> {
        self.upload_texture_region(texture, (0, 0), (texture.width(), texture.height()), data)
    }

    /// テクスチャの矩形領域へCPU側のバイト列(行詰め)をアップロードする。
    pub fn upload_texture_region(
        self: &Arc<Self>,
        texture: &Texture,
        origin: (u32, u32),
        size: (u32, u32),
        data: &[u8],
    ) -> Result<()> {
        let (x, y) = origin;
        let (w, h) = size;
        if x + w > texture.width() || y + h > texture.height() {
            bail!(
                "upload region {w}x{h}@({x},{y}) exceeds the {}x{} texture",
                texture.width(),
                texture.height()
            );
        }
        let expected = w as u64 * h as u64 * texture.format().bytes_per_texel() as u64;
        if data.len() as u64 != expected {
            bail!(
                "upload data is {} bytes, but a {w}x{h} {:?} region needs {expected}",
                data.len(),
                texture.format()
            );
        }

        let staging = self.create_vk_buffer(
            data.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "Texture Upload Staging",
        )?;
        unsafe {
            let ptr = staging
                .mapped_ptr()
                .context("staging buffer should be host-mapped")?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.as_ptr().cast(), data.len());
        }

        let pool = self.create_command_pool()?;
        let cb = self.allocate_command_buffer(pool)?;
        self.submit_and_wait(cb, |cb| unsafe {
            texture.transition(
                self,
                cb,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
            );
            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: COLOR_SUBRESOURCE,
                image_offset: vk::Offset3D {
                    x: x as i32,
                    y: y as i32,
                    z: 0,
                },
                image_extent: vk::Extent3D {
                    width: w,
                    height: h,
                    depth: 1,
                },
            };
            self.device.cmd_copy_buffer_to_image(
                cb,
                staging.handle(),
                texture.handle(),
                vk::ImageLayout::GENERAL,
                &[region],
            );
        })?;
        unsafe { self.device.destroy_command_pool(pool, None) };

        Ok(())
    }

    pub fn download_texture_data(self: &Arc<Self>, texture: &Texture) -> Result<Vec<u8>> {
        let extent = texture.extent();
        let byte_len = (extent.width as u64)
            * (extent.height as u64)
            * texture.format().bytes_per_texel() as u64;

        let readback = self.create_vk_buffer(
            byte_len,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
            "Texture Download Readback",
        )?;

        let pool = self.create_command_pool()?;
        let cb = self.allocate_command_buffer(pool)?;
        self.submit_and_wait(cb, |cb| unsafe {
            texture.transition(
                self,
                cb,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_READ,
            );
            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: COLOR_SUBRESOURCE,
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: extent,
            };
            self.device.cmd_copy_image_to_buffer(
                cb,
                texture.handle(),
                vk::ImageLayout::GENERAL,
                readback.handle(),
                &[region],
            );
        })?;
        unsafe { self.device.destroy_command_pool(pool, None) };

        let ptr = readback
            .mapped_ptr()
            .context("readback buffer should be host-mapped")?;
        let bytes = unsafe {
            std::slice::from_raw_parts(ptr.as_ptr().cast::<u8>(), byte_len as usize).to_vec()
        };
        Ok(bytes)
    }

    /// テクスチャ間の矩形コピーを1回のsubmitでまとめて実行する(同期)。
    /// コピー元とコピー先のフォーマットは一致している必要がある。
    pub fn copy_textures(&self, copies: &[TextureCopy<'_>]) -> Result<()> {
        for c in copies {
            if c.src.format() != c.dst.format() {
                bail!(
                    "copy_textures: format mismatch ({:?} -> {:?})",
                    c.src.format(),
                    c.dst.format()
                );
            }
            let (w, h) = c.size;
            if c.src_origin.0 + w > c.src.width()
                || c.src_origin.1 + h > c.src.height()
                || c.dst_origin.0 + w > c.dst.width()
                || c.dst_origin.1 + h > c.dst.height()
            {
                bail!("copy_textures: region exceeds a texture's bounds");
            }
        }
        if copies.is_empty() {
            return Ok(());
        }

        let pool = self.create_command_pool()?;
        let cb = self.allocate_command_buffer(pool)?;
        self.submit_and_wait(cb, |cb| unsafe {
            for c in copies {
                c.src.transition(
                    self,
                    cb,
                    vk::ImageLayout::GENERAL,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_READ,
                );
                c.dst.transition(
                    self,
                    cb,
                    vk::ImageLayout::GENERAL,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                );
                let region = vk::ImageCopy {
                    src_subresource: COLOR_SUBRESOURCE,
                    src_offset: vk::Offset3D {
                        x: c.src_origin.0 as i32,
                        y: c.src_origin.1 as i32,
                        z: 0,
                    },
                    dst_subresource: COLOR_SUBRESOURCE,
                    dst_offset: vk::Offset3D {
                        x: c.dst_origin.0 as i32,
                        y: c.dst_origin.1 as i32,
                        z: 0,
                    },
                    extent: vk::Extent3D {
                        width: c.size.0,
                        height: c.size.1,
                        depth: 1,
                    },
                };
                self.device.cmd_copy_image(
                    cb,
                    c.src.handle(),
                    vk::ImageLayout::GENERAL,
                    c.dst.handle(),
                    vk::ImageLayout::GENERAL,
                    &[region],
                );
            }
        })?;
        unsafe { self.device.destroy_command_pool(pool, None) };

        Ok(())
    }

    pub fn max_variable_texture_array_len(&self) -> u32 {
        self.max_variable_sampled_image_count()
    }

    pub fn maximum_texture_size(&self) -> u32 {
        self.limits.max_image_dimension2_d
    }
}
