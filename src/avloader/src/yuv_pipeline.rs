use anyhow::{bail, Context, Result};
use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::image_generator::ImageGenerator;
use gpu_util::rhi::*;

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

/// Plane descriptor used to allocate + upload data to GPU textures.
#[derive(Debug, Clone)]
pub struct PlaneDesc {
    /// Texture width in texels
    pub tex_width: u32,
    /// Texture height in texels
    pub tex_height: u32,
    /// Bytes per texel (1 → R8Unorm, 2 → R16Unorm or Rg8Unorm, 4 → Rg16Unorm)
    pub bytes_per_texel: u32,
    pub format: TextureFormat,
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
    pub y_scale: f32,
    pub c_offset: f32,
    pub c_scale: f32,
}

impl YuvConvParams {
    /// 8-bit BT.709 limited range (Y ∈ [16, 235], Cb/Cr ∈ [16, 240]).
    ///
    /// Chroma excursion is ±112 around 128, full width = 224.
    /// Dividing by 224 maps [16, 240] → [-0.5, 0.5] to match shader coefficients.
    pub fn limited_8bit() -> Self {
        Self {
            y_offset: 16.0 / 255.0,
            y_scale: 255.0 / 219.0,
            c_offset: 128.0 / 255.0,
            c_scale: 255.0 / 224.0,
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
            y_offset: (64u32 << 6) as f32 / 65535.0,  // 4096 / 65535
            y_scale: 65535.0 / (876u32 << 6) as f32,  // 65535 / 56064
            c_offset: (512u32 << 6) as f32 / 65535.0, // 32768 / 65535
            c_scale: 65535.0 / (896u32 << 6) as f32,  // 65535 / 57344
        }
    }

    /// 8-bit full range (JPEG / sRGB: Y ∈ [0, 255], Cb/Cr ∈ [0, 255] centred at 128).
    ///
    /// (Cb - 128) / 255 maps [0, 255] → [-0.502, 0.498] ≈ [-0.5, 0.5].
    pub fn full_range_8bit() -> Self {
        Self {
            y_offset: 0.0,
            y_scale: 1.0,
            c_offset: 128.0 / 255.0,
            c_scale: 1.0,
        }
    }

