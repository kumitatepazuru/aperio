use anyhow::{Context, Result};
use cosmic_text::{Attrs, Buffer as TextBuffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use gpu_util::compiled_shader::compile_slang_to_spirv;
use gpu_util::image_generator::ImageGenerator;
use gpu_util::rhi::{
    AddressMode, BindingDesc, BindingKind, BlendComponent, BlendFactor, BlendState, Buffer,
    BufferUsage, Device, Draw, FilterMode, GraphicsPipeline, GraphicsPipelineDesc,
    PipelineLayoutDesc, Resource, ResourceBinding, Sampler, SamplerOptions, SetLayoutDesc,
    ShaderStages, Texture, TextureCopy, TextureFormat, TextureUsage, TextureView, VertexAttribute,
    VertexBufferLayout, VertexFormat,
};
use std::collections::HashMap;

use crate::glyph_atlas::GlyphAtlas;
use crate::CharGlyphData;
use crate::FontsList;
use crate::GlyphInstance;
use crate::TextSpec;
use crate::Uniforms;

// ────────────────────────────────────────────────
//  PreparedText
// ────────────────────────────────────────────────

/// `prepare_render_text` が返す中間データ。
/// グリフのシェイプ・アトラス登録済みの状態を保持し、
/// `run_render_text` / `run_render_chars` に渡して実際の描画を行う。
pub struct PreparedText {
    /// マスクグリフのページ別インスタンス (R8Unorm アトラス用)（内部用）
    pub(crate) mask_page_instances: HashMap<usize, Vec<GlyphInstance>>,
    /// カラーグリフのページ別インスタンス (Rgba8Unorm アトラス用)（内部用）
    pub(crate) color_page_instances: HashMap<usize, Vec<GlyphInstance>>,
    /// 文字ごとのバウンディングボックス `(ch, x, y, w, h)`（内部用）
    pub(crate) char_bounds: Vec<(char, u32, u32, u32, u32)>,
    /// 出力テクスチャの幅（px）
    pub width: u32,
    /// 出力テクスチャの高さ（px）
    pub height: u32,
}

// ────────────────────────────────────────────────
//  TextRenderer
// ────────────────────────────────────────────────

/// グリフアトラスと GPU レンダーパスを使って文字列を高速描画するレンダラー。
///
/// # 描画フロー
/// 1. `prepare_render_text` でテキストをシェイプし、グリフをアトラスに登録して
///    `PreparedText`（幅・高さ・インスタンスデータ）を返す
/// 2. `run_render_text` で GPU レンダーパスを実行し出力テクスチャを返す
/// 3. `run_render_chars` は `run_render_text` の結果を文字単位に切り抜いて返す
pub struct TextRenderer {
    device: Device,
    image_generator: ImageGenerator,
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: GlyphAtlas,
    /// R8Unorm アトラス用パイプライン（マスクグリフ）
    mask_pipeline: GraphicsPipeline,
    /// Rgba8Unorm アトラス用パイプライン（カラーグリフ）
    color_pipeline: GraphicsPipeline,
    atlas_sampler: Sampler,
}

const TEXT_RENDER_SLANG: &str = include_str!("shaders/text_render.slang");

/// ホストから見えるバッファへバイト列を書き込む。
fn write_buffer(buffer: &Buffer, bytes: &[u8]) -> Result<()> {
    let ptr = buffer
        .mapped_ptr()
        .context("buffer is not host-mapped")?
        .as_ptr()
        .cast::<u8>();
    // SAFETY: バッファは`bytes.len()`以上のサイズで作られており、同期submitのため
    // GPUがこの領域を読んでいる最中に書き込むことはない。
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
    Ok(())
}

impl TextRenderer {
    /// `ImageGenerator` のデバイスを共有して初期化する。
    pub fn new(image_generator: &ImageGenerator) -> Result<Self> {
        let device = image_generator.device.clone();

        // set 0: atlas texture + sampler + uniform
        let binding = |binding: u32, kind: BindingKind, stages: ShaderStages| BindingDesc {
            binding,
            kind,
            count: 1,
            variable_count: false,
            stages,
        };
        let layout = PipelineLayoutDesc {
            sets: vec![SetLayoutDesc {
                bindings: vec![
                    binding(0, BindingKind::SampledImage, ShaderStages::FRAGMENT),
                    binding(1, BindingKind::Sampler, ShaderStages::FRAGMENT),
                    binding(2, BindingKind::UniformBuffer, ShaderStages::VERTEX),
                ],
            }],
        };

        // インスタンスバッファ頂点属性（stride = 48 bytes）
        let attribute = |location, format, offset| VertexAttribute { location, format, offset };
        let vertex_buffers = vec![VertexBufferLayout {
            stride: std::mem::size_of::<GlyphInstance>() as u32,
            per_instance: true,
            attributes: vec![
                attribute(0, VertexFormat::Float32x2, 0),
                attribute(1, VertexFormat::Float32x2, 8),
                attribute(2, VertexFormat::Float32x2, 16),
                attribute(3, VertexFormat::Float32x2, 24),
                attribute(4, VertexFormat::Float32x4, 32),
            ],
        }];

        let compile = |entry: &str| {
            compile_slang_to_spirv(
                "text_render",
                "text_render.slang",
                TEXT_RENDER_SLANG,
                entry,
                &[],
                &[],
            )
            .with_context(|| format!("Failed to compile text_render.slang ({entry})"))
        };
        let vertex_spirv = compile("vs_main")?;

        // ALPHA_BLENDING(色factor=SrcAlpha)だと、透明にクリアしたターゲットへ被覆率の低い
        // (アンチエイリアスされた)グリフを描画した際にrgbがcoverageで事前乗算された値として
        // 書き込まれてしまい、fs_mask/fs_colorが返しているストレートアルファ(rgbはcoverageで
        // 乗算しない。aperio全体の規約)が壊れる。色チャンネルは常にsrcで置き換え、
        // アルファだけ通常のsrc-overで蓄積することで、意図通りのストレートアルファ出力にする
        let blend = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::Zero,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
            },
        };

        // 共通パイプライン設定（出力フォーマットは常に Rgba16Float）。
        // Slangは単一エントリポイントのSPIR-Vを常に"main"という名前で出力する。
        let make_pipeline = |fragment_entry: &str| -> Result<GraphicsPipeline> {
            let fragment_spirv = compile(fragment_entry)?;
            device
                .create_graphics_pipeline(&GraphicsPipelineDesc {
                    vertex_spirv: &vertex_spirv,
                    vertex_entry: "main",
                    fragment_spirv: &fragment_spirv,
                    fragment_entry: "main",
                    layout: layout.clone(),
                    vertex_buffers: vertex_buffers.clone(),
                    color_format: TextureFormat::Rgba16Float,
                    blend: Some(blend),
                })
                .with_context(|| format!("Failed to create the {fragment_entry} pipeline"))
        };

        // R8Unorm アトラス用（マスクグリフ）: fs_mask で coverage を .r チャンネルから読む
        let mask_pipeline = make_pipeline("fs_mask")?;
        // Rgba8Unorm アトラス用（カラーグリフ）: fs_color で RGBA をそのまま読む
        let color_pipeline = make_pipeline("fs_color")?;

        let atlas_sampler = device.create_sampler(&SamplerOptions {
            address_mode: AddressMode::ClampToEdge,
            filter: FilterMode::Linear,
        })?;

        Ok(Self {
            atlas: GlyphAtlas::new(device.clone())?,
            device,
            image_generator: image_generator.clone(),
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            mask_pipeline,
            color_pipeline,
            atlas_sampler,
        })
    }

    /// テキストをシェイプしてグリフをアトラスに登録し、描画に必要なデータを返す。
    /// グリフが存在しない場合（空文字列・スペースのみ等）は `None` を返す。
    pub fn prepare_render_text(&mut self, spec: &TextSpec) -> Result<Option<PreparedText>> {
        // 1. テキストをシェイプ
        let line_height = spec.font_size * 1.2;
        let metrics = Metrics::new(spec.font_size, line_height);
        let mut buffer = TextBuffer::new(&mut self.font_system, metrics);

        if let Some(max_w) = spec.max_width {
            buffer.set_size(&mut self.font_system, Some(max_w as f32), None);
        }

        let mut attrs = match spec.font_family {
            Some(ref fam) => Attrs::new().family(Family::Name(fam.as_str())),
            None => Attrs::new(),
        };
        if let Some(w) = spec.font_weight {
            attrs = attrs.weight(Weight(w));
        }
        buffer.set_text(
            &mut self.font_system,
            &spec.text,
            &attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // 2. 文字バイトオフセット → char のマッピング（文字順を保持）
        let char_byte_map: Vec<(usize, char)> = {
            let mut pos = 0usize;
            spec.text
                .chars()
                .map(|ch| {
                    let p = pos;
                    pos += ch.len_utf8();
                    (p, ch)
                })
                .collect()
        };

        // 3. グリフ位置と CacheKey を収集
        struct GlyphInfo {
            cache_key: CacheKey,
            pixel_x: i32,
            pixel_y: i32,
            color: [f32; 4],
            byte_start: usize,
        }

        let default_color = spec.color;

        let mut glyph_infos: Vec<GlyphInfo> = Vec::new();
        let mut text_width: u32 = 1;
        let mut text_height: u32 = 1;
        let mut line_idx: u32 = 0;
        let mut last_line_top = f32::NEG_INFINITY;

        for run in buffer.layout_runs() {
            // 行の切り替わりを検出して line_spacing を適用
            if (run.line_top - last_line_top).abs() > 0.5 {
                if last_line_top != f32::NEG_INFINITY {
                    line_idx += 1;
                }
                last_line_top = run.line_top;
            }
            let extra_y = spec.line_spacing * line_idx as f32;
            let bottom = (run.line_top + run.line_height + extra_y).ceil() as u32;
            text_height = text_height.max(bottom);

            for (glyph_idx, glyph) in run.glyphs.iter().enumerate() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                let extra_x = (spec.char_spacing * glyph_idx as f32) as i32;
                let extra_y_int = extra_y as i32;

                // テキスト幅にも char_spacing を反映
                text_width = text_width.max(
                    run.line_w.ceil() as u32
                        + (spec.char_spacing * run.glyphs.len() as f32) as u32,
                );

                let glyph_color = glyph.color_opt.map_or(default_color, |c| {
                    [
                        c.r() as f32 / 255.0,
                        c.g() as f32 / 255.0,
                        c.b() as f32 / 255.0,
                        c.a() as f32 / 255.0,
                    ]
                });

                glyph_infos.push(GlyphInfo {
                    cache_key: physical.cache_key,
                    pixel_x: physical.x + extra_x,
                    pixel_y: physical.y + extra_y_int,
                    color: glyph_color,
                    byte_start: glyph.start,
                });
            }
        }

        // 4. 未登録グリフをアトラスに登録
        for info in &glyph_infos {
            self.atlas.ensure_glyph(
                &mut self.font_system,
                &mut self.swash_cache,
                info.cache_key,
            )?;
        }

        // 5. ページ別インスタンスバッファと文字バウンドを同時に構築
        let mut mask_page_instances: HashMap<usize, Vec<GlyphInstance>> =
            Default::default();
        let mut color_page_instances: HashMap<usize, Vec<GlyphInstance>> =
            Default::default();
        let mut bounds_map: HashMap<usize, (i32, i32, i32, i32)> =
            Default::default();

        for info in &glyph_infos {
            let Some(region) = self.atlas.get_region(info.cache_key) else {
                continue;
            };

            let dest_x = info.pixel_x + region.placement_left;
            let dest_y = info.pixel_y - region.placement_top;
            if dest_x < 0 || dest_y < 0 {
                continue;
            }

            // テクスチャサイズを確定
            text_width = text_width.max((dest_x as u32) + region.width);
            text_height = text_height.max((dest_y as u32) + region.height);

            // 文字バウンド更新（run_render_chars 用）
            let right = dest_x + region.width as i32;
            let bottom = dest_y + region.height as i32;
            let e = bounds_map
                .entry(info.byte_start)
                .or_insert((dest_x, dest_y, right, bottom));
            e.0 = e.0.min(dest_x);
            e.1 = e.1.min(dest_y);
            e.2 = e.2.max(right);
            e.3 = e.3.max(bottom);

            // アトラス UV
            let inv_w = 1.0 / region.tex_width as f32;
            let inv_h = 1.0 / region.tex_height as f32;
            let uv_min = [region.x as f32 * inv_w, region.y as f32 * inv_h];
            let uv_max = [
                (region.x + region.width) as f32 * inv_w,
                (region.y + region.height) as f32 * inv_h,
            ];

            let inst = GlyphInstance {
                pos: [dest_x as f32, dest_y as f32],
                size: [region.width as f32, region.height as f32],
                uv_min,
                uv_max,
                // カラーグリフは固有色を使うため tint は alpha のみ適用
                color: if region.is_color {
                    [1.0, 1.0, 1.0, info.color[3]]
                } else {
                    info.color
                },
            };

            if region.is_color {
                color_page_instances.entry(region.page).or_default().push(inst);
            } else {
                mask_page_instances.entry(region.page).or_default().push(inst);
            }
        }

        // グリフが何もない（空文字列・スペースのみ等）
        if mask_page_instances.is_empty() && color_page_instances.is_empty() {
            return Ok(None);
        }

        // 6. 文字バウンドを文字順に並べる
        let char_bounds = char_byte_map
            .iter()
            .filter_map(|(byte_start, ch)| {
                let (x1, y1, x2, y2) = *bounds_map.get(byte_start)?;
                let w = (x2 - x1) as u32;
                let h = (y2 - y1) as u32;
                if w == 0 || h == 0 {
                    return None;
                }
                Some((*ch, x1 as u32, y1 as u32, w, h))
            })
            .collect();

        Ok(Some(PreparedText {
            mask_page_instances,
            color_page_instances,
            char_bounds,
            width: text_width,
            height: text_height,
        }))
    }

    /// `PreparedText` を受け取り GPU レンダーパスを実行して出力テクスチャを返す。
    pub fn run_render_text(&mut self, prepared: &PreparedText) -> Result<Texture> {
        let text_width = prepared.width;
        let text_height = prepared.height;

        // 出力テクスチャ: Rgba16Float（TRANSFER_SRC を含めて run_render_chars での切り抜きに対応）
        let output_tex = self.image_generator.get_or_create_texture(
            text_width,
            text_height,
            TextureFormat::Rgba16Float,
            TextureUsage::SAMPLED | TextureUsage::COLOR_TARGET | TextureUsage::TRANSFER_SRC,
            "Text Output",
        )?;
        let output_view = self.device.create_texture_view(&output_tex)?;

        // ユニフォームバッファ
        let uniforms = Uniforms {
            output_size: [text_width as f32, text_height as f32],
            _pad: [0.0; 2],
        };
        let uniform_buf = self.image_generator.get_or_create_buffer(
            std::mem::size_of::<Uniforms>() as u64,
            BufferUsage::UNIFORM,
            "Text Uniforms",
        )?;
        write_buffer(&uniform_buf, bytemuck::bytes_of(&uniforms))?;

        // ページ別にアトラスのビューとインスタンスバッファを構築するヘルパー
        struct PageDraw {
            atlas_view: TextureView,
            instance_buf: Buffer,
            count: u32,
        }

        let build_page_draws = |page_instances: &HashMap<usize, Vec<GlyphInstance>>,
                                textures: &[Texture]|
         -> Result<Vec<PageDraw>> {
            let mut page_order: Vec<usize> = page_instances.keys().copied().collect();
            page_order.sort_unstable();
            page_order
                .iter()
                .map(|&page| {
                    let insts = &page_instances[&page];
                    let instance_buf = self.image_generator.get_or_create_buffer(
                        (std::mem::size_of::<GlyphInstance>() * insts.len()) as u64,
                        BufferUsage::VERTEX,
                        "Glyph Instances",
                    )?;
                    write_buffer(&instance_buf, bytemuck::cast_slice(insts))?;
                    Ok(PageDraw {
                        atlas_view: self.device.create_texture_view(&textures[page])?,
                        instance_buf,
                        count: insts.len() as u32,
                    })
                })
                .collect()
        };

        // マスクページ（R8Unorm）とカラーページ（Rgba8Unorm）を別々に構築
        let mask_draws = build_page_draws(&prepared.mask_page_instances, &self.atlas.mask_textures)?;
        let color_draws =
            build_page_draws(&prepared.color_page_instances, &self.atlas.color_textures)?;

        // 全グリフを1つのレンダーパスで描画（マスク → カラーの順）
        let page_draws: Vec<(&GraphicsPipeline, &PageDraw)> = mask_draws
            .iter()
            .map(|d| (&self.mask_pipeline, d))
            .chain(color_draws.iter().map(|d| (&self.color_pipeline, d)))
            .collect();
        let bindings: Vec<[ResourceBinding; 3]> = page_draws
            .iter()
            .map(|(_, d)| {
                [
                    ResourceBinding { binding: 0, resource: Resource::Texture(&d.atlas_view) },
                    ResourceBinding { binding: 1, resource: Resource::Sampler(&self.atlas_sampler) },
                    ResourceBinding { binding: 2, resource: Resource::Buffer(&uniform_buf) },
                ]
            })
            .collect();
        let sets: Vec<[&[ResourceBinding]; 1]> = bindings.iter().map(|b| [&b[..]]).collect();
        let vertex_buffers: Vec<[&Buffer; 1]> =
            page_draws.iter().map(|(_, d)| [&d.instance_buf]).collect();
        let draws: Vec<Draw> = page_draws
            .iter()
            .enumerate()
            .map(|(i, (pipeline, d))| Draw {
                pipeline,
                sets: &sets[i],
                vertex_buffers: &vertex_buffers[i],
                vertex_count: 6,
                instance_count: d.count,
            })
            .collect();

        self.device
            .render_pass(&output_view, Some([0.0; 4]), &draws)?;
        Ok(output_tex)
    }

    /// `PreparedText` を受け取り、全文字を 1 GPU パスでレンダリングして
    /// 文字ごとに切り抜いたテクスチャを返す。
    /// 戻り値は `(CharGlyphData, テクスチャ)` のリスト（文字順）。
    pub fn run_render_chars(
        &mut self,
        prepared: &PreparedText,
    ) -> Result<Vec<(CharGlyphData, Texture)>> {
        let full_tex = self.run_render_text(prepared)?;

        let mut result = Vec::with_capacity(prepared.char_bounds.len());
        for &(ch, x, y, w, h) in &prepared.char_bounds {
            let char_tex = self.image_generator.get_or_create_texture(
                w,
                h,
                TextureFormat::Rgba16Float,
                TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST | TextureUsage::TRANSFER_SRC,
                "Char Crop",
            )?;
            result.push((CharGlyphData { ch, x, y, w, h }, char_tex));
        }

        let copies: Vec<TextureCopy> = result
            .iter()
            .map(|(glyph, tex)| TextureCopy {
                src: &full_tex,
                src_origin: (glyph.x, glyph.y),
                dst: tex,
                dst_origin: (0, 0),
                size: (glyph.w, glyph.h),
            })
            .collect();
        self.device.copy_textures(&copies)?;
        Ok(result)
    }

    /// 既存の FontSystem からシステムフォント一覧を返す。
    pub fn get_fonts_list(&mut self) -> FontsList {
        build_fonts_map(self.font_system.db())
    }
}

// ────────────────────────────────────────────────
//  フォント列挙ユーティリティ
// ────────────────────────────────────────────────

fn build_fonts_map(db: &cosmic_text::fontdb::Database) -> FontsList {
    let mut map: HashMap<String, Vec<u16>> = HashMap::new();
    for face in db.faces() {
        if let Some((family, _)) = face.families.first() {
            let weights = map.entry(family.clone()).or_default();
            let w = face.weight.0;
            if !weights.contains(&w) {
                weights.push(w);
            }
        }
    }
    for weights in map.values_mut() {
        weights.sort_unstable();
    }
    map
}
