use anyhow::Result;
use gpu_util::image_generator::ImageGenerator;
use std::sync::Arc;
use wgpu::{include_wgsl, util::DeviceExt, TextureUsages};

// ─── YUV pixel format categories ─────────────────────────────────────────────

/// Categories of YUV layout that determine which shader pipeline is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YuvLayout {
    /// 3-plane planar (I420, I422, I444, …)
    Planar,
    /// 4-plane planar with separate alpha (YUVA420P, YUVA422P, YUVA444P, …)
    PlanarAlpha,
    /// 2-plane semi-planar with interleaved UV chroma (NV12, NV16, P010, …)
    SemiPlanar,
    /// 2-plane semi-planar with interleaved VU chroma (NV21 — bytes reversed vs NV12)
    SemiPlanarVU,
}

/// Plane descriptor used to allocate + upload data to wgpu textures.
#[derive(Debug, Clone)]
pub struct PlaneDesc {
    /// Texture width in texels
    pub tex_width: u32,
    /// Texture height in texels
    pub tex_height: u32,
    /// Bytes per texel (1 → R8Unorm, 2 → R16Unorm or Rg8Unorm, 4 → Rg16Unorm)
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

// ─── YuvConvParams ────────────────────────────────────────────────────────────

/// YUV→RGB conversion offsets and scales, expressed in the
/// normalised [0, 1] space returned by `textureSample` for the plane's texture format.
///
/// The shader matrix coefficients (1.5748 / 1.8556 etc.) follow the BT.709 convention
/// where Cb and Cr are normalised to **[-0.5, 0.5]**.  Therefore c_scale must map the
/// raw sampled chroma into that range, NOT into [-1, 1].
#[derive(Debug, Clone, Copy)]
pub struct YuvConvParams {
    pub y_offset: f32,
    pub y_scale:  f32,
    pub c_offset: f32,
    pub c_scale:  f32,
}

impl YuvConvParams {
    /// 8-bit BT.709 limited range (Y ∈ [16, 235], Cb/Cr ∈ [16, 240]).
    ///
    /// Chroma excursion is ±112 around 128, full width = 224.
    /// Dividing by 224 maps [16, 240] → [-0.5, 0.5] to match shader coefficients.
    pub fn limited_8bit() -> Self {
        Self {
            y_offset: 16.0 / 255.0,
            y_scale:  255.0 / 219.0,
            c_offset: 128.0 / 255.0,
            c_scale:  255.0 / 224.0,
        }
    }

    /// 10-bit BT.709 limited range stored in 16-bit LE (P010 / YUV420P10LE):
    /// each sample is the 10-bit value left-shifted 6 bits into a 16-bit word,
    /// so R16Unorm sampling yields `(10bit_val << 6) / 65535`.
    ///
    /// Derivation:
    ///   raw = (10bit_val × 64) / 65535
    ///   y_norm  = (raw − y_offset)  × y_scale   → [0, 1]   for val ∈ [64, 940]
    ///   c_norm  = (raw − c_offset)  × c_scale   → [-0.5, 0.5] for val ∈ [64, 960]
    ///
    /// Chroma excursion is ±448 around 512, full width = 896.
    pub fn limited_10bit_in_16() -> Self {
        Self {
            y_offset: (64u32  << 6) as f32 / 65535.0,  // 4096 / 65535
            y_scale:  65535.0 / (876u32 << 6) as f32,  // 65535 / 56064
            c_offset: (512u32 << 6) as f32 / 65535.0,  // 32768 / 65535
            c_scale:  65535.0 / (896u32 << 6) as f32,  // 65535 / 57344
        }
    }

    /// 8-bit full range (JPEG / sRGB: Y ∈ [0, 255], Cb/Cr ∈ [0, 255] centred at 128).
    ///
    /// (Cb - 128) / 255 maps [0, 255] → [-0.502, 0.498] ≈ [-0.5, 0.5].
    pub fn full_range_8bit() -> Self {
        Self {
            y_offset: 0.0,
            y_scale:  1.0,
            c_offset: 128.0 / 255.0,
            c_scale:  1.0,
        }
    }

