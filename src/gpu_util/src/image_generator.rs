// image_generator.rs
pub mod cpu_func_process;
pub mod final_process;
pub mod parallel_process;
pub mod wgsl_process;

use crate::{
    image_generate_builder::{ImageGenerateBuilder, PipelineStep},
    image_generator::{
        cpu_func_process::handle_cpu_func_step, final_process::handle_final_process,
        parallel_process::handle_parallel_step, wgsl_process::handle_wgsl_step,
    },
};
use anyhow::{bail, Context, Result};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Instant,
};
use wgpu::include_wgsl;

// WGSLの後処理シェーダー（f32 RGBA -> u32 RRGGBBAA）
const POST_PROCESS_WGSL: wgpu::ShaderModuleDescriptor<'_> =
    include_wgsl!("shaders/post_process.wgsl");

// パイプラインキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct PipelineCacheKey {
    id: String,
    input_texture_count: usize,
    has_storage: bool,
}

// テクスチャキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct TextureCacheKey {
    step: usize,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
}

// バッファキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct BufferCacheKey {
    size: u64,
    usage: wgpu::BufferUsages,
}

// キャッシュされる値
#[derive(Clone)]
pub(crate) struct CachedPipeline {
    pipeline: Arc<wgpu::ComputePipeline>,
}

/// パイプラインの各ステップの単一の出力を表すenum。
/// データがGPU上にあるか、CPU上にあるかを示します。
#[derive(Clone, Debug)]
pub enum StepOutput {
    Gpu {
        texture: Arc<wgpu::Texture>,
        width: u32,
        height: u32,
    },
    Cpu {
        data: Arc<Vec<f32>>,
        width: u32,
        height: u32,
    },
}

/// パイプラインの中間状態。
/// 直前のステップからの出力のリストです。
/// 並列処理後は複数の要素を持つことがあります。
pub(crate) type ProcessingState = Vec<StepOutput>;

/// wgpuのインスタンス、アダプタ、デバイス、キューを管理し、
/// 画像生成パイプラインを実行するクラス。
#[derive(Clone)]
pub struct ImageGenerator {
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    // 後処理用のパイプラインと関連リソース
    pub(crate) post_process_pipeline: Arc<wgpu::ComputePipeline>,
    pub(crate) post_process_bind_group_layout: Arc<wgpu::BindGroupLayout>,

    // --- パイプラインキャッシュシステム用のフィールド ---
    // 本体。キーとパイプラインオブジェクトを格納
    pipeline_cache: Arc<Mutex<HashMap<PipelineCacheKey, CachedPipeline>>>,
    // LRUアルゴリズムのための順序を保持 (先頭が最新、末尾が最も古い)
    cache_order: Arc<Mutex<VecDeque<PipelineCacheKey>>>,
    // キャッシュの最大サイズ
    max_cache_size: usize,

    // --- テクスチャキャッシュシステム用のフィールド ---
    // テクスチャキャッシュ本体
    texture_cache: Arc<Mutex<HashMap<TextureCacheKey, Arc<wgpu::Texture>>>>,
    // テクスチャキャッシュのLRU順序
    texture_cache_order: Arc<Mutex<VecDeque<TextureCacheKey>>>,
    // テクスチャキャッシュの最大サイズ
    max_texture_cache_size: usize,

    // --- バッファキャッシュシステム用のフィールド ---
    // バッファキャッシュ本体
    buffer_cache: Arc<Mutex<HashMap<BufferCacheKey, Arc<wgpu::Buffer>>>>,
    // バッファキャッシュのLRU順序
    buffer_cache_order: Arc<Mutex<VecDeque<BufferCacheKey>>>,
    // バッファキャッシュの最大サイズ
    max_buffer_cache_size: usize,
}

impl ImageGenerator {
    /// 新しいImageGeneratorインスタンスを非同期で作成します。
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .context("Failed to find an appropriate adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ImageGenerator Device"),
                required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING, // 配列テクスチャバインディングを有効化
                required_limits: wgpu::Limits {
                    max_binding_array_elements_per_shader_stage: 1000, // 必要に応じて調整
                    max_storage_buffer_binding_size: 2147483647,       // 2GB
                    ..wgpu::Limits::defaults()
                },
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("Failed to create device")?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // --- 後処理パイプラインの事前コンパイル ---
        let post_process_shader = device.create_shader_module(POST_PROCESS_WGSL);

