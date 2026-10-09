use anyhow::Result;
use cosmic_text::{CacheKey, FontSystem, SwashCache, SwashContent};
use gpu_util::rhi::{Device, Texture, TextureFormat, TextureUsage};
use std::collections::HashMap;

use crate::ATLAS_SIZE;

// ── Skyline packer ─────────────────────────────────────────────────────────────
//
// スカイライン法でグリフを詰め込む。シェルフ法より空白が少ない。
//
// `skyline` は (x_start, height) のリスト（x_start でソート済み）。
// セグメント i は [skyline[i].0, skyline[i+1].0) の範囲を高さ skyline[i].1 でカバー
// （最後のセグメントは atlas_width まで延びる）。

struct SkylinePacker {
    width: u32,
    height: u32,
    skyline: Vec<(u32, u32)>,
}

impl SkylinePacker {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            skyline: vec![(0, 0)],
        }
    }

    /// 指定 x でのスカイライン高さを返す
    fn height_at(&self, x: u32) -> u32 {
        let idx = self.skyline.partition_point(|&(sx, _)| sx <= x);
        if idx == 0 {
            0
        } else {
            self.skyline[idx - 1].1
        }
    }

    /// [x, x+w) 内のスカイライン最大高さを返す
    fn max_height_in_range(&self, x: u32, w: u32) -> u32 {
        let x_end = x + w;
        let mut max_h = 0u32;
        for i in 0..self.skyline.len() {
            let seg_x = self.skyline[i].0;
            let seg_end = self.skyline.get(i + 1).map_or(self.width, |&(sx, _)| sx);
            if seg_end <= x {
                continue;
            }
            if seg_x >= x_end {
                break;
            }
            max_h = max_h.max(self.skyline[i].1);
        }
        max_h
    }

    /// 指定サイズ (w, h) の領域を確保して左上座標 (x, y) を返す。
    /// 満杯なら None。1px パディング込みで配置する。
    /// 全セグメント開始点を候補として試し、y が最小になる位置を選ぶ。
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let pad_w = w + 1;
        let pad_h = h + 1;

        let mut best: Option<(u32, u32)> = None;
        for i in 0..self.skyline.len() {
            let x = self.skyline[i].0;
            if x + pad_w > self.width {
                break;
            }
            let y = self.max_height_in_range(x, pad_w);
            if y + pad_h > self.height {
                continue;
            }
            if best.map_or(true, |(_, by)| y < by) {
                best = Some((x, y));
            }
        }

        let (x, y) = best?;
        self.raise_skyline(x, pad_w, y + pad_h);
        Some((x, y))
    }

    /// [x, x+w) のスカイラインを new_h に引き上げる
    fn raise_skyline(&mut self, x: u32, w: u32, new_h: u32) {
        let x_end = x + w;
        // x_end 直後の高さを保存（右端の切れ目を復元するため）
        let h_after = if x_end < self.width {
            Some(self.height_at(x_end))
        } else {
            None
        };

        let insert_pos = self.skyline.partition_point(|&(sx, _)| sx < x);
        let remove_end = self.skyline.partition_point(|&(sx, _)| sx < x_end);
        self.skyline.drain(insert_pos..remove_end);
        self.skyline.insert(insert_pos, (x, new_h));

        if let Some(h) = h_after {
            let after = insert_pos + 1;
            if after >= self.skyline.len() || self.skyline[after].0 != x_end {
                self.skyline.insert(after, (x_end, h));
            }
        }

        // 同一高さの隣接セグメントをマージ
        let mut i = 0;
        while i + 1 < self.skyline.len() {
            if self.skyline[i].1 == self.skyline[i + 1].1 {
                self.skyline.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }
}

// ── Atlas Region ───────────────────────────────────────────────────────────────

pub struct AtlasRegion {
    /// is_color が false なら mask_textures、true なら color_textures へのインデックス
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// UV 計算用テクスチャサイズ（通常ページは ATLAS_SIZE、オーバーサイズは実サイズ）
    pub tex_width: u32,
    pub tex_height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub is_color: bool,
}

// ── GPU グリフアトラス ──────────────────────────────────────────────────────────

pub(crate) struct GlyphAtlas {
    device: Device,
    /// マスクグリフ用テクスチャ (R8Unorm): 通常アトラスページ + オーバーサイズ専用
    pub(crate) mask_textures: Vec<Texture>,
    /// カラーグリフ用テクスチャ (Rgba8Unorm): 通常アトラスページ + オーバーサイズ専用
    pub(crate) color_textures: Vec<Texture>,
    /// mask_textures 先頭ページ群のパッカー
    mask_packers: Vec<SkylinePacker>,
    /// color_textures 先頭ページ群のパッカー
    color_packers: Vec<SkylinePacker>,
    regions: HashMap<CacheKey, Option<AtlasRegion>>,
}

impl GlyphAtlas {
    pub(crate) fn new(device: Device) -> Result<Self> {
        // マスクアトラスは常に 1 ページ用意する（テキストは必ず使う）
        // カラーアトラスは絵文字が登場したときに初期作成する
        let first_mask = make_texture(
            &device,
            ATLAS_SIZE,
            ATLAS_SIZE,
            TextureFormat::R8Unorm,
            "Glyph Atlas Page (Mask R8Unorm)",
        )?;
        Ok(Self {
            device,
            mask_textures: vec![first_mask],
            color_textures: Vec::new(),
            mask_packers: vec![SkylinePacker::new(ATLAS_SIZE, ATLAS_SIZE)],
            color_packers: Vec::new(),
            regions: HashMap::new(),
        })
    }

    // ── グリフ登録 ────────────────────────────────────────────────────────────

    /// グリフがアトラスに登録済みでなければラスタライズして書き込む。
    /// アトラスが満杯なら新規ページを自動作成。ATLAS_SIZE を超えるグリフは専用テクスチャを使用。
    pub(crate) fn ensure_glyph(
        &mut self,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        cache_key: CacheKey,
    ) -> Result<()> {
        if self.regions.contains_key(&cache_key) {
            return Ok(());
        }

        // SwashCache から画像データを取り出す（borrowed なので先に owned data を抽出）
        let extracted = {
            match swash_cache.get_image(font_system, cache_key) {
                None => None,
                Some(image) => {
                    let w = image.placement.width;
                    let h = image.placement.height;
                    if w == 0 || h == 0 {
                        None
                    } else {
                        let is_color = matches!(image.content, SwashContent::Color);
                        // マスクグリフ → R8Unorm: coverage 値のみ（1 byte/pixel）
                        // カラーグリフ → Rgba8Unorm: RGBA そのまま（4 bytes/pixel）
                        let pixel_data: Vec<u8> = match image.content {
                            SwashContent::Mask => image.data.clone(),
                            SwashContent::Color => image.data.clone(),
                            SwashContent::SubpixelMask => image
                                .data
                                .chunks(3)
                                .map(|rgb| {
                                    ((rgb[0] as u32 + rgb[1] as u32 + rgb[2] as u32) / 3) as u8
                                })
                                .collect(),
                        };
                        Some((
                            w,
                            h,
                            image.placement.left,
                            image.placement.top,
                            is_color,
                            pixel_data,
                        ))
                    }
                }
            }
        }; // swash_cache の借用ここで終了

        let region = match extracted {
            None => None,
            Some((w, h, pl_left, pl_top, is_color, pixels)) => {
                let (format, kind) = if is_color {
                    (TextureFormat::Rgba8Unorm, "Color Rgba8Unorm")
                } else {
                    (TextureFormat::R8Unorm, "Mask R8Unorm")
                };

                let (page, ax, ay, tex_w, tex_h) = if w > ATLAS_SIZE || h > ATLAS_SIZE {
                    // ── オーバーサイズグリフ: 専用テクスチャを割り当て ──────────────
                    let tex = make_texture(
                        &self.device,
                        w,
                        h,
                        format,
                        &format!("Glyph Oversized ({kind})"),
                    )?;
                    self.device.upload_texture_data(&tex, &pixels)?;
                    let textures = if is_color {
                        &mut self.color_textures
                    } else {
                        &mut self.mask_textures
                    };
                    let page = textures.len();
                    textures.push(tex);
                    (page, 0u32, 0u32, w, h)
                } else {
                    // ── 通常グリフ: 既存ページへの割り当てを試み、満杯なら新ページ ──
                    let (textures, packers) = if is_color {
                        (&mut self.color_textures, &mut self.color_packers)
                    } else {
                        (&mut self.mask_textures, &mut self.mask_packers)
                    };
                    // カラーアトラスは絵文字が登場した時点で最初のページを作る
                    if is_color && textures.is_empty() {
                        textures.push(make_texture(
                            &self.device,
                            ATLAS_SIZE,
                            ATLAS_SIZE,
                            format,
                            &format!("Glyph Atlas Page ({kind})"),
                        )?);
                        packers.push(SkylinePacker::new(ATLAS_SIZE, ATLAS_SIZE));
                    }

                    let found = packers
                        .iter_mut()
                        .enumerate()
                        .find_map(|(pi, packer)| packer.allocate(w, h).map(|(x, y)| (pi, x, y)));
                    let (pi, ax, ay) = match found {
                        Some(a) => a,
                        None => {
                            let new_pi = textures.len();
                            textures.push(make_texture(
                                &self.device,
                                ATLAS_SIZE,
                                ATLAS_SIZE,
                                format,
                                &format!("Glyph Atlas Page ({kind})"),
                            )?);
                            packers.push(SkylinePacker::new(ATLAS_SIZE, ATLAS_SIZE));
                            let (x, y) = packers
                                .last_mut()
                                .unwrap()
                                .allocate(w, h)
                                .expect("ATLAS_SIZE 未満のグリフが新規ページに入らない");
                            (new_pi, x, y)
                        }
                    };

                    self.device
                        .upload_texture_region(&textures[pi], (ax, ay), (w, h), &pixels)?;
                    (pi, ax, ay, ATLAS_SIZE, ATLAS_SIZE)
                };

                Some(AtlasRegion {
                    page,
                    x: ax,
                    y: ay,
                    width: w,
                    height: h,
                    tex_width: tex_w,
                    tex_height: tex_h,
                    placement_left: pl_left,
                    placement_top: pl_top,
                    is_color,
                })
            }
        };

        self.regions.insert(cache_key, region);
        Ok(())
    }

    pub(crate) fn get_region(&self, cache_key: CacheKey) -> Option<&AtlasRegion> {
        self.regions.get(&cache_key)?.as_ref()
    }
}

fn make_texture(
    device: &Device,
    width: u32,
    height: u32,
    format: TextureFormat,
    label: &str,
) -> Result<Texture> {
    device.create_texture(
        width,
        height,
        format,
        TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        label,
    )
}
