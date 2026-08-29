// image_generate_builder.rs

use crate::compiled_func::{CompiledFunc, CompiledTextureFunc};
use crate::compiled_wgsl::CompiledWgsl;
use std::sync::Arc;
use uuid::Uuid;

/// パイプラインの各ステップを表すenum。
#[derive(Clone)]
pub enum PipelineStep {
    /// 単一のWGSLシェーダーを実行するステップ。
    Wgsl {
        /// このステップ固有の自動採番id(フレーム内でのテクスチャ使い回しの照合に使う)。
        id: String,
        wgsl: Arc<CompiledWgsl>,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
    /// 複数のWGSLシェーダーを並列に実行するステップ。
    Parallel {
        /// このステップ固有の自動採番id。IDはテクスチャごとではなくstepごとに
        /// 振られるべきという方針のもと、他のバリアントと同様に持つ
        /// (`id()`が`Parallel`だけ特例で`None`を返す、という状態を無くすため)。
        id: String,
        pipelines: Vec<ImageGenerateBuilder>,
    },
    /// CPUで関数を実行するステップ。
    CpuFunc {
        id: String,
        func: CompiledFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
    /// stateをすべてGPUテクスチャに変換したうえで関数を実行するステップ。
    TextureFunc {
        id: String,
        func: CompiledTextureFunc,
        params: Option<Vec<u8>>,
        output_width: u32,
        output_height: u32,
    },
    /// 同じフレーム内で`linked_id`が指すステップの出力をそのまま使い回すステップ。
    Linked {
        id: String,
        linked_id: String,
        output_width: u32,
        output_height: u32,
    },
}

impl PipelineStep {
    /// このステップ固有の自動採番id。全バリアントが持つため`Option`ではない。
    pub fn id(&self) -> &str {
        match self {
            PipelineStep::Wgsl { id, .. }
            | PipelineStep::Parallel { id, .. }
            | PipelineStep::CpuFunc { id, .. }
            | PipelineStep::TextureFunc { id, .. }
            | PipelineStep::Linked { id, .. } => id,
        }
    }

    /// このステップが`Linked`の場合、参照先の`linked_id`を返す。
    pub fn linked_id(&self) -> Option<&str> {
        match self {
            PipelineStep::Linked { linked_id, .. } => Some(linked_id),
            _ => None,
        }
    }
}

/// 1ステップぶんのidを再帰的に表現する型。leafは単一id文字列、`Parallel`は
/// 自分自身のidをキーとして、各ブランチの全ステップidリスト(`Vec<IdTree>`)を束ねる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdTree {
    Single(String),
    Parallel { id: String, branches: Vec<Vec<IdTree>> },
}

/// 画像生成パイプラインを構築するためのビルダー。

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

    /// このビルダーが持つ全ステップのidを、追加順の`IdTree`のリストとして返す
    /// (フィールドとして保持せず、都度`steps`から計算する)。空なら空の`Vec`。
    pub fn id_tree(&self) -> Vec<IdTree> {
        self.steps.iter().map(Self::step_to_id_tree).collect()
    }

    fn step_to_id_tree(step: &PipelineStep) -> IdTree {
        match step {
            PipelineStep::Parallel { id, pipelines } => IdTree::Parallel {
                id: id.clone(),
                branches: pipelines.iter().map(|p| p.id_tree()).collect(),
            },
            other => IdTree::Single(other.id().to_string()),
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
        let id = Uuid::new_v4().to_string();

        // Copy-on-Write: 新しいVecを作成して要素を追加
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::Wgsl {
            id,
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
        let id = Uuid::new_v4().to_string();

        // Copy-on-Write: 新しいVecを作成して要素を追加
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::Parallel { id, pipelines });

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
        let id = Uuid::new_v4().to_string();
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::CpuFunc {
            id,
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
        let id = Uuid::new_v4().to_string();
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::TextureFunc {
            id,
            func,
            params,
            output_width,
            output_height,
        });

        Self {
            steps: Arc::new(new_steps),
        }
    }

    /// 実際には計算を行わず、同じフレーム内で`linked_id`が指す既存ステップの出力を
    /// そのまま使い回すステップを追加します。
    ///
    /// # Arguments
    ///
    /// * `linked_id` - 使い回したい出力を持つステップの自動採番id(`get_id_tree`で取得したもの)。
    /// * `output_width` - このステップの出力画像の幅(使い回す実データと一致している必要がある)。
    /// * `output_height` - このステップの出力画像の高さ。
    pub fn add_linked(self, linked_id: String, output_width: u32, output_height: u32) -> Self {
        if !self.steps.is_empty() {
            // Linkedは直前の状態を無視してリンク先のテクスチャに差し替えるため、
            // それより前のステップの出力は捨てられる。呼び出し側の設計ミスの
            // サインであることが多いため警告する。
            log::warn!(
                "add_linked called on a builder with {} preceding step(s); their output will be discarded",
                self.steps.len()
            );
        }

        let id = Uuid::new_v4().to_string();
        let mut new_steps = (*self.steps).clone();
        new_steps.push(PipelineStep::Linked {
            id,
            linked_id,
            output_width,
            output_height,
        });

        Self {
            steps: Arc::new(new_steps),
        }
    }
}
