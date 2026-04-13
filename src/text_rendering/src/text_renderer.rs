use anyhow::Result;
use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache};
use gpu_util::{image_generator::ImageGenerator, resource_pool::LruCache};
use std::sync::Arc;
use wgpu::include_wgsl;
use wgpu::TextureUsages;

use crate::glyph_atlas::GlyphAtlas;
use crate::GlyphInstance;
use crate::TextCacheKey;
use crate::TextSpec;
use crate::Uniforms;
use crate::CACHE_SIZE;

// ────────────────────────────────────────────────
//  TextRenderer
// ────────────────────────────────────────────────

/// グリフアトラスと GPU レンダーパスを使って文字列を高速描画するレンダラー。
///
/// # 描画フロー
/// 1. cosmic-text でテキストをシェイプし各グリフの位置を取得
/// 2. SwashCache でラスタライズ → GPU アトラステクスチャへ（未登録グリフのみ）
/// 3. グリフ位置 + アトラス UV をインスタンスバッファとして積み
/// 4. レンダーパスで 1 ドローコールで全グリフを描画
/// 5. 出力テクスチャ（`Rgba8Unorm`）を返す
///
/// # キャッシュ
/// - グリフアトラス: `CacheKey` → アトラス領域
/// - 出力テクスチャ: `TextCacheKey` → `Arc<wgpu::Texture>`（LRU 256 エントリ）
pub struct TextRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    image_generator: ImageGenerator,
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: GlyphAtlas,
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    texture_cache: LruCache<TextCacheKey, Option<Arc<wgpu::Texture>>>,
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
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // インスタンスバッファレイアウト（stride = 48 bytes）
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    shader_location: 0,
                    offset: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 1,
                    offset: 8,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 2,
                    offset: 16,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 3,
                    offset: 24,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 4,
                    offset: 32,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("TextRenderer Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_buffer_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

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
            render_pipeline,
            bind_group_layout: bgl,
            atlas_sampler,
            texture_cache: LruCache::new(CACHE_SIZE),
        }
    }

    /// テキストをラスタライズして GPU テクスチャを返す。同一内容はキャッシュから返す。
    /// グリフが存在しない場合（空文字列・スペースのみ等）は `None` を返す。
    pub fn render_text(&mut self, spec: &TextSpec) -> Result<Option<Arc<wgpu::Texture>>> {
        let key = TextCacheKey::from_spec(spec);
        if let Some(cached) = self.texture_cache.get(&key) {
            return Ok(cached);
        }
        let tex = self.render_impl(spec)?;
        self.texture_cache.insert(key, tex.clone());
        Ok(tex)
    }

    /// テキストを 1 文字ずつ独立したテクスチャにして返す。
    /// 全文字列を 1 回の GPU パスでレンダリングし、文字ごとの領域を切り抜く。
    /// 戻り値は `(文字, テクスチャ)` のリスト。空白等の非可視グリフはスキップする。
    pub fn render_chars(&mut self, spec: &TextSpec) -> Result<Vec<(char, Arc<wgpu::Texture>)>> {
        // 1. 全文字列を一度のGPUパスでレンダリング（グリフなしならスキップ）
        let Some(full_tex) = self.render_text(spec)? else {
            return Ok(Vec::new());
        };

        // 2. 文字ごとのバウンディングボックスを収集
        let char_bounds = self.collect_char_glyph_bounds(spec);
        if char_bounds.is_empty() {
            return Ok(Vec::new());
        }

        // 3. 1コマンドエンコーダで全文字を切り抜き
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut result = Vec::new();

        for (ch, x, y, w, h) in char_bounds {
            let char_tex = self.image_generator.get_or_create_texture(
                w,
                h,
                wgpu::TextureFormat::Rgba8Unorm,
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
            result.push((ch, char_tex));
        }

        self.queue.submit(Some(encoder.finish()));
        Ok(result)
    }

    /// テキストをシェイプし、各文字がレンダリングテクスチャ上で占める矩形を返す。
    /// 戻り値は `(char, x, y, width, height)` のリスト（文字順）。不可視グリフはスキップ。
    fn collect_char_glyph_bounds(&mut self, spec: &TextSpec) -> Vec<(char, u32, u32, u32, u32)> {
        let line_height = spec.font_size * 1.2;
        let metrics = Metrics::new(spec.font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        if let Some(max_w) = spec.max_width {
            buffer.set_size(&mut self.font_system, Some(max_w as f32), None);
        }
        if let Some(ref fam) = spec.font_family {
            let attrs = Attrs::new().family(Family::Name(fam.as_str()));
            buffer.set_text(
                &mut self.font_system,
                &spec.text,
                &attrs,
                Shaping::Advanced,
                None,
            );
        } else {
            buffer.set_text(
                &mut self.font_system,
                &spec.text,
                &Attrs::new(),
                Shaping::Advanced,
                None,
            );
        }
        buffer.shape_until_scroll(&mut self.font_system, false);

        // 文字バイトオフセット → char のマッピング（文字順を保持）
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

        // byte_start → グリフ群の最小外接矩形 (min_x, min_y, max_x, max_y)
        let mut bounds_map: std::collections::HashMap<usize, (i32, i32, i32, i32)> =
            Default::default();
        let mut line_idx: u32 = 0;
        let mut last_line_top = f32::NEG_INFINITY;

        for run in buffer.layout_runs() {
            if (run.line_top - last_line_top).abs() > 0.5 {
                if last_line_top != f32::NEG_INFINITY {
                    line_idx += 1;
                }
                last_line_top = run.line_top;
            }
            let extra_y = spec.line_spacing * line_idx as f32;

            for (glyph_idx, glyph) in run.glyphs.iter().enumerate() {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                let extra_x = (spec.char_spacing * glyph_idx as f32) as i32;
                let extra_y_int = extra_y as i32;

                // アトラス登録（既登録なら即戻り）
                self.atlas.ensure_glyph(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &self.queue,
                    physical.cache_key,
                );

                let Some(region) = self.atlas.get_region(physical.cache_key) else {
                    continue;
                };
                let dest_x = physical.x + extra_x + region.placement_left;
                let dest_y = physical.y + extra_y_int - region.placement_top;
                if dest_x < 0 || dest_y < 0 {
                    continue;
                }

                let right = dest_x + region.width as i32;
                let bottom = dest_y + region.height as i32;
                let e = bounds_map
                    .entry(glyph.start)
                    .or_insert((dest_x, dest_y, right, bottom));
                e.0 = e.0.min(dest_x);
                e.1 = e.1.min(dest_y);
                e.2 = e.2.max(right);
                e.3 = e.3.max(bottom);
            }
        }

        // 文字順に (char, x, y, w, h) を構成
        char_byte_map
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
            .collect()
    }

    // ── 内部実装 ──────────────────────────────────

    fn render_impl(&mut self, spec: &TextSpec) -> Result<Option<Arc<wgpu::Texture>>> {
        // 1. テキストをシェイプ
        let line_height = spec.font_size * 1.2;
        let metrics = Metrics::new(spec.font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        if let Some(max_w) = spec.max_width {
            buffer.set_size(&mut self.font_system, Some(max_w as f32), None);
        }

        if let Some(ref fam) = spec.font_family {
            let attrs = Attrs::new().family(Family::Name(fam.as_str()));
            buffer.set_text(
                &mut self.font_system,
                &spec.text,
                &attrs,
                Shaping::Advanced,
                None,
            );
        } else {
            buffer.set_text(
                &mut self.font_system,
                &spec.text,
                &Attrs::new(),
                Shaping::Advanced,
                None,
            );
        }
        buffer.shape_until_scroll(&mut self.font_system, false);

        // 2. グリフ位置と CacheKey を収集
        struct GlyphInfo {
            cache_key: CacheKey,
            pixel_x: i32,
            pixel_y: i32,
            color: [f32; 4],
        }

        let [r, g, b, a] = spec.color;
        let default_color = [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ];

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
                    run.line_w.ceil() as u32 + (spec.char_spacing * run.glyphs.len() as f32) as u32,
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
                });
            }
        }

        // 3. 未登録グリフをアトラスに登録（disjoint borrow で font_system / swash_cache / atlas を個別に借用）
        for info in &glyph_infos {
            self.atlas.ensure_glyph(
                &mut self.font_system,
                &mut self.swash_cache,
                &self.queue,
                info.cache_key,
            );
        }

        // 4. インスタンスバッファをページ別に構築
        // page_instances: page_index → Vec<GlyphInstance>
        let mut page_instances: std::collections::HashMap<usize, Vec<GlyphInstance>> =
            std::collections::HashMap::new();

        for info in &glyph_infos {
            let Some(region) = self.atlas.get_region(info.cache_key) else {
                continue;
            };

            // スクリーン上のグリフ左上 = 物理座標 + placement オフセット
            let dest_x = info.pixel_x + region.placement_left;
            let dest_y = info.pixel_y - region.placement_top;
            if dest_x < 0 || dest_y < 0 {
                continue;
            }

            // アトラス UV（テクスチャサイズはページにより異なる）
            let inv_w = 1.0 / region.tex_width as f32;
            let inv_h = 1.0 / region.tex_height as f32;
            let uv_min = [region.x as f32 * inv_w, region.y as f32 * inv_h];
            let uv_max = [
                (region.x + region.width) as f32 * inv_w,
                (region.y + region.height) as f32 * inv_h,
            ];

            // カラーグリフは固有色を使うため tint は alpha のみ適用
            let inst_color = if region.is_color {
                [1.0, 1.0, 1.0, info.color[3]]
            } else {
                info.color
            };

            // テクスチャサイズを確定
            text_width = text_width.max((dest_x as u32) + region.width);
            text_height = text_height.max((dest_y as u32) + region.height);

            page_instances
                .entry(region.page)
                .or_default()
                .push(GlyphInstance {
                    pos: [dest_x as f32, dest_y as f32],
                    size: [region.width as f32, region.height as f32],
                    uv_min,
                    uv_max,
                    color: inst_color,
                });
        }

        // グリフが何もない（空文字列・スペースのみ等）
        if page_instances.is_empty() {
            return Ok(None);
        }

        // 5. 出力テクスチャを取得または作成（COPY_SRC を含めて render_chars での切り抜きに対応）
        let output_tex = self.image_generator.get_or_create_texture(
            text_width,
            text_height,
            wgpu::TextureFormat::Rgba8Unorm,
            TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_SRC,
            Some("Text Output"),
        );

        // 6. ユニフォームバッファ
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

        // 7. ページ別にバインドグループとインスタンスバッファを事前構築
        // （レンダーパス中は &self を持てないため、ここで全データを確保する）
        struct PageDraw {
            bind_group: wgpu::BindGroup,
            instance_buf: Arc<wgpu::Buffer>,
            count: u32,
        }
        let mut page_order: Vec<usize> = page_instances.keys().copied().collect();
        page_order.sort_unstable();
        let page_draws: Vec<PageDraw> = page_order
            .iter()
            .map(|&page| {
                let insts = &page_instances[&page];
                let atlas_view = self.atlas.textures[page].create_view(&Default::default());
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
            .collect();

        // 8. レンダーパスで全グリフを描画（ページごとにバインドグループを切り替え）
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
            rpass.set_pipeline(&self.render_pipeline);
            for draw in &page_draws {
                rpass.set_bind_group(0, &draw.bind_group, &[]);
                rpass.set_vertex_buffer(0, draw.instance_buf.slice(..));
                // 6 頂点（2 三角形）× インスタンス数
                rpass.draw(0..6, 0..draw.count);
            }
        }

        self.queue.submit(Some(encoder.finish()));

        Ok(Some(output_tex))
    }
}
