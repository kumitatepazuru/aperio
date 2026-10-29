use crate::{
    image_generate_builder::ImageGenerateBuilder,
    image_generator::{ImageGenerator, LinkedMemo, ProcessingState},
};
use anyhow::Result;
use futures::future::join_all;
use std::collections::HashSet;

pub async fn handle_parallel_step(
    generator: &ImageGenerator,
    state: &mut ProcessingState,
    pipelines: &[ImageGenerateBuilder],
    _step_index: usize,
    memo: &LinkedMemo,
) -> Result<ProcessingState> {
    // この並列ブロックに入る前の状態を、すべてのサブパイプラインの初期状態として使用する
    // StepOutputがClone可能である必要がある (Vec<f32>はclone可能、Arc<Buffer>もclone可能)
    let initial_state_for_sub_pipelines = state.clone();
    state.clear(); // 元のstateはクリアしておく

    // 兄弟ブランチ間の提供/要求idを見て、提供側が要求側より先に実行されるよう
    // tierに分ける(同一tier内は依存が無いので並列実行してよい)。
    let tiers = compute_branch_execution_order(pipelines, memo)?;

    let mut results: Vec<Option<Result<ProcessingState>>> = (0..pipelines.len()).map(|_| None).collect();

    for tier in tiers {
        let mut execution_futures = Vec::new();

        for branch_index in tier {
            // generatorはClone可能なので、各非同期タスクに所有権を渡せる
            let sub_generator = generator.clone();
            let steps = pipelines[branch_index].steps.clone(); // Arc<Vec<PipelineStep>>なので軽量なクローン
            let initial_state = initial_state_for_sub_pipelines.clone();
            let memo = memo.clone(); // Arcフィールドのみなので軽量なクローン

            let future = async move {
                (
                    branch_index,
                    sub_generator.execute_pipeline(&steps, initial_state, &memo).await,
                )
            };
            execution_futures.push(future);
        }

        // 同一tier内のサブパイプラインを並列に実行
        // 各ステップは自身でsubmitするため、エンコーダの収集は不要
        for (branch_index, result) in join_all(execution_futures).await {
            results[branch_index] = Some(result);
        }
    }

    // すべての結果を、元のpipelines順(=呼び出し元から見た順序)にまとめる
    let mut combined_state = ProcessingState::new();

    for result in results {
        match result.expect("every branch must have been executed exactly once") {
            Ok(mut sub_pipeline_state) => {
                combined_state.append(&mut sub_pipeline_state);
            }
            Err(e) => return Err(e), // エラーが発生した場合は即座に返す
        }
    }

    Ok(combined_state)
}

/// 各ブランチが(再帰的に)提供するid・要求するidを`memo`から取得し、要求側が
/// 提供側より後のtierになるようKahnのアルゴリズムでトポロジカルソートする。
/// 循環(相互依存)が残った場合はエラーにする。
fn compute_branch_execution_order(
    pipelines: &[ImageGenerateBuilder],
    memo: &LinkedMemo,
) -> Result<Vec<Vec<usize>>> {
    let n = pipelines.len();
    let provides: Vec<HashSet<String>> =
        pipelines.iter().map(|p| memo.provided_ids_in(&p.steps)).collect();
    let requires: Vec<HashSet<String>> =
        pipelines.iter().map(|p| memo.required_ids_in(&p.steps)).collect();

    // deps[i] = ブランチiが(このParallelの兄弟の中で)依存しているブランチのindex集合。
    // 要求idが同レベルのどの兄弟にも提供されていなければ、外側のスコープで
    // 既に解決済みのはずなので依存関係としては扱わない(実際に未解決なら
    // 今まで通りresolve()が自然にエラーを出す)。
    let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for i in 0..n {
        for id in &requires[i] {
            for j in 0..n {
                if i != j && provides[j].contains(id) {
                    deps[i].insert(j);
                }
            }
        }
    }

    let mut remaining: HashSet<usize> = (0..n).collect();
    let mut tiers = Vec::new();
    while !remaining.is_empty() {
        let ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|i| deps[*i].iter().all(|d| !remaining.contains(d)))
            .collect();
        if ready.is_empty() {
            anyhow::bail!("Circular dependency detected between parallel branches via linked_id references");
        }
        for r in &ready {
            remaining.remove(r);
        }
        tiers.push(ready);
    }
    Ok(tiers)
}
