// compiled_func.rs

use anyhow::Result;
use std::sync::Arc;

// CPU関数への入力用のstruct
pub struct CpuInputImage<'a> {
    pub data: &'a [f32],
    pub width: u32,
    pub height: u32,
}

// CPU関数の出力用のstruct
pub struct CpuOutput {
    pub data: Vec<f32>,
    pub width: u32,
    pub height: u32,
}

// CPUで実行される関数の型エイリアス。
pub type CpuFunction = dyn Fn(&[CpuInputImage], Option<&[u8]>) -> Result<CpuOutput> + Send + Sync;

/// CPUで実行される関数と、GPUとのデータ転送設定を保持する構造体。
#[derive(Clone)]
pub struct CompiledFunc {
    pub func: Arc<CpuFunction>,
}

impl CompiledFunc {
    /// 新しいCompiledFuncインスタンスを作成します。
    ///
    /// # Arguments
    ///
    /// * `func` - 実行するCPU関数。Box<dyn Fn(...)>の形式で渡されます。
    ///
    /// # Returns
    ///
    /// * `Self` - CompiledFuncインスタンス。
    pub fn new(func: Box<CpuFunction>) -> Self {
        Self {
            func: Arc::from(func),
        }
    }
}
