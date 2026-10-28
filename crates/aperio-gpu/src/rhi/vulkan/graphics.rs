// グラフィックス(ラスター)パイプラインと、dynamic renderingによる描画。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ash::vk;

use super::bindings::{ResourceBinding, TouchedTextures};
use super::descriptor::DescriptorSetLayout;
use super::device::VulkanDevice;
use super::resources::{to_vk_format, Buffer, TextureView};
use crate::rhi::{
    BlendFactor, BlendState, GraphicsPipelineDesc, PipelineLayoutDesc, TextureFormat, VertexFormat,
};

pub struct GraphicsPipeline {
    device: Arc<VulkanDevice>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layouts: Vec<Arc<DescriptorSetLayout>>,
    layout_desc: PipelineLayoutDesc,
    color_format: TextureFormat,
}

impl GraphicsPipeline {
    pub fn layout_desc(&self) -> &PipelineLayoutDesc {
        &self.layout_desc
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.device.destroy_pipeline(self.pipeline, None);
            self.device
                .device
                .destroy_pipeline_layout(self.layout, None);
        }
    }
}

/// VulkanDevice::render_passの1回の描画コマンド。
pub struct Draw<'a> {
    pub pipeline: &'a GraphicsPipeline,
    pub sets: Vec<Vec<ResourceBinding<'a>>>,
    pub vertex_buffers: Vec<&'a Buffer>,
    pub vertex_count: u32,
    pub instance_count: u32,
}

fn to_vk_vertex_format(format: VertexFormat) -> vk::Format {
    match format {
        VertexFormat::Float32x2 => vk::Format::R32G32_SFLOAT,
        VertexFormat::Float32x3 => vk::Format::R32G32B32_SFLOAT,
        VertexFormat::Float32x4 => vk::Format::R32G32B32A32_SFLOAT,
    }
}

fn to_vk_blend_factor(factor: BlendFactor) -> vk::BlendFactor {
    match factor {
        BlendFactor::Zero => vk::BlendFactor::ZERO,
        BlendFactor::One => vk::BlendFactor::ONE,
        BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
    }
}

fn to_vk_blend_attachment(blend: Option<BlendState>) -> vk::PipelineColorBlendAttachmentState {
    let base = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    match blend {
        None => base.blend_enable(false),
        Some(b) => base
            .blend_enable(true)
            .src_color_blend_factor(to_vk_blend_factor(b.color.src_factor))
            .dst_color_blend_factor(to_vk_blend_factor(b.color.dst_factor))
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(to_vk_blend_factor(b.alpha.src_factor))
            .dst_alpha_blend_factor(to_vk_blend_factor(b.alpha.dst_factor))
            .alpha_blend_op(vk::BlendOp::ADD),
    }
}

impl VulkanDevice {
    pub fn create_graphics_pipeline(
        self: &Arc<Self>,
        desc: &GraphicsPipelineDesc<'_>,
    ) -> Result<Arc<GraphicsPipeline>> {
        let (set_layouts, layout) = self.create_pipeline_layout_from_desc(&desc.layout)?;

        let result = (|| -> Result<vk::Pipeline> {
            let vertex_module = self.create_shader_module(desc.vertex_spirv)?;
            let fragment_module = match self.create_shader_module(desc.fragment_spirv) {
                Ok(m) => m,
                Err(e) => {
                    unsafe { self.device.destroy_shader_module(vertex_module, None) };
                    return Err(e);
                }
            };
            let vertex_entry = std::ffi::CString::new(desc.vertex_entry)?;
            let fragment_entry = std::ffi::CString::new(desc.fragment_entry)?;

            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex_module)
                    .name(&vertex_entry),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(&fragment_entry),
            ];

            let binding_descs: Vec<vk::VertexInputBindingDescription> = desc
                .vertex_buffers
                .iter()
                .enumerate()
                .map(|(i, vb)| vk::VertexInputBindingDescription {
                    binding: i as u32,
                    stride: vb.stride,
                    input_rate: if vb.per_instance {
                        vk::VertexInputRate::INSTANCE
                    } else {
                        vk::VertexInputRate::VERTEX
                    },
                })
                .collect();
            let attribute_descs: Vec<vk::VertexInputAttributeDescription> = desc
                .vertex_buffers
                .iter()
                .enumerate()
                .flat_map(|(i, vb)| {
                    vb.attributes
                        .iter()
                        .map(move |a| vk::VertexInputAttributeDescription {
                            location: a.location,
                            binding: i as u32,
                            format: to_vk_vertex_format(a.format),
                            offset: a.offset,
                        })
                })
                .collect();
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&binding_descs)
                .vertex_attribute_descriptions(&attribute_descs);

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let blend_attachments = [to_vk_blend_attachment(desc.blend)];
            let color_blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic_state =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

