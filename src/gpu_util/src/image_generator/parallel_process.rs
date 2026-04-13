use crate::{
    image_generate_builder::ImageGenerateBuilder,
    image_generator::{ImageGenerator, ProcessingState},
};
use anyhow::Result;
use futures::future::join_all;

pub async fn handle_parallel_step(
    generator: &ImageGenerator,
    state: &mut ProcessingState,
    pipelines: &[ImageGenerateBuilder],
    _step_index: usize,
) -> Result<ProcessingState> {
    // この並列ブロックに入る前の状態を、すべてのサブパイプラインの初期状態として使用する
    // StepOutputがClone可能である必要がある (Vec<f32>はclone可能、Arc<Buffer>もclone可能)
    let initial_state_for_sub_pipelines = state.clone();
    state.clear(); // 元のstateはクリアしておく

    let mut execution_futures = Vec::new();

    for sub_builder in pipelines {
        // generatorはClone可能なので、各非同期タスクに所有権を渡せる
        let sub_generator = generator.clone();
        let steps = sub_builder.steps.clone(); // Arc<Vec<PipelineStep>>なので軽量なクローン
        let initial_state = initial_state_for_sub_pipelines.clone();

        let future = async move { sub_generator.execute_pipeline(&steps, initial_state).await };
        execution_futures.push(future);
    }

    // すべてのサブパイプラインを並列に実行
    // 各ステップは自身でsubmitするため、エンコーダの収集は不要
    let results = join_all(execution_futures).await;

    // すべての結果を収集して、一つの状態にまとめる
    let mut combined_state = ProcessingState::new();

    for result in results {
        match result {
            Ok(mut sub_pipeline_state) => {
                combined_state.append(&mut sub_pipeline_state);
            }
            Err(e) => return Err(e), // エラーが発生した場合は即座に返す
        }
    }

    Ok(combined_state)
}