    /// 10-bit full range stored in 16-bit LE (Y ∈ [0, 1023], Cb/Cr ∈ [0, 1023] centred at 512).
    /// Same left-shift-6 storage convention as P010.
    ///
    /// (Cb - 512) / 1023 ≈ [-0.5, 0.5] to match shader coefficients.
    pub fn full_range_10bit_in_16() -> Self {
        Self {
            y_offset: 0.0,
            y_scale: 65535.0 / (1023u32 << 6) as f32, // 65535 / 65472
            c_offset: (512u32 << 6) as f32 / 65535.0, // 32768 / 65535
            c_scale: 65535.0 / (1023u32 << 6) as f32, // 65535 / 65472
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

const YUV_SLANG: &str = include_str!("shaders/yuv_to_rgba.slang");

pub struct YuvPipeline {
    layout: YuvLayout,
    pipeline: GraphicsPipeline,
    sampler: Sampler,
    params_buf: Buffer,
}

impl YuvPipeline {
    pub fn new(device: &Device, layout: YuvLayout, params: YuvConvParams) -> Result<Self> {
        // ── sampler ────────────────────────────────────────────────────────
        let sampler = device.create_sampler(&SamplerOptions {
            address_mode: AddressMode::ClampToEdge,
            filter: FilterMode::Linear,
        })?;

        // ── conversion params uniform buffer ────────────────────────────────
        let params_buf =
            device.create_buffer(16, BufferUsage::UNIFORM, true, "YuvPipeline params")?;
        let bytes = params.as_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                params_buf
                    .mapped_ptr()
                    .context("YuvPipeline params buffer is not host-mapped")?
                    .as_ptr()
                    .cast::<u8>(),
                bytes.len(),
            );
        }

        // ── descriptor layout (set 0 のみ。binding は yuv_to_rgba.slang と対応) ──
        let (plane_count, define) = match layout {
            YuvLayout::Planar => (3, "LAYOUT_PLANAR"),
            YuvLayout::PlanarAlpha => (4, "LAYOUT_PLANAR_ALPHA"),
            YuvLayout::SemiPlanar => (2, "LAYOUT_SEMIPLANAR"),
            YuvLayout::SemiPlanarVU => (2, "LAYOUT_SEMIPLANAR_VU"),
        };
        let binding = |binding: u32, kind: BindingKind| BindingDesc {
            binding,
            kind,
            count: 1,
            variable_count: false,
            stages: ShaderStages::FRAGMENT,
        };
        let mut bindings: Vec<BindingDesc> = (0..plane_count)
            .map(|i| binding(i, BindingKind::SampledImage))
            .collect();
        bindings.push(binding(plane_count, BindingKind::Sampler));
        bindings.push(binding(plane_count + 1, BindingKind::UniformBuffer));
        let pipeline_layout = PipelineLayoutDesc {
            sets: vec![SetLayoutDesc { bindings }],
        };

        // ── shader ──────────────────────────────────────────────────────────
        let compile = |entry: &str| {
            compile_slang_to_spirv(
                "yuv_to_rgba",
                "yuv_to_rgba.slang",
                YUV_SLANG,
                entry,
                &[],
                &[(define, "1")],
            )
            .with_context(|| format!("Failed to compile yuv_to_rgba.slang ({define}, {entry})"))
        };
        let vertex_spirv = compile("vs_main")?;
        let fragment_spirv = compile("fs_main")?;

        // Slangは単一エントリポイントのSPIR-Vを常に"main"という名前で出力する。
        let pipeline = device
            .create_graphics_pipeline(&GraphicsPipelineDesc {
                vertex_spirv: &vertex_spirv,
                vertex_entry: "main",
                fragment_spirv: &fragment_spirv,
                fragment_entry: "main",
                layout: pipeline_layout,
                vertex_buffers: vec![],
                color_format: TextureFormat::Rgba16Float,
                blend: None,
            })
            .context("Failed to create the YUV conversion pipeline")?;

        Ok(Self {
            layout,
            pipeline,
            sampler,
            params_buf,
        })
    }

    /// Upload plane data and run the YUV→RGBA16Float render pass.
    /// Returns a pooled Rgba16Float texture (SAMPLED | COLOR_TARGET | TRANSFER_SRC).
    pub fn convert(
        &self,
        ig: &ImageGenerator,
        plane_descs: &[PlaneDesc],
        plane_data: &[&[u8]],
        out_width: u32,
        out_height: u32,
    ) -> Result<Texture> {
        let device = &ig.device;

        // ── upload each YUV plane to a pooled texture ─────────────────────
        let mut plane_views = Vec::with_capacity(plane_descs.len());
        let mut plane_textures = Vec::with_capacity(plane_descs.len());
        for (desc, data) in plane_descs.iter().zip(plane_data.iter()) {
            let tex = ig.get_or_create_texture(
                desc.tex_width,
                desc.tex_height,
                desc.format,
                TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                "YUV plane",
            )?;
            device.upload_texture_data(&tex, data)?;
            plane_views.push(device.create_texture_view(&tex)?);
            plane_textures.push(tex);
        }

        // ── output texture – pooled, caller must not hold across frames ────
        let output = ig.get_or_create_texture(
            out_width,
            out_height,
            TextureFormat::Rgba16Float,
            TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC,
            "YUV output RGBA",
        )?;
        let out_view = device.create_texture_view(&output)?;

        // ── resource set (binding 番号は new() のレイアウトと同じ並び) ───────
        let mut resources: Vec<ResourceBinding> = plane_views
            .iter()
            .enumerate()
            .map(|(i, view)| ResourceBinding {
                binding: i as u32,
                resource: Resource::Texture(view),
            })
            .collect();
        let plane_count = match self.layout {
            YuvLayout::Planar => 3,
            YuvLayout::PlanarAlpha => 4,
            YuvLayout::SemiPlanar | YuvLayout::SemiPlanarVU => 2,
        };
        if plane_views.len() != plane_count {
            bail!(
                "{:?} needs {} planes, got {}",
                self.layout,
                plane_count,
                plane_views.len()
            );
        }
        resources.push(ResourceBinding {
            binding: plane_count as u32,
            resource: Resource::Sampler(&self.sampler),
        });
        resources.push(ResourceBinding {
            binding: plane_count as u32 + 1,
            resource: Resource::Buffer(&self.params_buf),
        });

        device.render_pass(
            &out_view,
            Some([0.0, 0.0, 0.0, 1.0]),
            &[Draw {
                pipeline: &self.pipeline,
                sets: &[&resources],
                vertex_buffers: &[],
                vertex_count: 3, // full-screen triangle
                instance_count: 1,
            }],
        )?;

        Ok(output)
    }
}
