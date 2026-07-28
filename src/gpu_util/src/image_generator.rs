// image_generator.rs
pub mod cpu_func_process;
pub mod final_process;
pub mod parallel_process;
pub mod texture_func_process;
pub mod wgsl_process;

#[cfg(target_os = "linux")]
use crate::texture_to_native::linux::*;

#[cfg(target_os = "windows")]
use crate::texture_to_native::windows::*;

use crate::{
    common_pipeline::{ComputePipeline, RenderPipeline},
    image_generate_builder::{ImageGenerateBuilder, PipelineStep},
    image_generator::{
        cpu_func_process::handle_cpu_func_step, final_process::handle_final_process,
        parallel_process::handle_parallel_step, texture_func_process::handle_texture_func_step,
        wgsl_process::handle_wgsl_step,
    },
    resource_pool::{LruCache, ResourcePool},
    SharedTextureFormat,
};
use anyhow::{bail, Context, Result};
use std::sync::{Arc, Mutex};
use wgpu::{include_wgsl, Features};

// WGSLの後処理シェーダー（f32 RGBA -> u32 RRGGBBAA）
const POST_PROCESS_WGSL: wgpu::ShaderModuleDescriptor<'_> =
    include_wgsl!("shaders/post_process.wgsl");
const BLIT_F32_WGSL: wgpu::ShaderModuleDescriptor<'_> = include_wgsl!("shaders/blit_f32.wgsl");

// パイプラインキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct PipelineCacheKey {
    id: String,
    input_texture_count: usize,
    has_storage: bool,
    has_sampler: bool,
}

// テクスチャキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct TextureCacheKey {
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
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    /// このデバイスで確保できる2Dテクスチャの最大辺長（px）。
    /// キャンバスを広げるエフェクトが拡張量を切り詰める上限として使う。
    pub maximum_texture_size: u32,
    // 後処理用のパイプラインと関連リソース
    pub(crate) post_process_pipeline: ComputePipeline,
    pub(crate) blit_f32_to_f16_pipeline: RenderPipeline,
    pub(crate) blit_f32_to_bgra8_pipeline: RenderPipeline,
    pub(crate) blit_sampler: wgpu::Sampler,

    // --- パイプラインキャッシュ（LRU、共有読み取り専用）---
    pipeline_cache: Arc<Mutex<LruCache<PipelineCacheKey, CachedPipeline>>>,

    // --- テクスチャリソースプール ---
    // 同一キーに対して複数インスタンスを管理し、並列パイプラインでの競合を防ぐ
    texture_pool: Arc<Mutex<ResourcePool<TextureCacheKey, Arc<wgpu::Texture>>>>,

    // --- バッファリソースプール ---
    buffer_pool: Arc<Mutex<ResourcePool<BufferCacheKey, Arc<wgpu::Buffer>>>>,
}