            let color_formats = [to_vk_format(desc.color_format)];
            let mut rendering_info =
                vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&color_formats);

            let create_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&color_blend)
                .dynamic_state(&dynamic_state)
                .layout(layout)
                .push_next(&mut rendering_info);

            let pipelines = unsafe {
                self.device
                    .create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
            };
            unsafe {
                self.device.destroy_shader_module(vertex_module, None);
                self.device.destroy_shader_module(fragment_module, None);
            }
            match pipelines {
                Ok(p) => Ok(p[0]),
                Err((_, e)) => Err(e).context("Failed to create Vulkan graphics pipeline"),
            }
        })();

        let pipeline = match result {
            Ok(p) => p,
            Err(e) => {
                unsafe { self.device.destroy_pipeline_layout(layout, None) };
                return Err(e);
            }
        };

        Ok(Arc::new(GraphicsPipeline {
            device: self.clone(),
            pipeline,
            layout,
            set_layouts,
            layout_desc: desc.layout.clone(),
            color_format: desc.color_format,
        }))
    }

    /// 1つのカラーターゲットへ`draws`を順に描画する(同期)。`clear`がNoneなら既存内容を保持する。
    ///
    /// ビューポートのY軸は反転させ、NDCのy+が画面上方向になる(wgpu/D3D/Metalと同じ向き)
    pub fn render_pass(
        &self,
        target: &TextureView,
        clear: Option<[f32; 4]>,
        draws: &[Draw<'_>],
    ) -> Result<()> {
        let target_texture = target.texture();
        let (width, height) = (target_texture.width(), target_texture.height());

        // 検証と、全ドローぶんのディスクリプタプールのサイズ算出。
        let mut all_variable_counts = Vec::with_capacity(draws.len());
        for draw in draws {
            if draw.pipeline.color_format != target_texture.format() {
                bail!(
                    "render_pass: pipeline renders to {:?} but the target is {:?}",
                    draw.pipeline.color_format,
                    target_texture.format()
                );
            }
            all_variable_counts.push(
                self.validate_sets(&draw.pipeline.layout_desc, &draw.sets)
                    .context("render_pass")?,
            );
        }
        let pool_layouts: Vec<_> = draws
            .iter()
            .zip(&all_variable_counts)
            .flat_map(|(draw, counts)| draw.pipeline.set_layouts.iter().zip(counts))
            .collect();

        // 描画0件でもクリアは行う。プールのサイズに0は指定できないため、その場合だけ省く。
        let pool = if pool_layouts.is_empty() {
            None
        } else {
            Some(self.create_descriptor_pool_for(&pool_layouts)?)
        };

        let mut touched = TouchedTextures::default();
        let mut draw_sets = Vec::with_capacity(draws.len());
        for (draw, counts) in draws.iter().zip(&all_variable_counts) {
            let sets = match pool {
                Some(pool) => self.allocate_and_write_sets(
                    pool,
                    &draw.pipeline.set_layouts,
                    &draw.pipeline.layout_desc,
                    &draw.sets,
                    counts,
                    &mut touched,
                )?,
                None => Vec::new(),
            };
            draw_sets.push(sets);
        }

        let command_pool = self.create_command_pool()?;
        let command_buffer = self.allocate_command_buffer(command_pool)?;

        self.submit_and_wait(command_buffer, |cb| unsafe {
            self.transition_touched(
                cb,
                &touched,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
            target_texture.transition(
                self,
                cb,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags::empty(),
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ,
            );

            let attachment = vk::RenderingAttachmentInfo::default()
                .image_view(target.handle())
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(if clear.is_some() {
                    vk::AttachmentLoadOp::CLEAR
                } else {
                    vk::AttachmentLoadOp::LOAD
                })
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: clear.unwrap_or([0.0; 4]),
                    },
                });
            let attachments = [attachment];
            let rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D { width, height },
                })
                .layer_count(1)
                .color_attachments(&attachments);

            self.dynamic_rendering.cmd_begin_rendering(cb, &rendering_info);
            self.device.cmd_set_viewport(
                cb,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: height as f32,
                    width: width as f32,
                    height: -(height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            self.device.cmd_set_scissor(
                cb,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D { width, height },
                }],
            );

            for (draw, sets) in draws.iter().zip(&draw_sets) {
                self.device.cmd_bind_pipeline(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.pipeline.pipeline,
                );
                if !sets.is_empty() {
                    self.device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        draw.pipeline.layout,
                        0,
                        sets,
                        &[],
                    );
                }
                if !draw.vertex_buffers.is_empty() {
                    let handles: Vec<vk::Buffer> =
                        draw.vertex_buffers.iter().map(|b| b.handle()).collect();
                    let offsets = vec![0u64; handles.len()];
                    self.device.cmd_bind_vertex_buffers(cb, 0, &handles, &offsets);
                }
                self.device
                    .cmd_draw(cb, draw.vertex_count, draw.instance_count, 0, 0);
            }

            self.dynamic_rendering.cmd_end_rendering(cb);
        })?;

        unsafe {
            self.device.destroy_command_pool(command_pool, None);
            if let Some(pool) = pool {
                self.device.destroy_descriptor_pool(pool, None);
            }
        }

        Ok(())
    }
}
