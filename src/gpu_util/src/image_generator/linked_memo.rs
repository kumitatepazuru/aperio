// image_generator/linked_memo.rs
use crate::image_generate_builder::PipelineStep;
use crate::image_generator::StepOutput;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// フレーム生成の時にlinked_id経由でテクスチャを使いまわすときに使う一時保存の構造体
#[derive(Clone)]
pub struct LinkedMemo {
    // new時に自動生成されるtarget ids
    target_ids: Arc<HashSet<String>>,
    // linked_id -> そのステップが実際に生成した出力(まるごと)のマップ。
    // 通常のステップなら1要素、Parallelならブランチ数ぶんの要素になる。
    store: Arc<Mutex<HashMap<String, Vec<StepOutput>>>>,
}

impl LinkedMemo {
    pub(crate) fn new(steps: &[PipelineStep]) -> Self {
        let mut target_ids = HashSet::new();
        Self::collect_linked_ids(steps, &mut target_ids);

        Self {
            target_ids: Arc::new(target_ids),
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// `steps`(`Parallel`内のサブビルダーも再帰的に)から、`Linked`ステップが参照している
    /// `linked_id`をすべて集める。
    fn collect_linked_ids(steps: &[PipelineStep], out: &mut HashSet<String>) {
        for step in steps {
            if let Some(linked_id) = step.linked_id() {
                out.insert(linked_id.to_string());
            }
            if let PipelineStep::Parallel { pipelines, .. } = step {
                for pipeline in pipelines {
                    Self::collect_linked_ids(&pipeline.steps, out);
                }
            }
        }
    }

    /// `steps`(ネストした`Parallel`も含め再帰的に)の中で、`target_ids`に含まれる
    /// (=どこかから`linked_id`として参照されている)ステップidを集める。
    /// `handle_parallel_step`が兄弟ブランチの実行順序を決めるために使う。
    pub(crate) fn provided_ids_in(&self, steps: &[PipelineStep]) -> HashSet<String> {
        let mut out = HashSet::new();
        Self::collect_provided_ids(steps, &self.target_ids, &mut out);
        out
    }

    fn collect_provided_ids(steps: &[PipelineStep], target_ids: &HashSet<String>, out: &mut HashSet<String>) {
        for step in steps {
            if target_ids.contains(step.id()) {
                out.insert(step.id().to_string());
            }
            if let PipelineStep::Parallel { pipelines, .. } = step {
                for pipeline in pipelines {
                    Self::collect_provided_ids(&pipeline.steps, target_ids, out);
                }
            }
        }
    }

    /// `steps`(ネストした`Parallel`も含め再帰的に)の中の`Linked`ステップが
    /// 参照している`linked_id`を集める。
    pub(crate) fn required_ids_in(&self, steps: &[PipelineStep]) -> HashSet<String> {
        let mut out = HashSet::new();
        Self::collect_linked_ids(steps, &mut out);
        out
    }

    /// `id`が`Linked`から参照されている対象であれば、その出力(まるごと)を保存する。
    /// `Parallel`ステップの場合は`outputs`がブランチ数ぶんの複数要素になる。
    pub(crate) fn record_if_target(&self, id: &str, outputs: &[StepOutput]) {
        if self.target_ids.contains(id) {
            self.store
                .lock()
                .unwrap()
                .insert(id.to_string(), outputs.to_vec());
        }
    }

    /// `linked_id`が指す、既に計算済みの出力(まるごと)を取得する。
    pub(crate) fn resolve(&self, linked_id: &str) -> Result<Vec<StepOutput>> {
        self.store
            .lock()
            .unwrap()
            .get(linked_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Linked pipeline step: no cached output found for linked_id={linked_id}"
                )
            })
    }
}
