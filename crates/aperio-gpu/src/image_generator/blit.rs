// テクスチャ → 別フォーマットのレンダーターゲットへの等倍コピー(フォーマット変換付き)。

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::compiled_shader::compile_slang_to_spirv;
use crate::image_generator::ImageGenerator;
use crate::rhi::{
    AddressMode, BindingDesc, BindingKind, Draw, FilterMode, GraphicsPipeline,
    GraphicsPipelineDesc, PipelineLayoutDesc, Resource, ResourceBinding, Sampler, SamplerOptions,
    SetLayoutDesc, ShaderStages, Texture, TextureFormat,
};

const BLIT_SLANG: &str = include_str!("../shaders/blit.slang");

/// blit用の遅延生成リソース(出力フォーマットごとのパイプラインと共有サンプラー)。
#[derive(Default)]
pub(crate) struct BlitResources {
    pipelines: HashMap<TextureFormat, GraphicsPipeline>,
    sampler: Option<Sampler>,
}

impl ImageGenerator {
    /// `src`を`dst`へ等倍で描き写す。`dst`は`COLOR_TARGET`用途で作られている必要があり、
    /// `src`は`SAMPLED`用途で作られている必要がある。サイズは同じでなければならない。
    pub fn blit_to_texture(&self, src: &Texture, dst: &Texture) -> Result<()> {
        if (src.width(), src.height()) != (dst.width(), dst.height()) {
            bail!(
                "blit requires equally sized textures ({}x{} -> {}x{})",
                src.width(),
                src.height(),
                dst.width(),
                dst.height()
            );
        }

        let mut blit = self.blit.lock().unwrap();
        if blit.sampler.is_none() {
            blit.sampler = Some(self.device.create_sampler(&SamplerOptions {
                address_mode: AddressMode::ClampToEdge,
                filter: FilterMode::Nearest,
            })?);
        }
        let format = dst.format();
        if !blit.pipelines.contains_key(&format) {
            let pipeline = self.create_blit_pipeline(format)?;
            blit.pipelines.insert(format, pipeline);
        }

        let src_view = self.device.create_texture_view(src)?;
        let dst_view = self.device.create_texture_view(dst)?;
        let bindings = [
            ResourceBinding {
                binding: 0,
                resource: Resource::Texture(&src_view),
            },
            ResourceBinding {
                binding: 1,
                resource: Resource::Sampler(blit.sampler.as_ref().unwrap()),
            },
        ];
        self.device.render_pass(
            &dst_view,
            None,
            &[Draw {
                pipeline: &blit.pipelines[&format],
                sets: &[&bindings],
                vertex_buffers: &[],
                vertex_count: 3,
                instance_count: 1,
            }],
        )
    }

    fn create_blit_pipeline(&self, format: TextureFormat) -> Result<GraphicsPipeline> {
        let compile = |entry: &str| {
            compile_slang_to_spirv("blit", "blit.slang", BLIT_SLANG, entry, &[], &[])
                .with_context(|| format!("Failed to compile the built-in blit shader ({entry})"))
        };
        let (vertex_spirv, fragment_spirv) = (compile("vs_main")?, compile("fs_main")?);

        let binding = |binding, kind| BindingDesc {
            binding,
            kind,
            count: 1,
            variable_count: false,
            stages: ShaderStages::FRAGMENT,
        };
        // Slangは単一エントリポイントのSPIR-Vを常に"main"という名前で出力する。
        self.device
            .create_graphics_pipeline(&GraphicsPipelineDesc {
                vertex_spirv: &vertex_spirv,
                vertex_entry: "main",
                fragment_spirv: &fragment_spirv,
                fragment_entry: "main",
                layout: PipelineLayoutDesc {
                    sets: vec![SetLayoutDesc {
                        bindings: vec![
                            binding(0, BindingKind::SampledImage),
                            binding(1, BindingKind::Sampler),
                        ],
                    }],
                },
                vertex_buffers: vec![],
                color_format: format,
                blend: None,
            })
            .with_context(|| format!("Failed to create the blit pipeline for {format:?}"))
    }
}
