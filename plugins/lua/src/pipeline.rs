//! 変換の前半(フロントエンド → 生成関数の展開 → カーネル化の計画)をまとめて実行する。

use std::path::Path;

use crate::analysis::consts::site_arg_info;
use crate::analysis::effects::{Effects, SiteImpl};
use crate::analysis::resolve::Resolver;
use crate::diag::Diagnostics;
use crate::expand::{self, SiteExpansion, SiteTarget};
use crate::frontend;
use crate::hir::Program;
use crate::kernel::{self, Inputs, KernelPlan};
use crate::spec::Registry;
use crate::types::TypeSpec;

/// 解析結果(codegen の入力)。
pub struct Analysis {
    pub program: Program,
    pub asts: Vec<full_moon::ast::Ast>,
    pub targets: Vec<SiteTarget>,
    pub expansions: Vec<SiteExpansion>,
    pub plan: KernelPlan,
}

/// 解析する。構文エラーなどで先へ進めないときは None(診断は diags に入る)。
pub fn analyze(
    source: &str,
    chunk_name: &str,
    base_dir: Option<&Path>,
    registry: &Registry,
    diags: &mut Diagnostics,
) -> Option<Analysis> {
    let fe = frontend::build(source, chunk_name, base_dir, registry, diags)?;
    let mut program = fe.program;
    if diags.has_errors() {
        return None;
    }

    let info = site_arg_info(&program);
    let targets = {
        let resolver = Resolver::new(&program);
        kernel::decide_targets(&program, registry, &resolver)
    };
    let expansions = expand::expand(&mut program, registry, &targets, &info, diags);

    let site_impls: Vec<SiteImpl> = expansions
        .iter()
        .map(|e| match e {
            SiteExpansion::Cpu(c) => SiteImpl::CpuFunc(c.func),
            SiteExpansion::Gpu(_) => SiteImpl::Gpu,
            SiteExpansion::Py => SiteImpl::Py,
            SiteExpansion::Failed => SiteImpl::Unknown,
        })
        .collect();
    let gpu_params: Vec<Vec<TypeSpec>> = expansions
        .iter()
        .map(|e| match e {
            SiteExpansion::Gpu(g) => g.params.clone(),
            _ => Vec::new(),
        })
        .collect();
    let gpu_returns: Vec<Vec<TypeSpec>> = expansions
        .iter()
        .map(|e| match e {
            SiteExpansion::Gpu(g) => g.returns.clone(),
            _ => Vec::new(),
        })
        .collect();

    let plan = {
        let resolver = Resolver::new(&program);
        let effects = Effects::new(&program, &resolver, &site_impls);
        let inputs = Inputs {
            program: &program,
            targets: &targets,
            site_impls: &site_impls,
            options: &registry.options,
            gpu_returns: &gpu_returns,
            gpu_params: &gpu_params,
            externals: &registry.externals,
        };
        kernel::plan(&inputs, &resolver, &effects, diags)
    };

    Some(Analysis {
        program,
        asts: fe.asts,
        targets,
        expansions,
        plan,
    })
}
