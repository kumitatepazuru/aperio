// VulkanDeviceの公開メソッド群(rhi::DeviceのVulkan側の実体)。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ash::vk;
use gpu_allocator::MemoryLocation;

use super::device::VulkanDevice;
use super::pipeline::ComputePipeline;
use super::resources::{from_vk_format, to_vk_format, Buffer, Sampler, Texture, TextureView};
use crate::image_pixel_format::ImagePixelFormat;
use crate::rhi::{
    AddressMode, BufferUsage, FilterMode, InputArity, OutputKind, SamplerOptions, TextureUsage,
};

/// `VulkanDevice::dispatch_compute`の出力先。
pub enum DispatchOutput<'a> {
    Texture(&'a TextureView),
    Buffer(&'a Buffer),
}

const INPUTS_SET: usize = 0;
const RES_SET: usize = 1;

fn to_vk_buffer_usage(usage: BufferUsage) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    if usage.contains(BufferUsage::STORAGE) {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if usage.contains(BufferUsage::TRANSFER_SRC) {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(BufferUsage::TRANSFER_DST) {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    flags
}

fn to_vk_texture_usage(usage: TextureUsage) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(TextureUsage::SAMPLED) {
        flags |= vk::ImageUsageFlags::SAMPLED;
    }
    if usage.contains(TextureUsage::STORAGE) {
        flags |= vk::ImageUsageFlags::STORAGE;
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
        format: ImagePixelFormat,
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
    pub fn dispatch_compute(
        &self,
        pipeline: &ComputePipeline,
        inputs: &[&TextureView],
        output: DispatchOutput<'_>,
        sampler: Option<&Sampler>,
        params: Option<&Buffer>,
        workgroups: (u32, u32, u32),
    ) -> Result<()> {
        let desc = pipeline.desc();
        let output_kind = match output {
            DispatchOutput::Texture(_) => OutputKind::Texture,
            DispatchOutput::Buffer(_) => OutputKind::Buffer,
        };
        if output_kind != desc.output {
            bail!("dispatch_compute: output kind does not match the pipeline's shape");
        }
        if sampler.is_some() != desc.has_sampler {
            bail!("dispatch_compute: sampler presence does not match the pipeline's shape");
        }
        if params.is_some() != desc.has_params {
            bail!("dispatch_compute: params presence does not match the pipeline's shape");
        }
        if desc.input_arity == InputArity::Fixed && inputs.len() as u32 != desc.input_count {
            bail!(
                "dispatch_compute: pipeline expects {} fixed input texture(s), got {}",
                desc.input_count,
                inputs.len()
            );
        }

        if desc.input_arity == InputArity::Variable
            && inputs.len() as u32 > self.max_variable_sampled_image_count()
        {
            bail!(
                "dispatch_compute: {} input textures exceed the device limit of {}",
                inputs.len(),
                self.max_variable_sampled_image_count()
            );
        }

        // 呼び出しごとに専用プールを作って使い捨てる。
        // TODO: プールをパイプライン側で使い回す。
        let pool = self.create_descriptor_pool_for(&pipeline.set_layouts, inputs.len() as u32)?;

        let mut sets = Vec::with_capacity(pipeline.set_layouts.len());
        for layout in pipeline.set_layouts.iter() {
            let variable_count = layout
                .bindings()
                .iter()
                .any(|b| b.variable_count)
                .then_some(inputs.len() as u32);
            sets.push(self.allocate_descriptor_set(pool, layout.handle(), variable_count)?);
        }

        // set 0 = inputs、set 1 = res(binding 0 = 出力、続いて(あれば)サンプラー、params)。
        match &output {
            DispatchOutput::Texture(view) => self.write_image_descriptor(
                sets[RES_SET],
                0,
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                view.handle(),
                vk::ImageLayout::GENERAL,
                vk::Sampler::null(),
            ),
            DispatchOutput::Buffer(buffer) => self.write_buffer_descriptor(
                sets[RES_SET],
                0,
                vk::DescriptorType::STORAGE_BUFFER,
                buffer.handle(),
                0,
                vk::WHOLE_SIZE,
            ),
        }

        let mut next_res_binding = 1;
        if let Some(sampler) = sampler {
            self.write_image_descriptor(
                sets[RES_SET],
                next_res_binding,
                0,
                vk::DescriptorType::SAMPLER,
                vk::ImageView::null(),
                vk::ImageLayout::UNDEFINED,
                sampler.handle(),
            );
            next_res_binding += 1;
        }

        if let Some(params) = params {
            self.write_buffer_descriptor(
                sets[RES_SET],
                next_res_binding,
                vk::DescriptorType::STORAGE_BUFFER,
                params.handle(),
                0,
                vk::WHOLE_SIZE,
            );
        }

        match desc.input_arity {
            InputArity::Fixed => {
                for (i, view) in inputs.iter().enumerate() {
                    self.write_image_descriptor(
                        sets[INPUTS_SET],
                        i as u32,
                        0,
                        vk::DescriptorType::SAMPLED_IMAGE,
                        view.handle(),
                        vk::ImageLayout::GENERAL,
                        vk::Sampler::null(),
                    );
                }
            }
            InputArity::Variable => {
                for (i, view) in inputs.iter().enumerate() {
                    self.write_image_descriptor(
                        sets[INPUTS_SET],
                        0,
                        i as u32,
                        vk::DescriptorType::SAMPLED_IMAGE,
                        view.handle(),
                        vk::ImageLayout::GENERAL,
                        vk::Sampler::null(),
                    );
                }
            }
        }

        // --- コマンド記録・実行 ---
        let command_pool = self.create_command_pool()?;
        let command_buffer = self.allocate_command_buffer(command_pool)?;

        self.submit_and_wait(command_buffer, |cb| unsafe {
            for view in inputs {
                view.texture().transition(
                    self,
                    cb,
                    vk::ImageLayout::GENERAL,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::SHADER_READ,
                );
            }
            if let DispatchOutput::Texture(view) = &output {
                view.texture().transition(
                    self,
                    cb,
                    vk::ImageLayout::GENERAL,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::SHADER_WRITE,
                );
            }

            self.device
                .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline.handle());
            self.device.cmd_bind_descriptor_sets(
                cb,
                vk::PipelineBindPoint::COMPUTE,
                pipeline.pipeline_layout(),
                0,
                &sets,
                &[],
            );
            let (x, y, z) = workgroups;
            self.device.cmd_dispatch(cb, x, y, z);
        })?;

        unsafe {
            self.device.destroy_command_pool(command_pool, None);
            self.device.destroy_descriptor_pool(pool, None);
        }

        Ok(())
    }

    pub fn upload_texture_data(self: &Arc<Self>, texture: &Texture, data: &[u8]) -> Result<()> {
        let extent = texture.extent();
        let byte_len = data.len() as u64;

        let staging = self.create_vk_buffer(
            byte_len,
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
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: extent,
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
        let format = texture.vk_format();
        let byte_len = (extent.width as u64) * (extent.height as u64) * bytes_per_texel(format)?;

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
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
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

    pub fn max_variable_texture_array_len(&self) -> u32 {
        self.max_variable_sampled_image_count()
    }

    pub fn maximum_texture_size(&self) -> u32 {
        self.limits.max_image_dimension2_d
    }
}

/// 1テクセルあたりのバイト数(`download_texture_data`のバッファサイズ算出用)。
fn bytes_per_texel(format: vk::Format) -> Result<u64> {
    match from_vk_format(format)? {
        ImagePixelFormat::Rgba8Unorm => Ok(4),
        ImagePixelFormat::Rgba16Float => Ok(8),
        ImagePixelFormat::Rgba32Float => Ok(16),
    }
}