        let post_process_bind_group_layout = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Post Process Bind Group Layout"),
                entries: &[
                    // @group(0) @binding(0) var input_texture: texture_2d<f32>;
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // @group(0) @binding(1) var<storage, read_write> output_pixels: array<u32>;
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None, // サイズは実行時に決まるためNone
                        },
                        count: None,
                    },
                ],
            },
        ));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Post Process Pipeline Layout"),
            bind_group_layouts: &[&post_process_bind_group_layout],
            push_constant_ranges: &[],
        });

        let post_process_pipeline = Arc::new(device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some("Post Process Pipeline"),
                layout: Some(&pipeline_layout),
                module: &post_process_shader,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None, // TODO: キャッシュを実装
            },
        ));

        Ok(Self {
            device,
            queue,
            post_process_pipeline,
            post_process_bind_group_layout,

            // キャッシュフィールドの初期化
            pipeline_cache: Arc::new(Mutex::new(HashMap::new())),
            cache_order: Arc::new(Mutex::new(VecDeque::new())),
            max_cache_size: 100, // デフォルトのキャッシュサイズ

            // テクスチャキャッシュの初期化
            texture_cache: Arc::new(Mutex::new(HashMap::new())),
            texture_cache_order: Arc::new(Mutex::new(VecDeque::new())),
            max_texture_cache_size: 100, // デフォルトのテクスチャキャッシュサイズ

            // バッファキャッシュの初期化
            buffer_cache: Arc::new(Mutex::new(HashMap::new())),
            buffer_cache_order: Arc::new(Mutex::new(VecDeque::new())),
            max_buffer_cache_size: 100, // デフォルトのバッファキャッシュサイズ
        })
    }

    // --- キャッシュ管理用のメソッド ---

    /// 現在のキャッシュの最大サイズを取得
    pub fn max_cache_size(&self) -> usize {
        self.max_cache_size
    }

    /// キャッシュの最大サイズを設定
    /// 新しいサイズが現在のキャッシュ数より小さい場合、古いエントリが削除されます。
    pub fn set_max_cache_size(&mut self, size: usize) {
        self.max_cache_size = size;
        let mut cache_order = self.cache_order.lock().unwrap();

        // キャッシュが新しい上限を超えている場合は、古いものから削除
        while cache_order.len() > self.max_cache_size {
            if let Some(oldest_key) = cache_order.pop_back() {
                self.pipeline_cache.lock().unwrap().remove(&oldest_key);
            }
        }
    }

    // --- テクスチャキャッシュ管理用のメソッド ---

    /// 現在のテクスチャキャッシュの最大サイズを取得
    pub fn max_texture_cache_size(&self) -> usize {
        self.max_texture_cache_size
    }

    /// テクスチャキャッシュの最大サイズを設定
    /// 新しいサイズが現在のキャッシュ数より小さい場合、古いエントリが削除されます。
    pub fn set_max_texture_cache_size(&mut self, size: usize) {
        self.max_texture_cache_size = size;
        let mut cache_order = self.texture_cache_order.lock().unwrap();

        // キャッシュが新しい上限を超えている場合は、古いものから削除
        while cache_order.len() > self.max_texture_cache_size {
            if let Some(oldest_key) = cache_order.pop_back() {
                self.texture_cache.lock().unwrap().remove(&oldest_key);
            }
        }
    }

    // --- バッファキャッシュ管理用のメソッド ---

    /// 現在のバッファキャッシュの最大サイズを取得
    pub fn max_buffer_cache_size(&self) -> usize {
        self.max_buffer_cache_size
    }

    /// バッファキャッシュの最大サイズを設定
    /// 新しいサイズが現在のキャッシュ数より小さい場合、古いエントリが削除されます。
    pub fn set_max_buffer_cache_size(&mut self, size: usize) {
        self.max_buffer_cache_size = size;
        let mut cache_order = self.buffer_cache_order.lock().unwrap();

        // キャッシュが新しい上限を超えている場合は、古いものから削除
        while cache_order.len() > self.max_buffer_cache_size {
            if let Some(oldest_key) = cache_order.pop_back() {
                self.buffer_cache.lock().unwrap().remove(&oldest_key);
            }
        }
    }

    /// テクスチャを取得または作成するためのヘルパーメソッド
    pub(crate) fn get_or_create_texture(
        &self,
        step_index: usize,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        label: Option<&str>,
    ) -> Arc<wgpu::Texture> {
        let key = TextureCacheKey {
            step: step_index,
            width,
            height,
            format,
            usage,
        };

        // --- 1. キャッシュ検索とLRU更新 ---
        let mut cache = self.texture_cache.lock().unwrap();
        let mut order = self.texture_cache_order.lock().unwrap();

        if let Some(cached_texture) = cache.get(&key) {
            // ヒットした場合、LRU順序を更新
            if let Some(pos) = order.iter().position(|k| k == &key) {
                order.remove(pos);
            }
            order.push_front(key.clone());
            return cached_texture.clone();
        }

        // --- 2. キャッシュミス: 新しくテクスチャを作成 ---
        let texture = Arc::new(self.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        }));

        // --- 3. 新しいテクスチャをキャッシュに保存 & LRU更新 ---
        cache.insert(key.clone(), texture.clone());
        order.push_front(key.clone());

        // --- 4. キャッシュサイズを超えていたら古いものを削除 ---
        if order.len() > self.max_texture_cache_size {
            if let Some(oldest_key) = order.pop_back() {
                cache.remove(&oldest_key);
            }
        }

        texture
    }

    /// バッファを取得または作成するためのヘルパーメソッド
    pub(crate) fn get_or_create_buffer(
        &self,
        size: u64,
        usage: wgpu::BufferUsages,
        label: Option<&str>,
    ) -> Arc<wgpu::Buffer> {
        let key = BufferCacheKey { size, usage };

        // --- 1. キャッシュ検索とLRU更新 ---
        let mut cache = self.buffer_cache.lock().unwrap();
        let mut order = self.buffer_cache_order.lock().unwrap();

        if let Some(cached_buffer) = cache.get(&key) {
            // ヒットした場合、LRU順序を更新
            if let Some(pos) = order.iter().position(|k| k == &key) {
                order.remove(pos);
            }
            order.push_front(key.clone());
            return cached_buffer.clone();
        }

        // --- 2. キャッシュミス: 新しくバッファを作成 ---
        let buffer = Arc::new(self.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size,
            usage,
            mapped_at_creation: false,
        }));

        // --- 3. 新しいバッファをキャッシュに保存 & LRU更新 ---
        cache.insert(key.clone(), buffer.clone());
        order.push_front(key.clone());

        // --- 4. キャッシュサイズを超えていたら古いものを削除 ---
        if order.len() > self.max_buffer_cache_size {
            if let Some(oldest_key) = order.pop_back() {
                cache.remove(&oldest_key);
            }
        }

        buffer
    }

    /// 指定されたステップリストを、与えられた初期状態から実行する内部関数。
    /// 最終的な状態と、生成されたコマンドエンコーダを返す。
    pub(crate) async fn execute_pipeline(
        &self,
        steps: &[PipelineStep],
        initial_state: ProcessingState,
    ) -> Result<(ProcessingState, Vec<wgpu::CommandEncoder>)> {
        let mut state = initial_state;
        let mut all_encoders = Vec::new();

        for (i, step) in steps.iter().enumerate() {
            // 各ハンドラは `&self` を受け取るように変更する必要がある

            let (new_state, mut encoder_opt) = match step {
                PipelineStep::Wgsl {
                    wgsl,
                    params,
                    output_height,
                    output_width,
                } => {
                    handle_wgsl_step(self, &state, wgsl, params, i, *output_width, *output_height)?
                }
                PipelineStep::Parallel { pipelines } => {
                    // ここが新しいロジック
                    handle_parallel_step(self, &mut state, pipelines, i, &mut all_encoders).await?
                }
                PipelineStep::CpuFunc {
                    func,
                    params,
                    output_height,
                    output_width,
                } => {
                    handle_cpu_func_step(
                        self,
                        &mut state,
                        func,
                        params,
                        *output_width,
                        *output_height,
                        &mut all_encoders,
                    )
                    .await?
                }
            };
            state = new_state;
            all_encoders.append(&mut encoder_opt);
        }

        Ok((state, all_encoders))
    }

    /// ImageGenerateBuilderで構築されたパイプラインを実行し、画像を生成します。
    pub async fn generate(&self, builder: ImageGenerateBuilder) -> Result<Vec<u8>> {
        let time = Instant::now();
        let (final_state_vec, encoders) = self.execute_pipeline(&builder.steps, Vec::new()).await?;

        self.queue.submit(encoders.into_iter().map(|e| e.finish()));

        // final_state_vecは単一の要素を持つはず
        if final_state_vec.len() != 1 {
            bail!(
                "Final processing state should have exactly one element, but has {}",
                final_state_vec.len()
            );
        }
        println!("Pipeline execution completed in {:.2?}.", time.elapsed());

        handle_final_process(self, final_state_vec).await
    }

    // --- パイプラインを取得または生成するためのヘルパーメソッドを追加 ---
    pub(crate) fn get_or_create_pipeline(
        &self,
        key: &PipelineCacheKey,
        shader_module: &wgpu::ShaderModule,
    ) -> Result<CachedPipeline> {
        // --- 1. キャッシュ検索とLRU更新 ---
        let mut cache = self.pipeline_cache.lock().unwrap();
        let mut order = self.cache_order.lock().unwrap();

        if let Some(cached) = cache.get(key) {
            // ヒットした場合、LRU順序を更新 (該当キーを一度削除して先頭に追加)
            if let Some(pos) = order.iter().position(|k| k == key) {
                order.remove(pos);
            }
            order.push_front(key.clone());
            return Ok(cached.clone());
        }

        // --- 2. キャッシュミス: 新しくパイプラインを生成 ---

        // --- バインドグループ0 (入力/出力テクスチャ) ---
        let mut bgl_entries_group0 = Vec::new();

        // Binding 0: 入力テクスチャの配列 (存在する場合)
        if key.input_texture_count > 0 {
            bgl_entries_group0.push(wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: core::num::NonZeroU32::new(key.input_texture_count as u32),
            });
        }

        // Binding 0 or 1: 出力テクスチャ (常に存在)
        // binding番号は、入力テクスチャの有無で変わる
        bgl_entries_group0.push(wgpu::BindGroupLayoutEntry {
            binding: if key.input_texture_count > 0 { 1 } else { 0 },
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba32Float,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });

        let bind_group_layout_0 =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("BGL Group 0 for {}", key.id)),
                    entries: &bgl_entries_group0,
                });

        // --- バインドグループ1 (Storageパラメータ) ---
        let mut bind_group_layouts = vec![bind_group_layout_0];
        if key.has_storage {
            let bind_group_layout_1 =
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some(&format!("BGL Group 1 for {}", key.id)),
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        }],
                    });
            bind_group_layouts.push(bind_group_layout_1);
        }

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("PL for {}", key.id)),
                bind_group_layouts: &bind_group_layouts.iter().collect::<Vec<_>>(),
                push_constant_ranges: &[],
            });

        let pipeline = Arc::new(self.device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some(&format!("Pipeline for {}", key.id)),
                layout: Some(&pipeline_layout),
                module: shader_module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        ));

        // CachedPipelineも複数のレイアウトを保持できるように更新が必要
        let new_item = CachedPipeline { pipeline };

        // --- 3. 新しいアイテムをキャッシュに保存 & LRU更新 ---
        cache.insert(key.clone(), new_item.clone());
        order.push_front(key.clone());

        // --- 4. キャッシュサイズを超えていたら古いものを削除 ---
        if order.len() > self.max_cache_size {
            if let Some(oldest_key) = order.pop_back() {
                cache.remove(&oldest_key);
            }
        }

        Ok(new_item)
    }
}
