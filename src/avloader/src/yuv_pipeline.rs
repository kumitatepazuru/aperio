use anyhow::Result;
use gpu_util::image_generator::ImageGenerator;
use std::sync::Arc;
use wgpu::{include_wgsl, TextureUsages};

// ─── YUV pixel format categories ─────────────────────────────────────────────

/// Categories of YUV layout that determine which shader pipeline is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YuvLayout {
    /// 3-plane planar (I420, I422, I444, …)
    Planar,
    /// 2-plane semi-planar with interleaved UV (NV12, NV21, …)
    SemiPlanar,
}

/// Plane descriptor used to allocate + upload data to wgpu textures.
#[derive(Debug, Clone)]
pub struct PlaneDesc {
    /// Texture width in texels
    pub tex_width: u32,
    /// Texture height in texels
    pub tex_height: u32,
    /// Bytes per texel (1 → R8Unorm, 2 → Rg8Unorm)
    pub bytes_per_texel: u32,
    pub format: wgpu::TextureFormat,
}

impl PlaneDesc {
    pub fn bytes_per_row(&self) -> u32 {
        self.tex_width * self.bytes_per_texel
    }
    pub fn total_bytes(&self) -> usize {
        (self.tex_width * self.tex_height * self.bytes_per_texel) as usize
    }
}

// ─── YuvPipeline ─────────────────────────────────────────────────────────────

pub struct YuvPipeline {
    layout: YuvLayout,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl YuvPipeline {
    pub fn new(device: &wgpu::Device, layout: YuvLayout) -> Self {
        // ── sampler ────────────────────────────────────────────────────────
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("YuvPipeline sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── bind group layout ───────────────────────────────────────────────
        let plane_binding = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_binding = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };

        let bgl_entries: Vec<wgpu::BindGroupLayoutEntry> = match layout {
            YuvLayout::Planar => vec![
                plane_binding(0), // Y
                plane_binding(1), // Cb
                plane_binding(2), // Cr
                sampler_binding(3),
            ],
            YuvLayout::SemiPlanar => vec![
                plane_binding(0), // Y
                plane_binding(1), // UV (Rg8Unorm)
                sampler_binding(2),
            ],
        };

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("YuvPipeline BGL"),
            entries: &bgl_entries,
        });

        // ── shader ──────────────────────────────────────────────────────────
        let shader = match layout {
            YuvLayout::Planar => {
                device.create_shader_module(include_wgsl!("shaders/yuv_planar_to_rgba.wgsl"))
            }
            YuvLayout::SemiPlanar => {
                device.create_shader_module(include_wgsl!("shaders/yuv_semiplanar_to_rgba.wgsl"))
            }
        };

        // ── render pipeline ─────────────────────────────────────────────────
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("YuvPipeline layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("YuvPipeline"),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        Self {
            layout,
            pipeline,
            bgl,
            sampler,
        }
    }

    /// Upload plane data and run the YUV→RGBA16Float render pass.
    /// Returns an `Arc<wgpu::Texture>` (Rgba16Float, TEXTURE_BINDING | RENDER_ATTACHMENT | COPY_SRC).
    pub fn convert(
        &self,
        ig: &ImageGenerator,
        plane_descs: &[PlaneDesc],
        plane_data: &[&[u8]],
        out_width: u32,
        out_height: u32,
    ) -> Result<Arc<wgpu::Texture>> {
        let device = &ig.device;
        let queue = &ig.queue;

        // ── upload each YUV plane to a pooled texture ─────────────────────
        let plane_textures: Vec<Arc<wgpu::Texture>> = plane_descs
            .iter()
            .zip(plane_data.iter())
            .map(|(desc, data)| {
                let tex = ig.get_or_create_texture(
                    desc.tex_width,
                    desc.tex_height,
                    desc.format,
                    TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    Some("YUV plane"),
                );
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(desc.bytes_per_row()),
                        rows_per_image: Some(desc.tex_height),
                    },
                    wgpu::Extent3d {
                        width: desc.tex_width,
                        height: desc.tex_height,
                        depth_or_array_layers: 1,
                    },
                );
                tex
            })
            .collect();

        // ── output texture – pooled, caller must not hold across frames ────
        let output = ig.get_or_create_texture(
            out_width,
            out_height,
            wgpu::TextureFormat::Rgba16Float,
            TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            Some("YUV output RGBA"),
        );

        // ── bind group ──────────────────────────────────────────────────────
        let views: Vec<wgpu::TextureView> = plane_textures
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();

        let bg_entries: Vec<wgpu::BindGroupEntry> = match self.layout {
            YuvLayout::Planar => vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
            YuvLayout::SemiPlanar => vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        };

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("YuvPipeline bind group"),
            layout: &self.bgl,
            entries: &bg_entries,
        });

        // ── render pass ─────────────────────────────────────────────────────
        let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("YuvPipeline encoder"),
        });

        {
            let mut rpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("YUV→RGBA pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1); // full-screen triangle
        }

        queue.submit([enc.finish()]);

        Ok(output)
    }
}
