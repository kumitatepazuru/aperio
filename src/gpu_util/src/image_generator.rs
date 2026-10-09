pub(crate) mod blit;
pub mod cpu_func_process;
pub mod final_process;
pub mod layout;
pub(crate) mod linked_memo;
pub mod parallel_process;
pub mod shader_process;
pub mod texture_func_process;

pub(crate) use linked_memo::LinkedMemo;

#[cfg(target_os = "linux")]
use crate::texture_to_native::linux::{attach_texture_to_shared_texture, SharedTextureHandle};
#[cfg(target_os = "windows")]
use crate::texture_to_native::windows::{attach_texture_to_shared_texture, SharedTextureHandle};

use crate::{
    compiled_shader::compile_slang_to_spirv,
    image_generate_builder::{ImageGenerateBuilder, PipelineStep},
    image_generator::{
        cpu_func_process::handle_cpu_func_step,
        final_process::handle_final_process,
        layout::{InputArity, OutputKind, ShaderShape},
        parallel_process::handle_parallel_step,
        shader_process::handle_shader_step,
        texture_func_process::handle_texture_func_step,
    },
    image_pixel_format::ImagePixelFormat,
    resource_pool::{LruCache, ResourcePool},
    rhi::{self, BufferUsage, TextureFormat, TextureUsage},
};
use anyhow::{bail, Context, Result};
use std::sync::{Arc, Mutex};

const POST_PROCESS_SLANG: &str = include_str!("shaders/post_process.slang");

// テクスチャキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct TextureCacheKey {
    width: u32,
    height: u32,
    format: TextureFormat,
    usage: TextureUsage,
}

// バッファキャッシュのキーとなる構造体
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub(crate) struct BufferCacheKey {
    size: u64,
    usage: BufferUsage,
}