    /// 10-bit full range stored in 16-bit LE (Y ∈ [0, 1023], Cb/Cr ∈ [0, 1023] centred at 512).
    /// Same left-shift-6 storage convention as P010.
    ///
    /// (Cb - 512) / 1023 ≈ [-0.5, 0.5] to match shader coefficients.
    pub fn full_range_10bit_in_16() -> Self {
        Self {
            y_offset: 0.0,
            y_scale:  65535.0 / (1023u32 << 6) as f32,  // 65535 / 65472
            c_offset: (512u32 << 6) as f32 / 65535.0,   // 32768 / 65535
            c_scale:  65535.0 / (1023u32 << 6) as f32,  // 65535 / 65472
        }
    }

    fn as_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&self.y_offset.to_ne_bytes());
        b[4..8].copy_from_slice(&self.y_scale.to_ne_bytes());
        b[8..12].copy_from_slice(&self.c_offset.to_ne_bytes());
        b[12..16].copy_from_slice(&self.c_scale.to_ne_bytes());
        b
    }
}

// ─── YuvPipeline ─────────────────────────────────────────────────────────────

pub struct YuvPipeline {
    layout: YuvLayout,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params_buf: wgpu::Buffer,
}

impl YuvPipeline {
    pub fn new(device: &wgpu::Device, layout: YuvLayout, params: YuvConvParams) -> Self {
        // ── sampler ────────────────────────────────────────────────────────
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("YuvPipeline sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── conversion params uniform buffer ────────────────────────────────
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("YuvPipeline params"),
            contents: &params.as_bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
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
        let params_binding = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let bgl_entries: Vec<wgpu::BindGroupLayoutEntry> = match layout {
            YuvLayout::Planar => vec![
                plane_binding(0),   // Y
                plane_binding(1),   // Cb
                plane_binding(2),   // Cr
                sampler_binding(3),
                params_binding(4),
            ],
            YuvLayout::PlanarAlpha => vec![
                plane_binding(0),   // Y
                plane_binding(1),   // Cb
                plane_binding(2),   // Cr
                plane_binding(3),   // A
                sampler_binding(4),
                params_binding(5),
            ],
            YuvLayout::SemiPlanar | YuvLayout::SemiPlanarVU => vec![
                plane_binding(0),   // Y
                plane_binding(1),   // UV or VU (Rg8Unorm / Rg16Unorm)
                sampler_binding(2),
                params_binding(3),
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
            YuvLayout::PlanarAlpha => {
                device.create_shader_module(include_wgsl!("shaders/yuv_planar_alpha_to_rgba.wgsl"))
            }
            YuvLayout::SemiPlanar => {
                device.create_shader_module(include_wgsl!("shaders/yuv_semiplanar_to_rgba.wgsl"))
            }
            YuvLayout::SemiPlanarVU => {
                device.create_shader_module(include_wgsl!("shaders/yuv_semiplanar_vu_to_rgba.wgsl"))
            }
        };

        // ── render pipeline ─────────────────────────────────────────────────
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("YuvPipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
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
            multiview_mask: None,
            cache: None,
        });

        Self {
            layout,
            pipeline,
            bgl,
            sampler,
            params_buf,
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params_buf.as_entire_binding(),
                },
            ],
            YuvLayout::PlanarAlpha => vec![
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
                    resource: wgpu::BindingResource::TextureView(&views[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.params_buf.as_entire_binding(),
                },
            ],
            YuvLayout::SemiPlanar | YuvLayout::SemiPlanarVU => vec![
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.params_buf.as_entire_binding(),
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
                multiview_mask: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1); // full-screen triangle
        }

        queue.submit([enc.finish()]);

        Ok(output)
    }
}
