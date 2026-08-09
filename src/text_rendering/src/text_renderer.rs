use anyhow::Result;
use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use gpu_util::image_generator::ImageGenerator;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::include_wgsl;
use wgpu::TextureUsages;

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
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    image_generator: ImageGenerator,
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: GlyphAtlas,
    /// R8Unorm アトラス用パイプライン（マスクグリフ）
    mask_pipeline: wgpu::RenderPipeline,
    /// Rgba8Unorm アトラス用パイプライン（カラーグリフ）
    color_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
}

impl TextRenderer {
    /// `ImageGenerator` のデバイス / キューを共有して初期化する。
    pub fn new(image_generator: &ImageGenerator) -> Self {
        let device = &*image_generator.device;

        // バインドグループレイアウト: atlas texture + sampler + uniform
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("TextRenderer BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(include_wgsl!("shaders/text_render.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("TextRenderer Pipeline Layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // インスタンスバッファ頂点属性（stride = 48 bytes）
        let vertex_attrs = [
            wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { shader_location: 1, offset: 8,  format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { shader_location: 2, offset: 16, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { shader_location: 3, offset: 24, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { shader_location: 4, offset: 32, format: wgpu::VertexFormat::Float32x4 },
        ];
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &vertex_attrs,
        };

        // 共通パイプライン設定クロージャ（出力フォーマットは常に Rgba16Float）
        let make_pipeline = |label: &str, fs_entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(vertex_buffer_layout.clone())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        // ALPHA_BLENDING(色factor=SrcAlpha)だと、透明にクリアした
                        // ターゲットへ被覆率の低い(アンチエイリアスされた)グリフを
                        // 描画した際にrgbがcoverageで事前乗算された値として書き込まれて
                        // しまい、fs_mask/fs_colorが返しているストレートアルファ
                        // (rgbはcoverageで乗算しない。aperio全体の規約)が壊れる。
                        // 色チャンネルは常にsrcで置き換え、アルファだけ通常のsrc-overで
                        // 蓄積することで、意図通りのストレートアルファ出力にする
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::Zero,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::OVER,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        // R8Unorm アトラス用（マスクグリフ）: fs_mask で coverage を .r チャンネルから読む
        let mask_pipeline = make_pipeline("TextRenderer Mask Pipeline", "fs_mask");
        // Rgba8Unorm アトラス用（カラーグリフ）: fs_color で RGBA をそのまま読む
        let color_pipeline = make_pipeline("TextRenderer Color Pipeline", "fs_color");

        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            device: image_generator.device.clone(),
            queue: image_generator.queue.clone(),
            image_generator: image_generator.clone(),
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(image_generator.device.clone()),
            mask_pipeline,
            color_pipeline,
            bind_group_layout: bgl,
            atlas_sampler,
        }
    }

    /// テキストをシェイプしてグリフをアトラスに登録し、描画に必要なデータを返す。
    /// グリフが存在しない場合（空文字列・スペースのみ等）は `None` を返す。
    pub fn prepare_render_text(&mut self, spec: &TextSpec) -> Result<Option<PreparedText>> {
        // 1. テキストをシェイプ
        let line_height = spec.font_size * 1.2;
        let metrics = Metrics::new(spec.font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

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
                &self.queue,
                info.cache_key,
            );
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
    pub fn run_render_text(&mut self, prepared: &PreparedText) -> Result<Arc<wgpu::Texture>> {
        let text_width = prepared.width;
        let text_height = prepared.height;

        // 出力テクスチャ: Rgba16Float（COPY_SRC を含めて run_render_chars での切り抜きに対応）
        let output_tex = self.image_generator.get_or_create_texture(
            text_width,
            text_height,
            wgpu::TextureFormat::Rgba16Float,
            TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_SRC,
            Some("Text Output"),
        );

        // ユニフォームバッファ
        let uniforms = Uniforms {
            output_size: [text_width as f32, text_height as f32],
            _pad: [0.0; 2],
        };
        let uniform_buf = self.image_generator.get_or_create_buffer(
            std::mem::size_of::<Uniforms>() as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            Some("Text Uniforms"),
        );
        self.queue
            .write_buffer(&uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        // ページ別にバインドグループとインスタンスバッファを構築するヘルパー
        struct PageDraw {
            bind_group: wgpu::BindGroup,
            instance_buf: Arc<wgpu::Buffer>,
            count: u32,
        }

        let build_page_draws = |page_instances: &HashMap<usize, Vec<GlyphInstance>>,
                                     textures: &[Arc<wgpu::Texture>]| {
            let mut page_order: Vec<usize> = page_instances.keys().copied().collect();
            page_order.sort_unstable();
            page_order
                .iter()
                .map(|&page| {
                    let insts = &page_instances[&page];
                    let atlas_view = textures[page].create_view(&Default::default());
                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Text BG"),
                        layout: &self.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&atlas_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: uniform_buf.as_entire_binding(),
                            },
                        ],
                    });
                    let instance_buf = self.image_generator.get_or_create_buffer(
                        (std::mem::size_of::<GlyphInstance>() * insts.len()) as u64,
                        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        Some("Glyph Instances"),
                    );
                    self.queue
                        .write_buffer(&instance_buf, 0, bytemuck::cast_slice(insts));
                    PageDraw {
                        bind_group,
                        instance_buf,
                        count: insts.len() as u32,
                    }
                })
                .collect::<Vec<_>>()
        };

        // マスクページ（R8Unorm）とカラーページ（Rgba8Unorm）を別々に構築
        let mask_draws = build_page_draws(
            &prepared.mask_page_instances,
            &self.atlas.mask_textures,
        );
        let color_draws = build_page_draws(
            &prepared.color_page_instances,
            &self.atlas.color_textures,
        );

        // レンダーパスで全グリフを描画（マスク → カラーの順）
        let output_view = output_tex.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // マスクグリフ（R8Unorm アトラス）
            if !mask_draws.is_empty() {
                rpass.set_pipeline(&self.mask_pipeline);
                for draw in &mask_draws {
                    rpass.set_bind_group(0, &draw.bind_group, &[]);
                    rpass.set_vertex_buffer(0, draw.instance_buf.slice(..));
                    rpass.draw(0..6, 0..draw.count);
                }
            }

            // カラーグリフ（Rgba8Unorm アトラス）
            if !color_draws.is_empty() {
                rpass.set_pipeline(&self.color_pipeline);
                for draw in &color_draws {
                    rpass.set_bind_group(0, &draw.bind_group, &[]);
                    rpass.set_vertex_buffer(0, draw.instance_buf.slice(..));
                    rpass.draw(0..6, 0..draw.count);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
        Ok(output_tex)
    }

    /// `PreparedText` を受け取り、全文字を 1 GPU パスでレンダリングして
    /// 文字ごとに切り抜いたテクスチャを返す。
    /// 戻り値は `(CharGlyphData, テクスチャ)` のリスト（文字順）。
    pub fn run_render_chars(
        &mut self,
        prepared: &PreparedText,
    ) -> Result<Vec<(CharGlyphData, Arc<wgpu::Texture>)>> {
        let full_tex = self.run_render_text(prepared)?;

        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut result = Vec::new();

        for &(ch, x, y, w, h) in &prepared.char_bounds {
            let char_tex = self.image_generator.get_or_create_texture(
                w,
                h,
                wgpu::TextureFormat::Rgba16Float,
                TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                Some("Char Crop"),
            );
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: full_tex.as_ref(),
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                char_tex.as_image_copy(),
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            result.push((CharGlyphData { ch, x, y, w, h }, char_tex));
        }

        self.queue.submit(Some(encoder.finish()));
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
