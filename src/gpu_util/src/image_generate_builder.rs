// image_generate_builder.rs

use crate::compiled_func::{CompiledFunc, CompiledTextureFunc};
use crate::compiled_wgsl::CompiledWgsl;
use std::sync::Arc;

/// パイプラインの各ステップを表すenum。
#[derive(Clone)]
pub enum PipelineStep {
    /// 単一のWGSLシェーダーを実行するステップ。
    Wgsl {
        wgsl: Arc<CompiledWgsl>,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
    /// 複数のWGSLシェーダーを並列に実行するステップ。
    Parallel {
        pipelines: Vec<ImageGenerateBuilder>,
    },
    /// CPUで関数を実行するステップ。
    CpuFunc {
        func: CompiledFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
    /// stateをすべてGPUテクスチャに変換したうえで関数を実行するステップ。
    TextureFunc {
        func: CompiledTextureFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
}

/// 画像生成パイプラインを構築するためのビルダー。
///
/// `add`メソッドで処理ステップを直列に追加していきます。
/// パフォーマンス最適化のため、内部データをArcでラップして共有参照を使用。
#[derive(Clone)]
pub struct ImageGenerateBuilder {
    pub(crate) steps: Arc<Vec<PipelineStep>>,
}

impl ImageGenerateBuilder {
    /// 新しいImageGenerateBuilderインスタンスを作成します。
    pub fn new() -> Self {
        Self {
            steps: Arc::new(Vec::new()),
        }
    }

    /// WGSL処理ステップをパイプラインに追加します（直列実行）。
    ///
    /// # Arguments
    ///
    /// * `wgsl` - `CompiledWgsl`のArc参照。
    /// * `params` - シェーダーのStorage Bufferに渡すパラメータ。`bytemuck`でシリアライズされたバイト列を渡します。
    /// * `output_width` - このステップの出力画像の幅。
    /// * `output_height` - このステップの出力画像の高さ。
    pub fn add_wgsl(
        self,
        wgsl: CompiledWgsl,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    ) -> Self {
        let wgsl = Arc::new(wgsl);

        // Copy-on-Write: 新しいVecを作成して要素を追加
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::Wgsl {
            wgsl,
            params,
            output_width,
            output_height,
        });

        Self {
            steps: Arc::new(new_steps),
        }
    }

    /// 複数のWGSL処理ステップをパイプラインに追加します（並列実行）。
    ///
    /// # Arguments
    ///
    /// * `pipelines` - 並列実行するパイプラインの配列。
    pub fn add_parallel_wgsl(self, pipelines: Vec<ImageGenerateBuilder>) -> Self {
        // Copy-on-Write: 新しいVecを作成して要素を追加
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::Parallel { pipelines });

        Self {
            steps: Arc::new(new_steps),
        }
    }

    /// CPU関数処理ステップをパイプラインに追加します。
    ///
    /// # Arguments
    ///
    /// * `func` - `CompiledFunc`参照。
    /// * `params` - 関数に渡す任意のパラメータ。
    /// * `output_width` - このステップの出力画像の幅。
    /// * `output_height` - このステップの出力画像の高さ。
    pub fn add_func(
        self,
        func: CompiledFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    ) -> Self {
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::CpuFunc {
            func,
            params,
            output_width,
            output_height,
        });

        Self {
            steps: Arc::new(new_steps),
        }
    }

    /// GPUテクスチャ関数処理ステップをパイプラインに追加します。
    ///
    /// stateをすべてGPUテクスチャに変換したうえで関数を呼び出します。
    /// widthとheightは関数の戻り値によって決定されるため、add時には指定不要です。
    ///
    /// # Arguments
    ///
    /// * `func` - `CompiledTextureFunc`参照。
    /// * `params` - 関数に渡す任意のパラメータ。
    /// GPUテクスチャ関数処理ステップをパイプラインに追加します。
    ///
    /// # Arguments
    ///
    /// * `func` - `CompiledTextureFunc`参照。
    /// * `params` - 関数に渡す任意のパラメータ。
    /// * `output_width` - このステップの出力画像の幅。
    /// * `output_height` - このステップの出力画像の高さ。
    pub fn add_texture_func(
        self,
        func: CompiledTextureFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    ) -> Self {
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::TextureFunc {
            func,
            params,
            output_width,
            output_height,
        });

        Self {
            steps: Arc::new(new_steps),
        }
    }
}
