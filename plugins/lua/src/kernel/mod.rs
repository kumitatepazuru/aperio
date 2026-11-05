//! カーネル化: GpuFunction を含むループを検出し、反復を GPU スレッドに割り当てる計画を立てる。
//!
//! 流れ:
//! 1. [`detect`]: GpuFunction の呼び出しを含む文を探し、並列次元にできる数値 for の入れ子を決める
//!    (できなければ外側を CPU に残す / 単一スレッドのカーネルにする)。
//! 2. [`bt`]: カーネル本体の binding time(U: 反復不変 / I: 反復依存だが CPU で計算可 / P: GPU 専用)。
//! 3. [`deps`]: 並列化の判定(P の値の持ち越し、共有テーブルへの P の書き込み、break など)。
//! 4. [`classify`]: GPU コードに必要な値のうち CPU でしか計算できないものを、持ち出し(hoist)と
//!    再生での記録(record)に振り分け、型を決める。
//!
//! 計画([`Kernel`])は codegen(Slang / CPU 側のディスパッチ用ブロック)が使う。

pub mod bt;
pub mod classify;
pub mod deps;
pub mod detect;
pub mod types;

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::analysis::effects::SiteImpl;
use crate::expand::SiteTarget;
use crate::hir::*;

pub use bt::Bt;
pub use types::GpuTy;

/// 変数の参照(local またはグローバル)。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VarRef {
    Local(VarId),
    Global(String),
}

/// 式を識別するキー(ファイル・開始・終了バイト)。カーネル本体の中では一意になる。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprKey {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl ExprKey {
    pub fn of(e: &Expr) -> Self {
        Self {
            file: e.span.file,
            start: e.span.start,
            end: e.span.end,
        }
    }
}

/// 並列次元(数値 for)。外側から順に並ぶ。
#[derive(Clone, Debug)]
pub struct Dim {
    pub stmt: StmtId,
    pub var: VarId,
    pub start: Expr,
    pub limit: Expr,
    pub step: Option<Expr>,
}

/// GPU コードが読む、カーネルの外の値。
#[derive(Clone, Debug, PartialEq)]
pub enum CaptureSource {
    Var(VarRef),
    /// 外部値(`__lg_ext.name` / `${name}`)。
    External(String),
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub source: CaptureSource,
    pub ty: GpuTy,
    /// Slang 側の名前。
    pub name: String,
}

/// CPU でしか計算できない値(持ち出し or 記録)。
#[derive(Clone, Debug)]
pub struct CpuValue {
    pub expr: Expr,
    pub ty: GpuTy,
    /// 記録の場合: それを含む内側のループ(record_loops の添字)。
    pub record_loop: Option<usize>,
    /// 記録の場合: 反復ごとの記録ブロック(またはその内側ループの 1 反復分)の中の位置。
    pub offset: u32,
}