impl ImageGenerator {
    /// 新しいImageGeneratorインスタンスを非同期で作成します。
    pub async fn new() -> Result<Self> {
        let backends = if cfg!(target_os = "windows") {
            wgpu::Backends::DX12
        } else if cfg!(target_os = "linux") {
            wgpu::Backends::VULKAN
        } else if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
        } else {
            bail!("Unsupported OS for ImageGenerator");
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::advanced_debugging(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .context("Failed to find an appropriate adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ImageGenerator Device"),
                required_features: Features::TEXTURE_BINDING_ARRAY
                    | Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
                    | Features::ADDRESS_MODE_CLAMP_TO_BORDER
                    | Features::FLOAT32_FILTERABLE,
                // | Features::SHADER_F16, // AMDやQualcommではなぜかunsupportedになる(動作はする) TODO: 対応方法を調査
                required_limits: wgpu::Limits {
                    max_binding_array_elements_per_shader_stage: 1000, // 必要に応じて調整
                    max_storage_buffer_binding_size: 134217728, // 128MB TODO: 環境によって数値が大きく異なるため動的変更ができるようにする
                    ..wgpu::Limits::defaults()
                },
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("Failed to create device")?;
        device.set_device_lost_callback(|reason, msg| {
            eprintln!("DEVICE LOST: {reason:?} msg={msg}");
        });
        let maximum_texture_size = device.limits().max_texture_dimension_2d;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // --- bufferでの後処理パイプラインの事前コンパイル ---
        let post_process_pipeline = ComputePipeline::new(
            device.as_ref(),
            POST_PROCESS_WGSL,
            &[
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
            "Post Process",
        );

        // --- Render pass用のサンプラー ---
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Blit Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // --- blit F32->F16 render pipeline ---
        let blit_f32_to_f16_pipeline = RenderPipeline::new(
            device.as_ref(),
            BLIT_F32_WGSL,
            &[
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
            ],
            wgpu::TextureFormat::Rgba16Float,
            "Blit F32->F16",
        );

        // --- blit F32->BGRA8unorm render pipeline ---
        let blit_f32_to_bgra8_pipeline = RenderPipeline::new(
            device.as_ref(),
            BLIT_F32_WGSL,
            &[
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
            ],
            wgpu::TextureFormat::Bgra8Unorm,
            "Blit F32->BGRA8unorm",
        );

        Ok(Self {
            device,
            queue,
            maximum_texture_size,
            post_process_pipeline,
            blit_f32_to_f16_pipeline,
            blit_f32_to_bgra8_pipeline,
            blit_sampler,

            // パイプラインキャッシュの初期化
            pipeline_cache: Arc::new(Mutex::new(LruCache::new(100))),

            // テクスチャリソースプールの初期化（キーごとに最大10インスタンスを保持）
            texture_pool: Arc::new(Mutex::new(ResourcePool::new(10))),

            // バッファリソースプールの初期化
            buffer_pool: Arc::new(Mutex::new(ResourcePool::new(10))),
        })
    }

    /// パイプラインキャッシュの最大サイズを設定します。
    pub fn set_max_cache_size(&mut self, size: usize) {
        self.pipeline_cache.lock().unwrap().set_max_size(size);
    }

    /// テクスチャを取得または作成するためのヘルパーメソッド。
    /// ResourcePool により、並列パイプライン内で同じキーに対して独立したインスタンスが返されます。
    pub fn get_or_create_texture(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        label: Option<&str>,
    ) -> Arc<wgpu::Texture> {
        let key = TextureCacheKey {
            width,
            height,
            format,
            usage,
        };
        let device = self.device.clone();
        let label = label.map(|s| s.to_owned());
        self.texture_pool.lock().unwrap().acquire(key, move || {
            Arc::new(device.create_texture(&wgpu::TextureDescriptor {
                label: label.as_deref(),
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
            }))
        })
    }

    /// バッファを取得または作成するためのヘルパーメソッド。
    pub fn get_or_create_buffer(
        &self,
        size: u64,
        usage: wgpu::BufferUsages,
        label: Option<&str>,
    ) -> Arc<wgpu::Buffer> {
        let key = BufferCacheKey { size, usage };
        let device = self.device.clone();
        let label = label.map(|s| s.to_owned());
        self.buffer_pool.lock().unwrap().acquire(key, move || {
            Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                label: label.as_deref(),
                size,
                usage,
                mapped_at_creation: false,
            }))
        })
    }

    /// 指定されたステップリストを、与えられた初期状態から実行する内部関数。
    /// 各ステップはそれぞれ自身のコマンドをキューにsubmitする。
    pub(crate) async fn execute_pipeline(
        &self,
        steps: &[PipelineStep],
        initial_state: ProcessingState,
    ) -> Result<ProcessingState> {
        let mut state = initial_state;

        for (i, step) in steps.iter().enumerate() {
            state = match step {
                PipelineStep::Wgsl {
                    wgsl,
                    params,
                    output_height,
                    output_width,
                } => {
                    handle_wgsl_step(self, &state, wgsl, params, i, *output_width, *output_height)?
                }
                PipelineStep::Parallel { pipelines } => {
                    handle_parallel_step(self, &mut state, pipelines, i).await?
                }
                PipelineStep::CpuFunc {
                    func,
                    params,
                    output_width,
                    output_height,
                } => {
                    handle_cpu_func_step(
                        self,
                        &mut state,
                        func,
                        params,
                        *output_width,
                        *output_height,
                    )
                    .await?
                }
                PipelineStep::TextureFunc {
                    func,
                    params,
                    output_width,
                    output_height,
                } => {
                    handle_texture_func_step(
                        self,
                        &mut state,
                        func,
                        params,
                        *output_width,
                        *output_height,
                    )
                    .await?
                }
            };
        }

        Ok(state)
    }

    /// ImageGenerateBuilderで構築されたパイプラインを実行し、画像を生成する。
    ///
    /// このメソッドは内部実装用のヘルパーとして意図的に非公開にしており、
    /// 外部からは `generate_buf` または `generate_shared_texture` を通してのみ  
    /// 画像生成機能を利用できるようにしている。
    async fn generate(&self, builder: ImageGenerateBuilder) -> Result<Vec<StepOutput>> {
        // フレーム開始時に前フレームで使用したリソースを解放して再利用可能にする
        self.texture_pool.lock().unwrap().reset_used();
        self.buffer_pool.lock().unwrap().reset_used();

        let final_state_vec = self.execute_pipeline(&builder.steps, Vec::new()).await?;

        // final_state_vecは単一の要素を持つはず
        if final_state_vec.len() != 1 {
            bail!(
                "Final processing state should have exactly one element, but has {}",
                final_state_vec.len()
            );
        }

        Ok(final_state_vec)
    }

    pub async fn generate_buf(&self, builder: ImageGenerateBuilder) -> Result<Vec<u8>> {
        let final_state_vec = self.generate(builder).await?;

        Ok(handle_final_process(self, final_state_vec).await?)
    }

    pub async fn generate_shared_texture(
        &self,
        builder: ImageGenerateBuilder,
        texture_handle: &SharedTextureHandle,
        format: &SharedTextureFormat,
    ) -> Result<()> {
        let final_state_vec = self.generate(builder).await?;

        if let StepOutput::Gpu { texture, .. } = &final_state_vec[0] {
            attach_texture_to_shared_texture(texture_handle, format, texture, self)?;
            return Ok(());
        }

        bail!("Final output is not a GPU texture or unsupported OS.");
    }

    pub(crate) fn get_or_create_pipeline(
        &self,
        key: &PipelineCacheKey,
        shader_module: &wgpu::ShaderModule,
    ) -> Result<CachedPipeline> {
        let mut cache = self.pipeline_cache.lock().unwrap();
        if let Some(cached) = cache.get(key) {
            return Ok(cached);
        }

        // キャッシュミス: 新しくパイプラインを生成
        let mut bgl_entries_group0 = Vec::new();

        if key.input_texture_count > 0 {
            bgl_entries_group0.push(wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: core::num::NonZeroU32::new(key.input_texture_count as u32),
            });
        }

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

        if key.has_sampler {
            bgl_entries_group0.push(wgpu::BindGroupLayoutEntry {
                binding: if key.input_texture_count > 0 { 2 } else { 1 },
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }

        let bind_group_layout_0 =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("BGL Group 0 for {}", key.id)),
                    entries: &bgl_entries_group0,
                });

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
                bind_group_layouts: &bind_group_layouts.iter().map(Some).collect::<Vec<_>>(),
                immediate_size: 0,
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

        let new_item = CachedPipeline { pipeline };
        cache.insert(key.clone(), new_item.clone());
        Ok(new_item)
    }
}