/// パイプラインの各ステップの単一の出力を表すenum。
/// データがGPU上にあるか、CPU上にあるかを示します。
#[derive(Clone, Debug)]
pub enum StepOutput {
    Gpu {
        texture: rhi::Texture,
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

/// GPU RHIデバイスを管理し、画像生成パイプラインを実行するクラス。
#[derive(Clone)]
pub struct ImageGenerator {
    pub device: rhi::Device,
    /// このデバイスで確保できる2Dテクスチャの最大辺長（px）。
    /// キャンバスを広げるエフェクトが拡張量を切り詰める上限として使う。
    pub maximum_texture_size: u32,
    /// パイプライン内部のワーキングテクスチャのフォーマット。
    image_format: ImagePixelFormat,

    /// 後処理(f32 RGBA -> u32 RRGGBBAA)用の固定パイプライン。
    pub(crate) post_process_pipeline: rhi::ComputePipeline,

    // パイプラインキャッシュ（LRU、共有読み取り専用）
    pipeline_cache: Arc<Mutex<LruCache<String, rhi::ComputePipeline>>>,

    // テクスチャリソースプール
    // 同一キーに対して複数インスタンスを管理し、並列パイプラインでの競合を防ぐ
    texture_pool: Arc<Mutex<ResourcePool<TextureCacheKey, rhi::Texture>>>,

    // バッファリソースプール
    buffer_pool: Arc<Mutex<ResourcePool<BufferCacheKey, rhi::Buffer>>>,

    // 共有テクスチャへの書き出し(blit)用の遅延生成リソース
    pub(crate) blit: Arc<Mutex<blit::BlitResources>>,
}

impl ImageGenerator {
    /// 新しいImageGeneratorインスタンスを作成します。
    pub fn new(format: ImagePixelFormat) -> Result<Self> {
        let device = rhi::Device::new()?;
        let maximum_texture_size = device.maximum_texture_size();

        // 後処理パイプライン: 入力テクスチャ(set 0) -> u32ストレージバッファ(set 1)。
        let post_process_spirv = compile_slang_to_spirv(
            "post_process",
            "post_process.slang",
            POST_PROCESS_SLANG,
            "main",
            &[],
            &[],
        )
        .context("Failed to compile the built-in post_process Slang shader")?;
        let post_process_pipeline = device
            .create_compute_pipeline(
                &post_process_spirv,
                "main",
                &ShaderShape {
                    input_arity: InputArity::Fixed,
                    input_count: 1,
                    has_sampler: false,
                    has_params: false,
                    output: OutputKind::Buffer,
                }
                .layout(),
            )
            .context("Failed to create the built-in post_process pipeline")?;

        Ok(Self {
            device,
            maximum_texture_size,
            image_format: format,

            post_process_pipeline,

            // パイプラインキャッシュの初期化
            pipeline_cache: Arc::new(Mutex::new(LruCache::new(100))),

            // テクスチャリソースプールの初期化（キーごとに最大10インスタンスを保持）
            texture_pool: Arc::new(Mutex::new(ResourcePool::new(10))),

            // バッファリソースプールの初期化
            buffer_pool: Arc::new(Mutex::new(ResourcePool::new(10))),

            blit: Arc::new(Mutex::new(blit::BlitResources::default())),
        })
    }

    /// パイプライン内部のワーキングテクスチャのフォーマットを返す
    pub fn image_format(&self) -> ImagePixelFormat {
        self.image_format
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
        format: impl Into<TextureFormat>,
        usage: TextureUsage,
        label: &str,
    ) -> Result<rhi::Texture> {
        let format = format.into();
        let key = TextureCacheKey {
            width,
            height,
            format,
            usage,
        };
        let device = self.device.clone();
        let label = label.to_owned();
        self.texture_pool.lock().unwrap().acquire(key, move || {
            device.create_texture(width, height, format, usage, &label)
        })
    }

    /// バッファを取得または作成するためのヘルパーメソッド。
    pub fn get_or_create_buffer(
        &self,
        size: u64,
        usage: BufferUsage,
        label: &str,
    ) -> Result<rhi::Buffer> {
        let key = BufferCacheKey { size, usage };
        let device = self.device.clone();
        let label = label.to_owned();
        self.buffer_pool
            .lock()
            .unwrap()
            .acquire(key, move || device.create_buffer(size, usage, true, &label))
    }

    /// 指定されたステップリストを、与えられた初期状態から実行する内部関数。
    /// 各ステップはそれぞれ自身のコマンドをキューにsubmitする。
    pub(crate) async fn execute_pipeline(
        &self,
        steps: &[PipelineStep],
        initial_state: ProcessingState,
        memo: &LinkedMemo,
    ) -> Result<ProcessingState> {
        let mut state = initial_state;

        for (i, step) in steps.iter().enumerate() {
            state = match step {
                PipelineStep::Slang {
                    shader: slang,
                    params,
                    output_width,
                    output_height,
                    ..
                } => handle_shader_step(
                    self,
                    &state,
                    slang,
                    params,
                    i,
                    *output_width,
                    *output_height,
                )?,
                PipelineStep::Parallel { pipelines, .. } => {
                    handle_parallel_step(self, &mut state, pipelines, i, memo).await?
                }
                PipelineStep::CpuFunc {
                    func,
                    params,
                    output_width,
                    output_height,
                    ..
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
                    ..
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
                PipelineStep::Linked { linked_id, .. } => memo.resolve(linked_id)?,
            };

            memo.record_if_target(step.id(), &state);
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

        let memo = LinkedMemo::new(&builder.steps);

        let final_state_vec = self
            .execute_pipeline(&builder.steps, Vec::new(), &memo)
            .await?;

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

        handle_final_process(self, final_state_vec).await
    }

    pub async fn generate_shared_texture(
        &self,
        builder: ImageGenerateBuilder,
        texture_handle: &SharedTextureHandle,
        format: &crate::SharedTextureFormat,
    ) -> Result<()> {
        let final_state_vec = self.generate(builder).await?;

        if let StepOutput::Gpu { texture, .. } = &final_state_vec[0] {
            // TODO:Linuxのdmabuf importとWindowsのD3D12-via-Vulkan importが実装され次第、実際に機能するようになる。
            // それまでは呼び出し先が明示的なエラーを返す。
            attach_texture_to_shared_texture(texture_handle, format, texture, self)?;
            return Ok(());
        }

        bail!("Final output is not a GPU texture.")
    }

    /// CompiledShaderから(キャッシュがあればそれを使って)実行可能なパイプラインを得る。
    ///
    /// ここでは形状の食い違いチェックとキャッシュの出し入れだけを行う。
    pub(crate) fn get_or_create_pipeline(
        &self,
        shader: &crate::compiled_shader::CompiledShader,
        input_count: u32,
        has_params: bool,
    ) -> Result<rhi::ComputePipeline> {
        let shape = ShaderShape {
            input_arity: shader.input_arity,
            input_count,
            has_sampler: shader.sampler.is_some(),
            has_params,
            output: OutputKind::Texture,
        };
        let layout = shape.layout();

        let mut cache = self.pipeline_cache.lock().unwrap();
        if let Some(cached) = cache.get(&shader.name) {
            if *cached.layout() != layout {
                bail!(
                    "Slang shader '{}' was first used with a different resource shape \
                     than this call ({} input texture(s), has_params={}). A compiled \
                     shader's resource shape must stay consistent across all uses.",
                    shader.name,
                    input_count,
                    has_params
                );
            }
            return Ok(cached);
        }

        let pipeline =
            self.device
                .create_compute_pipeline(&shader.spirv, &shader.entry_point, &layout)?;

        cache.insert(shader.name.clone(), pipeline.clone());
        Ok(pipeline)
    }
}