/// 記録を含む内側の(逐次の)数値 for。
#[derive(Clone, Debug)]
pub struct RecordLoop {
    pub stmt: StmtId,
    /// 1 反復あたりの記録数。
    pub stride: u32,
    /// 反復ごとの記録ブロックの中での開始位置。
    pub base: u32,
    /// 記録できる最大反復回数。
    pub max_iterations: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuRef {
    Hoist(usize),
    Record(usize),
}

#[derive(Clone, Debug)]
pub struct Kernel {
    pub id: u32,
    /// カーネルを含む関数。
    pub func: FuncId,
    /// ディスパッチ用ブロックに置き換える文(同じブロック内で連続)。
    pub stmts: Vec<StmtId>,
    /// 置き換える範囲(最初の文の先頭から最後の文の末尾まで)。
    pub span: Span,
    pub dims: Vec<Dim>,
    /// 1 反復で実行する文(最内の並列ループの本体。次元が無ければ置き換える文そのもの)。
    pub body: Block,
    /// 本体の local と、本体で代入される外側の変数の binding time。
    pub var_bt: HashMap<VarRef, Bt>,
    /// GPU 内の私有変数として扱う外側の変数(本体の外では読まれない)。
    pub privatized: HashSet<VarRef>,
    /// 本体で代入される(U / I の値で)外側の変数。値は CPU の再生で計算する。
    pub carried: HashSet<VarRef>,
    /// 本体で書き込まれる共有テーブル(外側で作られたテーブル)の式。GPU からは読めない。
    pub written_tables: HashSet<VarRef>,
    pub captures: IndexMap<String, Capture>,
    pub hoists: Vec<CpuValue>,
    pub records: Vec<CpuValue>,
    pub record_loops: Vec<RecordLoop>,
    /// 1 反復あたりの記録スロット数。
    pub slots_per_iteration: u32,
    /// CPU での再生が必要か(副作用のある U / I の文、記録、外側の変数の更新)。
    pub needs_replay: bool,
    pub cpu_refs: HashMap<ExprKey, CpuRef>,
    /// GPU で使う local の型(本体の local と私有化した変数)。
    pub var_types: HashMap<VarId, GpuTy>,
    pub privatized_globals: HashMap<String, GpuTy>,
    /// GPU に変換するユーザー関数(CpuFunction の生成関数を含む)。
    pub funcs: Vec<FuncId>,
    pub func_types: HashMap<FuncId, types::FuncSig>,
    /// GPU に変換する文(本体のうち CPU 専用の文は含まない)。
    pub gpu_stmts: HashSet<StmtId>,
    /// CPU の再生で実行する葉の文(副作用・外側の状態の更新・記録に必要な定義)。
    pub replay_leaves: HashSet<StmtId>,
    /// 記録する式を含む文(再生ではその位置で記録する値だけを評価する)。
    pub record_stmts: HashSet<StmtId>,
    /// 再生でも同じ構造で実行する条件付きの飛び越し(goto / break を含む if)。
    pub replay_controls: HashSet<StmtId>,
    /// GPU で評価する式の型。
    pub expr_types: HashMap<ExprKey, GpuTy>,
    /// 単一スレッドのカーネル(並列次元なし)。
    pub single_thread: bool,
}

/// プログラム全体のカーネル化の計画。
#[derive(Clone, Debug, Default)]
pub struct KernelPlan {
    pub kernels: Vec<Kernel>,
    /// 最終的な各 site の割り当て。
    pub site_targets: Vec<SiteTarget>,
}

/// Cpu と Gpu の両方が登録された名前の割り当てを決める。
///
/// 既定(Cpu)の割り当てでカーネル候補を探し、候補の本体の中で引数が反復(I)または
/// GPU の値(P)に依存する呼び出しは Gpu にする。それ以外は Cpu のまま。
pub fn decide_targets(
    program: &Program,
    registry: &crate::spec::Registry,
    resolver: &crate::analysis::resolve::Resolver,
) -> Vec<SiteTarget> {
    let mut targets = crate::expand::default_targets(program, registry);
    let dual: HashSet<SiteId> = program
        .sites
        .iter()
        .filter(|s| {
            let e = registry.get(&s.name).unwrap();
            e.cpu.is_some() && e.gpu.is_some()
        })
        .map(|s| s.id)
        .collect();
    if dual.is_empty() {
        return targets;
    }
    let mut scratch = crate::diag::Diagnostics::new();
    let (cands, gpu_funcs) = detect::Detector::new(program, resolver, &targets, &mut scratch).run();
    let mut to_gpu = Vec::new();
    for c in &cands {
        let dim_vars = c.dims.iter().map(|d| d.var).collect();
        let mut cx = bt::BtCx::new(program, resolver, &targets, dim_vars, &gpu_funcs);
        cx.result = c.bt.clone();
        deps::visit_reads(program, &c.body, &mut |e| {
            if let ExprKind::Spec(site) = e.kind {
                if dual.contains(&site) {
                    let args_bt = program
                        .site(site)
                        .args
                        .iter()
                        .fold(Bt::U, |a, x| a.max(cx.expr(x)));
                    if args_bt >= Bt::I {
                        to_gpu.push(site);
                    }
                }
            }
        });
    }
    for s in to_gpu {
        targets[s as usize] = SiteTarget::Gpu;
    }
    targets
}

/// カーネル化の計画を立てる。エラーは diags に出し、失敗したカーネルは含めない。
pub fn plan(
    inputs: &Inputs,
    resolver: &crate::analysis::resolve::Resolver,
    effects: &crate::analysis::effects::Effects,
    diags: &mut crate::diag::Diagnostics,
) -> KernelPlan {
    let (cands, gpu_funcs) =
        detect::Detector::new(inputs.program, resolver, inputs.targets, diags).run();
    let mut kernels = Vec::new();
    for c in cands {
        let id = kernels.len() as u32;
        let cls = classify::Classifier::new(
            inputs.program,
            resolver,
            effects,
            inputs,
            &gpu_funcs,
            diags,
            id,
            c,
        );
        if let Some(k) = cls.run() {
            kernels.push(k);
        }
    }
    KernelPlan {
        kernels,
        site_targets: inputs.targets.to_vec(),
    }
}

/// カーネル化の入力(expand 済みの情報)。
pub struct Inputs<'a> {
    pub program: &'a Program,
    pub targets: &'a [SiteTarget],
    pub site_impls: &'a [SiteImpl],
    pub options: &'a crate::spec::Options,
    pub gpu_returns: &'a [Vec<crate::types::TypeSpec>],
    pub gpu_params: &'a [Vec<crate::types::TypeSpec>],
    pub externals: &'a IndexMap<String, crate::types::TypeSpec>,
}
